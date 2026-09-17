use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, MAIN_DB};

use crate::error::{CoreError, Result};

pub(crate) const CURRENT_SCHEMA_VERSION: u32 = 4;
const INITIAL_SCHEMA: &str = include_str!("../../migrations/001_initial.sql");

pub(super) fn apply(
    mut connection: Connection,
    database_path: &Path,
    root: &Path,
    existing_database: bool,
) -> Result<Connection> {
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(CoreError::UnsupportedSchemaVersion {
            found: version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    if version == CURRENT_SCHEMA_VERSION {
        return Ok(connection);
    }

    if existing_database && database_path.metadata()?.len() > 0 {
        backup_before_migration(&connection, root, version)?;
    }

    let transaction = connection
        .transaction()
        .map_err(|error| CoreError::Migration(error.to_string()))?;
    transaction
        .execute_batch(INITIAL_SCHEMA)
        .map_err(|error| CoreError::Migration(error.to_string()))?;
    if version == 1 {
        transaction
            .execute(
                "INSERT OR IGNORE INTO user_label_suppressions (portrait_id, category, normalized_value) SELECT portrait_id, category, normalized_value FROM suppressed_inferred_labels",
                [],
            )
            .map_err(|error| CoreError::Migration(error.to_string()))?;
    }
    if version < 3 {
        transaction
            .execute(
                "INSERT OR IGNORE INTO portrait_sources (portrait_id, source_id) SELECT id, source_id FROM portraits",
                [],
            )
            .map_err(|error| CoreError::Migration(error.to_string()))?;
    }
    transaction
        .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
        .map_err(|error| CoreError::Migration(error.to_string()))?;
    transaction
        .commit()
        .map_err(|error| CoreError::Migration(error.to_string()))?;

    Ok(connection)
}

fn backup_before_migration(connection: &Connection, root: &Path, version: u32) -> Result<()> {
    fs::create_dir_all(root.join("staging"))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CoreError::Migration(error.to_string()))?
        .as_nanos();
    let destination = root
        .join("staging")
        .join(format!("pre-migration-v{version}-{timestamp}.sqlite3"));
    connection.backup(MAIN_DB, destination, None)?;
    Ok(())
}
