use std::path::PathBuf;

use crate::types::{DestinationOrigin, DestinationState, DiscoveryReport, Game};

use super::destination_for_game_root;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryPlatform {
    Linux,
    Windows,
    Macos,
}

#[derive(Debug, Clone)]
pub struct DiscoveryEnvironment {
    pub platform: DiscoveryPlatform,
    pub home: PathBuf,
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_data_home: Option<PathBuf>,
    pub steam_roots: Vec<PathBuf>,
    pub windows_local_low: Option<PathBuf>,
}

impl DiscoveryEnvironment {
    #[must_use]
    pub fn production() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        #[cfg(target_os = "windows")]
        let platform = DiscoveryPlatform::Windows;
        #[cfg(target_os = "macos")]
        let platform = DiscoveryPlatform::Macos;
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        let platform = DiscoveryPlatform::Linux;
        let xdg_config_home = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
        let xdg_data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
        let steam_roots = match platform {
            DiscoveryPlatform::Linux => {
                let mut roots = vec![
                    home.join(".local/share/Steam"),
                    home.join(".steam/steam"),
                    home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
                ];
                if let Some(data) = &xdg_data_home {
                    roots.push(data.join("Steam"));
                }
                roots
            }
            DiscoveryPlatform::Macos => vec![home.join("Library/Application Support/Steam")],
            DiscoveryPlatform::Windows => windows_steam_roots(),
        };
        Self {
            platform,
            home,
            xdg_config_home,
            xdg_data_home,
            steam_roots,
            windows_local_low: windows_local_low(),
        }
    }
}

pub(crate) fn native_candidates(env: &DiscoveryEnvironment) -> DiscoveryReport {
    let mut warnings = Vec::new();
    let mut inspect = |root: PathBuf, game, evidence: &str| match destination_for_game_root(
        root.clone(),
        game,
        DestinationOrigin::Steam,
        vec![evidence.into()],
    ) {
        Ok(destination) => Some(destination),
        Err(error) => {
            warnings.push(format!(
                "Cannot inspect native game data: {}: {error}",
                root.display()
            ));
            None
        }
    };
    let destinations = match env.platform {
        DiscoveryPlatform::Linux => {
            let mut roots = vec![env.home.join(".config")];
            if let Some(root) = &env.xdg_config_home {
                roots.push(root.clone());
            }
            roots
                .into_iter()
                .filter_map(|config| {
                    inspect(
                        config.join("unity3d/Owlcat Games/Pathfinder Kingmaker"),
                        Game::Kingmaker,
                        "Linux native Kingmaker compatibility path",
                    )
                })
                .collect()
        }
        DiscoveryPlatform::Windows => env
            .windows_local_low
            .iter()
            .flat_map(|root| {
                [Game::Kingmaker, Game::Wotr]
                    .into_iter()
                    .map(move |game| (root.join(game.windows_suffix()), game))
            })
            .filter_map(|(root, game)| {
                inspect(root, game, "Windows LocalAppDataLow compatibility path")
            })
            .collect(),
        DiscoveryPlatform::Macos => [
            "Owlcat Games/Pathfinder Kingmaker",
            "unity.Owlcat Games.Pathfinder Kingmaker",
            "unity3d/Owlcat Games/Pathfinder Kingmaker",
        ]
        .into_iter()
        .filter_map(|suffix| {
            inspect(
                env.home.join("Library/Application Support").join(suffix),
                Game::Kingmaker,
                "macOS Kingmaker candidate; retained only when present",
            )
        })
        .filter(|destination| destination.state != DestinationState::Uninitialized)
        .collect(),
    };
    DiscoveryReport {
        destinations,
        warnings,
    }
}

#[cfg(target_os = "windows")]
fn windows_steam_roots() -> Vec<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY};
    let current = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Valve\\Steam")
        .ok()
        .and_then(|key| key.get_value::<String, _>("SteamPath").ok())
        .map(PathBuf::from);
    let machine = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags("SOFTWARE\\Valve\\Steam", KEY_READ | KEY_WOW64_32KEY)
        .ok()
        .and_then(|key| key.get_value::<String, _>("InstallPath").ok())
        .map(PathBuf::from);
    current.into_iter().chain(machine).collect()
}
#[cfg(not(target_os = "windows"))]
fn windows_steam_roots() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(target_os = "windows")]
fn windows_local_low() -> Option<PathBuf> {
    known_folders::get_known_folder_path(known_folders::KnownFolder::LocalAppDataLow)
}
#[cfg(not(target_os = "windows"))]
fn windows_local_low() -> Option<PathBuf> {
    None
}
