use std::fs;
use std::path::{Path, PathBuf};

use keyvalues_parser::{Parser, Value};

use crate::types::{Destination, DestinationOrigin, Game};

use super::{DiscoveryEnvironment, deduplicate, resolve_prefix};

pub(crate) struct SteamDiscovery {
    pub destinations: Vec<Destination>,
    pub warnings: Vec<String>,
}

pub(crate) fn steam_candidates(env: &DiscoveryEnvironment) -> SteamDiscovery {
    let mut candidates = Vec::new();
    let mut warnings = Vec::new();
    for root in dedup_roots(&env.steam_roots) {
        for library in libraries(&root, &mut warnings) {
            for game in [Game::Kingmaker, Game::Wotr] {
                let manifest = manifest_evidence(&library, game);
                let prefix = library
                    .join("steamapps/compatdata")
                    .join(game.steam_id())
                    .join("pfx");
                match fs::metadata(&prefix) {
                    Ok(metadata) if metadata.is_dir() => {
                        if let Ok(mut resolved) = resolve_prefix(&prefix, game) {
                            for destination in &mut resolved {
                                destination.origin = DestinationOrigin::Steam;
                                destination
                                    .evidence
                                    .push(format!("Steam library: {}", library.display()));
                                destination
                                    .evidence
                                    .push(format!("Steam root: {}", root.display()));
                                destination.evidence.push(manifest.clone());
                            }
                            candidates.extend(resolved);
                        } else {
                            warnings.push(format!(
                                "Cannot read Steam compatibility prefix: {}",
                                prefix.display()
                            ));
                        }
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => {
                        warnings.push(format!(
                            "Cannot inspect Steam compatibility prefix: {}",
                            prefix.display()
                        ));
                    }
                }
            }
        }
    }
    SteamDiscovery {
        destinations: deduplicate(candidates).unwrap_or_default(),
        warnings,
    }
}

fn dedup_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for root in roots {
        let identity = root.canonicalize().unwrap_or_else(|_| root.clone());
        if !unique.iter().any(|known: &PathBuf| {
            known.canonicalize().unwrap_or_else(|_| known.clone()) == identity
        }) {
            unique.push(root.clone());
        }
    }
    unique
}

fn libraries(root: &Path, warnings: &mut Vec<String>) -> Vec<PathBuf> {
    let mut output = vec![root.to_path_buf()];
    let path = root.join("steamapps/libraryfolders.vdf");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return output,
        Err(_) => {
            warnings.push(format!(
                "Cannot read Steam library list: {}",
                path.display()
            ));
            return output;
        }
    };
    let parser = Parser::new();
    let Ok(vdf) = parser.parse(&text) else {
        warnings.push(format!(
            "Steam library list is malformed: {}",
            path.display()
        ));
        return output;
    };
    let Some(libraries) = vdf.value.get_obj() else {
        return output;
    };
    for values in libraries.values() {
        for value in values {
            let path = match value {
                Value::Str(value) => Some(PathBuf::from(value.as_ref())),
                Value::Obj(object) => object
                    .get("path")
                    .and_then(|values| values.first())
                    .and_then(Value::get_str)
                    .map(PathBuf::from),
            };
            if let Some(path) = path {
                output.push(path);
            }
        }
    }
    dedup_roots(&output)
}

fn manifest_evidence(library: &Path, game: Game) -> String {
    let manifest = library
        .join("steamapps")
        .join(format!("appmanifest_{}.acf", game.steam_id()));
    let Ok(text) = fs::read_to_string(manifest) else {
        return "Steam app manifest is absent or unreadable; retained prefix evidence.".into();
    };
    let Ok(vdf) = Parser::new().parse(&text) else {
        return "Steam app manifest could not be parsed; retained prefix evidence.".into();
    };
    if vdf
        .value
        .get_obj()
        .and_then(|object| object.get("appid"))
        .and_then(|values| values.first())
        .and_then(Value::get_str)
        == Some(game.steam_id())
    {
        "Steam app manifest confirms this game.".into()
    } else {
        "Steam app manifest does not confirm this game; retained prefix evidence.".into()
    }
}
