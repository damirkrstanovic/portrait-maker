//! Portable snapshots. Callers must hold the library mutation gate for the whole operation.
use crate::{
    CoreError, Library, Result,
    import::{
        JobContext,
        archive::{ExtractionLimits, extract_archive, validate_entry_path},
        validate::decode_png,
    },
    library::{CURRENT_FORMAT_VERSION, migrations::CURRENT_SCHEMA_VERSION},
};
use image::GenericImageView;
use rusqlite::{Connection, MAIN_DB, OpenFlags};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};
use uuid::Uuid;

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::InvalidBackup(message.into())
}
fn cancelled(job: &JobContext) -> Result<()> {
    if job.is_cancelled() {
        Err(CoreError::Cancelled)
    } else {
        Ok(())
    }
}

pub fn backup_library(lib: &Library, target: &Path, job: &JobContext) -> Result<()> {
    backup_library_confirmed(lib, target, job, false)
}

/// `overwrite` must only be true after confirmation of this exact output path.
pub fn backup_library_confirmed(
    lib: &Library,
    target: &Path,
    job: &JobContext,
    overwrite: bool,
) -> Result<()> {
    cancelled(job)?;
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize()?;
    if parent.starts_with(lib.root().canonicalize()?) {
        return Err(invalid("Choose a backup file outside the library."));
    }
    let target = parent.join(
        target
            .file_name()
            .ok_or_else(|| invalid("Choose a ZIP filename."))?,
    );
    check_output(&target, overwrite)?;
    ensure_portable_operations(lib.connection())?;
    let scratch = tempfile::tempdir_in(&parent)?;
    let database = scratch.path().join("library.sqlite3");
    job.report_status(0, None, "Snapshotting library database");
    lib.connection().backup(MAIN_DB, &database, None)?;
    let connection = Connection::open(&database)?;
    connection.execute(
        "DELETE FROM operation_state WHERE kind='export_receipt'",
        [],
    )?;
    // Derived hashes are not portable authority. Rebuilding them after restore prevents an
    // untrusted archive cache from influencing a future skip/consolidation decision.
    connection
        .execute_batch("DELETE FROM portrait_fingerprints; DELETE FROM pixel_fingerprints;")?;
    drop(connection);
    let connection = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let paths = validate_database(&connection, lib.root(), job)?;
    drop(connection);
    let temporary = tempfile::NamedTempFile::new_in(&parent)?;
    let mut writer = zip::ZipWriter::new(temporary.reopen()?);
    let total = paths.len() as u64 + 2;
    let entries = std::iter::once(("library.json".to_owned(), lib.root().join("library.json")))
        .chain(std::iter::once(("library.sqlite3".to_owned(), database)))
        .chain(paths.into_iter().map(|p| {
            let path = lib.root().join(&p);
            (p, path)
        }));
    for (index, (name, path)) in entries.enumerate() {
        cancelled(job)?;
        regular_file(lib.root(), &path, name == "library.sqlite3")?;
        let mut source = File::open(&path)?;
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored)
                    .large_file(source.metadata()?.len() >= u64::from(u32::MAX)),
            )
            .map_err(|e| invalid(e.to_string()))?;
        let mut buffer = [0u8; 65536];
        loop {
            cancelled(job)?;
            let read = source.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            writer.write_all(&buffer[..read])?;
        }
        job.report_status(index as u64 + 1, Some(total), "Writing portable backup");
    }
    writer
        .finish()
        .map_err(|e| invalid(e.to_string()))?
        .sync_all()?;
    cancelled(job)?;
    check_output(&target, overwrite)?;
    if overwrite {
        temporary
            .persist(&target)
            .map_err(|e| CoreError::Io(e.error))?;
    } else {
        temporary
            .persist_noclobber(&target)
            .map_err(|e| CoreError::Io(e.error))?;
    }
    Ok(())
}
fn check_output(target: &Path, overwrite: bool) -> Result<()> {
    match fs::symlink_metadata(target) {
        Ok(m) if !m.is_file() || m.file_type().is_symlink() => {
            Err(invalid("The backup output must be a regular file."))
        }
        Ok(_) if !overwrite => Err(CoreError::BackupConfirmationRequired),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
fn empty_target(root: &Path) -> Result<()> {
    match fs::symlink_metadata(root) {
        Ok(m)
            if m.is_dir()
                && !m.file_type().is_symlink()
                && fs::read_dir(root)?.next().is_none() =>
        {
            Ok(())
        }
        Ok(_) => Err(CoreError::DestinationNotEmpty),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub fn restore_library(archive: &Path, root: &Path, job: &JobContext) -> Result<Library> {
    cancelled(job)?;
    empty_target(root)?;
    let parent = root
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let root = parent.join(root.file_name().ok_or(CoreError::InvalidLibraryName)?);
    let scratch = tempfile::tempdir_in(parent)?;
    let stage = scratch.path().join("library");
    job.report_status(0, None, "Extracting portable backup");
    // Reuse the bounded, collision/link/traversal-safe archive reader.
    extract_archive(archive, &stage, &ExtractionLimits::default(), job)?;
    let manifest_path = stage.join("library.json");
    if manifest_path.metadata()?.len() > 65536 {
        return Err(invalid("Manifest is too large."));
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Manifest {
        format_version: u32,
        library_id: Uuid,
    }
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(manifest_path)?).map_err(|e| invalid(e.to_string()))?;
    if manifest.format_version != CURRENT_FORMAT_VERSION {
        return Err(CoreError::UnsupportedFormatVersion {
            found: manifest.format_version,
            supported: CURRENT_FORMAT_VERSION,
        });
    }
    if manifest.library_id.is_nil() {
        return Err(invalid("Library identity is missing."));
    }
    let connection = Connection::open_with_flags(
        stage.join("library.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?;
    let paths = validate_database(&connection, &stage, job)?;
    // Local receipts never become history or recovery instructions on the restored machine.
    connection.execute(
        "DELETE FROM operation_state WHERE kind='export_receipt'",
        [],
    )?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version >= 4 {
        connection
            .execute_batch("DELETE FROM portrait_fingerprints; DELETE FROM pixel_fingerprints;")?;
    }
    drop(connection);
    let allowed: HashSet<_> = paths
        .into_iter()
        .chain(["library.json".into(), "library.sqlite3".into()])
        .collect();
    check_tree(&stage, &stage, &allowed, job)?;
    cancelled(job)?;
    empty_target(&root)?;
    if root.exists() {
        fs::remove_dir(&root)?;
    }
    fs::rename(&stage, &root)?;
    Library::open(&root)
}
fn ensure_portable_operations(connection: &Connection) -> Result<()> {
    let count:i64=connection.query_row("SELECT count(*) FROM operation_state WHERE kind NOT IN ('catalog_revision','export_receipt')",[],|r|r.get(0))?;
    if count != 0 {
        return Err(invalid(
            "Unresolved operations remain. Reopen the original library and resolve recovery before backing up or restoring.",
        ));
    }
    Ok(())
}
fn validate_database(
    connection: &Connection,
    root: &Path,
    job: &JobContext,
) -> Result<Vec<String>> {
    cancelled(job)?;
    connection.pragma_update(None, "trusted_schema", false)?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version > CURRENT_SCHEMA_VERSION {
        return Err(CoreError::UnsupportedSchemaVersion {
            found: version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    if version == 0 {
        return Err(invalid("Unversioned database."));
    }
    // Require the known table definitions, including FTS shadow tables; do not execute a supplied trigger/view.
    let expected = Connection::open_in_memory()?;
    expected.execute_batch(include_str!("../migrations/001_initial.sql"))?;
    fn schema(c: &Connection) -> Result<BTreeMap<String, String>> {
        let mut s = c.prepare(
            "SELECT name,sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'",
        )?;
        Ok(s.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get::<_, String>(1)?
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            ))
        })?
        .collect::<std::result::Result<_, _>>()?)
    }
    let actual = schema(connection)?;
    let mut expected = schema(&expected)?;
    if version < 3 {
        expected.remove("portrait_sources");
        expected.remove("portrait_sources_source_id_idx");
    }
    if version == 1 && !actual.contains_key("user_label_suppressions") {
        expected.remove("user_label_suppressions");
    }
    if version < 4 {
        expected.remove("portrait_fingerprints");
        expected.remove("pixel_fingerprints");
    }
    if actual != expected {
        return Err(invalid(
            "Database schema does not match a supported portrait library.",
        ));
    }
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(invalid(format!("Database integrity check: {integrity}")));
    }
    if connection.prepare("PRAGMA foreign_key_check")?.exists([])? {
        return Err(invalid("Database contains broken references."));
    }
    ensure_portable_operations(connection)?;
    let incomplete:i64=connection.query_row("SELECT count(*) FROM portraits p WHERE (SELECT count(*) FROM assets a WHERE a.portrait_id=p.id AND a.role IN ('small','medium','large'))<>3",[],|r|r.get(0))?;
    if incomplete != 0 {
        return Err(invalid("A portrait is missing an image role."));
    }
    let mut stmt = connection.prepare(
        "SELECT relative_path,width,height,file_size FROM assets ORDER BY relative_path",
    )?;
    let assets = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, u32>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut paths = Vec::new();
    let total = assets.len() as u64;
    for (index, (path, width, height, size)) in assets.into_iter().enumerate() {
        cancelled(job)?;
        let relative = validate_entry_path(&path)?;
        if relative.to_str() != Some(path.as_str()) || !relative.starts_with("portraits") {
            return Err(invalid(format!("Unsafe asset path: {path}")));
        }
        let full = root.join(&relative);
        regular_file(root, &full, false)?;
        if full.metadata()?.len() != size as u64
            || decode_png(&full)?.dimensions() != (width, height)
        {
            return Err(invalid(format!("Asset dimensions or size differ: {path}")));
        }
        paths.push(path);
        job.report_status(index as u64 + 1, Some(total), "Validating library images");
    }
    Ok(paths)
}
fn regular_file(root: &Path, path: &Path, temporary: bool) -> Result<()> {
    if !temporary {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| invalid("Asset outside library."))?;
        let mut current = root.to_path_buf();
        for part in relative.components() {
            current.push(part);
            if fs::symlink_metadata(&current)?.file_type().is_symlink() {
                return Err(invalid("Linked library assets are not portable."));
            }
        }
    }
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(invalid("Expected regular library file."));
    }
    Ok(())
}
fn check_tree(root: &Path, dir: &Path, allowed: &HashSet<String>, job: &JobContext) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        cancelled(job)?;
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            check_tree(root, &path, allowed, job)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| invalid("Invalid backup path"))?
                .components()
                .map(|p| p.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if !allowed.contains(&relative) {
                return Err(invalid(format!("Unexpected backup file: {relative}")));
            }
        }
    }
    Ok(())
}
