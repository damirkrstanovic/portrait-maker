//! Read-only game destination discovery.  This module intentionally never creates game folders.

mod platform;
mod steam;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::Result;
use crate::types::{Destination, DestinationOrigin, DestinationState, DiscoveryReport, Game};

pub use platform::{DiscoveryEnvironment, DiscoveryPlatform};

pub fn discover_destinations(env: &DiscoveryEnvironment) -> Result<Vec<Destination>> {
    Ok(discover_report(env)?.destinations)
}

pub fn discover_report(env: &DiscoveryEnvironment) -> Result<DiscoveryReport> {
    let mut report = platform::native_candidates(env);
    let steam = steam::steam_candidates(env);
    report.destinations.extend(steam.destinations);
    report.warnings.extend(steam.warnings);
    report.destinations = deduplicate(report.destinations)?;
    Ok(report)
}

/// Resolve a user-selected Wine/Proton prefix without creating or importing anything.
pub fn resolve_prefix(prefix: &Path, game: Game) -> Result<Vec<Destination>> {
    let users = prefix.join("drive_c/users");
    let entries = match fs::read_dir(&users) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(error.into());
        }
        Err(error) => return Err(error.into()),
    };
    let mut destinations = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if is_template_user(&name) {
            continue;
        }
        if !is_directory(&entry.path())? {
            continue;
        }
        let root = entry
            .path()
            .join("AppData/LocalLow")
            .join(game.windows_suffix());
        destinations.push(destination_for_game_root(
            root,
            game,
            DestinationOrigin::Manual,
            vec![format!("Manual compatibility prefix user: {name}")],
        )?);
    }
    deduplicate(destinations)
}

/// Validate a manually selected Portraits directory (or game-data root) read-only.
pub fn validate_destination(path: &Path, game: Game) -> Result<Destination> {
    let game_root = if path.file_name().is_some_and(|name| name == "Portraits") {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    destination_for_game_root(
        game_root,
        game,
        DestinationOrigin::Manual,
        vec!["Manually selected destination".into()],
    )
}

pub(crate) fn destination_for_game_root(
    game_root: PathBuf,
    game: Game,
    origin: DestinationOrigin,
    mut evidence: Vec<String>,
) -> Result<Destination> {
    let portraits = game_root.join("Portraits");
    let state = if is_directory(&portraits)? {
        DestinationState::Existing
    } else if is_directory(&game_root)? {
        evidence.push("Game data exists but its Portraits folder is absent.".into());
        DestinationState::MissingPortraits
    } else {
        evidence.push("Game data has not been initialized at this location.".into());
        DestinationState::Uninitialized
    };
    Ok(Destination {
        id: Uuid::new_v4(),
        game,
        name: format!("{} — {}", game.display_name(), portraits.display()),
        path: portraits,
        origin,
        state,
        evidence,
    })
}

fn is_directory(path: &Path) -> Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_dir()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn deduplicate(candidates: Vec<Destination>) -> Result<Vec<Destination>> {
    let mut output: HashMap<(Game, PathBuf), Destination> = HashMap::new();
    for candidate in candidates {
        let key_path = candidate
            .path
            .canonicalize()
            .unwrap_or_else(|_| candidate.path.clone());
        let key = (candidate.game, key_path);
        if let Some(saved) = output.get_mut(&key) {
            for evidence in candidate.evidence {
                if !saved.evidence.contains(&evidence) {
                    saved.evidence.push(evidence);
                }
            }
        } else {
            output.insert(key, candidate);
        }
    }
    let mut output = output.into_values().collect::<Vec<_>>();
    output.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(output)
}

fn is_template_user(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "public" | "default" | "default user" | "all users"
    )
}
