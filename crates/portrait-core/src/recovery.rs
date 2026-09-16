use std::fs;
use std::path::Path;

use rusqlite::Connection;
use serde::Deserialize;
use uuid::Uuid;

use crate::Result;
use crate::trash::{ensure_managed_parent, remove_directory_if_present};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportIntent {
    portrait_id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PurgeIntent {
    ids: Vec<Uuid>,
}

pub(crate) fn recover_import_operations(root: &Path, connection: &Connection) -> Result<()> {
    crate::export::journal::recover_exports(root, connection)?;
    recover_extraction_staging(root)?;
    let mut statement = connection
        .prepare("SELECT operation_id, state_json FROM operation_state WHERE kind = 'import'")?;
    let records = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for (operation_id, state_json) in records {
        let Ok(operation_id) = Uuid::parse_str(&operation_id) else {
            continue;
        };
        let Ok(intent) = serde_json::from_str::<ImportIntent>(&state_json) else {
            continue;
        };
        let committed = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM portraits WHERE id = ?1)",
            [intent.portrait_id.to_string()],
            |row| row.get::<_, bool>(0),
        )?;
        if !committed {
            remove_if_present(&root.join("staging").join(format!("import-{operation_id}")))?;
            remove_if_present(&root.join("portraits").join(intent.portrait_id.to_string()))?;
        }
        connection.execute(
            "DELETE FROM operation_state WHERE operation_id = ?1",
            [operation_id.to_string()],
        )?;
    }
    recover_purge_operations(root, connection)
        .map_err(|error| crate::CoreError::Recovery(error.to_string()))?;
    Ok(())
}

fn recover_purge_operations(root: &Path, connection: &Connection) -> Result<()> {
    let mut statement = connection
        .prepare("SELECT operation_id, state_json FROM operation_state WHERE kind = 'purge'")?;
    let records = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    for (operation_id, state_json) in records {
        let operation_id = Uuid::parse_str(&operation_id).map_err(|error| {
            crate::CoreError::Recovery(format!("purge journal has invalid operation ID: {error}"))
        })?;
        let intent = serde_json::from_str::<PurgeIntent>(&state_json).map_err(|error| {
            crate::CoreError::Recovery(format!(
                "purge journal {operation_id} is malformed: {error}"
            ))
        })?;
        if intent.ids.is_empty()
            || intent
                .ids
                .iter()
                .any(|id| intent.ids.iter().filter(|other| *other == id).count() != 1)
        {
            return Err(crate::CoreError::Recovery(format!(
                "purge journal {operation_id} has an empty or duplicate portrait ID list"
            )));
        }
        recover_purge_operation(root, connection, operation_id, &intent.ids)?;
    }
    Ok(())
}

pub(crate) fn recover_purge_operation(
    root: &Path,
    connection: &Connection,
    operation_id: Uuid,
    ids: &[Uuid],
) -> Result<()> {
    ensure_managed_parent(&root.join("portraits"))?;
    ensure_managed_parent(&root.join("staging"))?;
    let staging = root.join("staging").join(format!("purge-{operation_id}"));
    let staging_exists = match fs::symlink_metadata(&staging) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => true,
        Ok(_) => return Err(crate::CoreError::UnsafeManagedDirectory),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    let committed: bool = connection.query_row(
        &format!(
            "SELECT NOT EXISTS(SELECT 1 FROM portraits WHERE id IN ({}))",
            placeholders(ids.len())
        ),
        rusqlite::params_from_iter(ids.iter().map(|id| id.to_string())),
        |row| row.get(0),
    )?;
    if !committed {
        if !staging_exists {
            return Err(crate::CoreError::Recovery(format!(
                "uncommitted purge journal {operation_id} is missing its staging directory"
            )));
        }
        for id in ids {
            let staged = staging.join(id.to_string());
            let final_path = root.join("portraits").join(id.to_string());
            match fs::symlink_metadata(&staged) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                    if fs::symlink_metadata(&final_path).is_ok() {
                        return Err(crate::CoreError::UnsafeManagedDirectory);
                    }
                    fs::rename(staged, final_path)?;
                }
                Ok(_) => return Err(crate::CoreError::UnsafeManagedDirectory),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    if staging_exists {
        remove_directory_if_present(&staging)?;
    }
    connection.execute(
        "DELETE FROM operation_state WHERE operation_id = ?1 AND kind = 'purge'",
        [operation_id.to_string()],
    )?;
    Ok(())
}

fn recover_extraction_staging(root: &Path) -> Result<()> {
    let staging = root.join("staging");
    for entry in fs::read_dir(staging)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(id) = name.strip_prefix("extract-") else {
            continue;
        };
        if file_type.is_dir() && !file_type.is_symlink() && Uuid::parse_str(id).is_ok() {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

fn remove_if_present(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path)?,
        Ok(_) => fs::remove_file(path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
}
