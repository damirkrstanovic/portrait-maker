use std::fs;

use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use uuid::Uuid;

use crate::catalog::bump_catalog_revision;
use crate::import::JobContext;
use crate::{CoreError, Library, Result};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PurgeIntent {
    ids: Vec<Uuid>,
    committed: bool,
}

pub fn trash_portraits(library: &mut Library, ids: &[Uuid]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let transaction = library.connection().unchecked_transaction()?;
    let placeholders = placeholders(ids.len());
    transaction.execute(
        &format!("UPDATE portraits SET trashed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id IN ({placeholders}) AND trashed_at IS NULL"),
        rusqlite::params_from_iter(ids.iter().map(|id| id.to_string())),
    )?;
    transaction.execute(
        &format!("DELETE FROM selection WHERE portrait_id IN ({placeholders})"),
        rusqlite::params_from_iter(ids.iter().map(|id| id.to_string())),
    )?;
    bump_catalog_revision(&transaction)?;
    transaction.commit()?;
    Ok(())
}

pub fn restore_portraits(library: &mut Library, ids: &[Uuid]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let transaction = library.connection().unchecked_transaction()?;
    let placeholders = placeholders(ids.len());
    transaction.execute(
        &format!("UPDATE portraits SET trashed_at = NULL WHERE id IN ({placeholders}) AND trashed_at IS NOT NULL"),
        rusqlite::params_from_iter(ids.iter().map(|id| id.to_string())),
    )?;
    bump_catalog_revision(&transaction)?;
    transaction.commit()?;
    Ok(())
}

/// Permanently removes only still-trashed records and their managed asset directories.
///
/// A durable journal is written before any asset move. Startup recovery derives all paths
/// from the library root and UUIDs in that journal; it never consumes a caller path.
pub fn purge_portraits(library: &mut Library, ids: &[Uuid], job: &JobContext) -> Result<u64> {
    if job.is_cancelled() || ids.is_empty() {
        return if job.is_cancelled() {
            Err(CoreError::Cancelled)
        } else {
            Ok(0)
        };
    }
    let ids = still_trashed_ids(library, ids)?;
    if ids.is_empty() {
        return Ok(0);
    }
    ensure_managed_parent(library.root().join("portraits").as_path())?;
    ensure_managed_parent(library.root().join("staging").as_path())?;

    let operation_id = Uuid::new_v4();
    let staging_root = library
        .root()
        .join("staging")
        .join(format!("purge-{operation_id}"));
    fs::create_dir(&staging_root)?;
    library.connection().execute(
        "INSERT INTO operation_state (operation_id, kind, state_json) VALUES (?1, 'purge', ?2)",
        params![
            operation_id.to_string(),
            serde_json::to_string(&PurgeIntent {
                ids: ids.clone(),
                committed: false
            })?
        ],
    )?;

    let result = (|| {
        for (index, id) in ids.iter().enumerate() {
            if job.is_cancelled() {
                return Err(CoreError::Cancelled);
            }
            move_asset_directory_to_staging(library, *id, &staging_root)?;
            job.report_progress((index + 1) as u64, Some(ids.len() as u64));
        }
        if job.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        let transaction = library.connection().unchecked_transaction()?;
        for id in &ids {
            remove_search_document(&transaction, *id)?;
        }
        let placeholders = placeholders(ids.len());
        let deleted = transaction.execute(
            &format!(
                "DELETE FROM portraits WHERE id IN ({placeholders}) AND trashed_at IS NOT NULL"
            ),
            rusqlite::params_from_iter(ids.iter().map(|id| id.to_string())),
        )?;
        bump_catalog_revision(&transaction)?;
        transaction.commit()?;
        library.connection().execute(
            "UPDATE operation_state SET state_json = ?1 WHERE operation_id = ?2",
            params![
                serde_json::to_string(&PurgeIntent {
                    ids: ids.clone(),
                    committed: true
                })?,
                operation_id.to_string()
            ],
        )?;
        remove_directory_if_present(&staging_root)?;
        library.connection().execute(
            "DELETE FROM operation_state WHERE operation_id = ?1",
            [operation_id.to_string()],
        )?;
        Ok(deleted as u64)
    })();

    match result {
        Ok(deleted) => Ok(deleted),
        Err(operation_error) => match crate::recovery::recover_purge_operation(
            library.root(),
            library.connection(),
            operation_id,
            &ids,
        ) {
            Ok(()) => Err(operation_error),
            Err(recovery_error) => Err(CoreError::Recovery(format!(
                "purge rollback failed after {operation_error}: {recovery_error}"
            ))),
        },
    }
}

fn still_trashed_ids(library: &Library, ids: &[Uuid]) -> Result<Vec<Uuid>> {
    let mut result = Vec::new();
    for id in ids {
        let present: bool = library.connection().query_row(
            "SELECT EXISTS(SELECT 1 FROM portraits WHERE id = ?1 AND trashed_at IS NOT NULL)",
            [id.to_string()],
            |row| row.get(0),
        )?;
        if present && !result.contains(id) {
            result.push(*id);
        }
    }
    Ok(result)
}

fn move_asset_directory_to_staging(
    library: &Library,
    id: Uuid,
    staging_root: &std::path::Path,
) -> Result<()> {
    let source = library.root().join("portraits").join(id.to_string());
    let destination = staging_root.join(id.to_string());
    match fs::symlink_metadata(&source) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::rename(source, destination)?
        }
        Ok(_) => return Err(CoreError::UnsafeManagedDirectory),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn remove_search_document(transaction: &rusqlite::Transaction<'_>, id: Uuid) -> Result<()> {
    let document = transaction.query_row(
        "SELECT id, name, original_folder, source_name, labels, description FROM search_documents WHERE portrait_id = ?1",
        [id.to_string()],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?)),
    ).optional()?;
    if let Some((rowid, name, folder, source, labels, description)) = document {
        transaction.execute(
            "INSERT INTO search_index(search_index, rowid, name, original_folder, source_name, labels, description) VALUES('delete', ?1, ?2, ?3, ?4, ?5, ?6)",
            params![rowid, name, folder, source, labels, description],
        )?;
        transaction.execute("DELETE FROM search_documents WHERE id = ?1", [rowid])?;
    }
    Ok(())
}

pub(crate) fn ensure_managed_parent(path: &std::path::Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(CoreError::UnsafeManagedDirectory),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn remove_directory_if_present(path: &std::path::Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path)?
        }
        Ok(_) => return Err(CoreError::UnsafeManagedDirectory),
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
