use portrait_core::metadata::apply_inferred_labels;
use portrait_core::types::Label;
use portrait_core::{CoreError, Library};
use rusqlite::Connection;
use serde_json::json;

#[test]
fn moving_a_closed_library_preserves_identity() {
    let temp = tempfile::tempdir().unwrap();
    let old = temp.path().join("old");
    let new = temp.path().join("new");
    let lib = Library::create(&old).unwrap();
    let id = lib.id();
    assert!(Library::open(&old).is_err());
    drop(lib);
    std::fs::rename(&old, &new).unwrap();
    assert_eq!(Library::open(&new).unwrap().id(), id);
}

#[test]
fn create_refuses_a_nonempty_destination_without_touching_it() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("portraits");
    std::fs::create_dir(&root).unwrap();
    let sentinel = root.join("keep.txt");
    std::fs::write(&sentinel, "do not replace").unwrap();

    let error = Library::create(&root).unwrap_err();

    assert_eq!(error.code(), "LIBRARY_DESTINATION_NOT_EMPTY");
    assert_eq!(std::fs::read_to_string(sentinel).unwrap(), "do not replace");
    assert!(!root.join("library.json").exists());
}

#[test]
fn opening_a_newer_library_format_returns_an_actionable_error() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("future-library");
    let library = Library::create(&root).unwrap();
    let id = library.id();
    drop(library);
    std::fs::write(
        root.join("library.json"),
        serde_json::to_vec_pretty(&json!({
            "formatVersion": 999,
            "libraryId": id,
        }))
        .unwrap(),
    )
    .unwrap();

    let error = Library::open(&root).unwrap_err();

    assert!(matches!(
        error,
        CoreError::UnsupportedFormatVersion {
            found: 999,
            supported: 1
        }
    ));
    assert_eq!(error.code(), "LIBRARY_FORMAT_TOO_NEW");
    assert!(error.to_string().contains("999"));
}

#[test]
fn created_library_has_required_schema_and_connection_safety_settings() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("schema-library");
    let library = Library::create(&root).unwrap();

    let foreign_keys: u32 = library
        .connection()
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .unwrap();
    let journal_mode: String = library
        .connection()
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    let version: u32 = library
        .connection()
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    let mut statement = library
        .connection()
        .prepare("SELECT name FROM sqlite_master WHERE type IN ('table', 'view')")
        .unwrap();
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<std::collections::BTreeSet<_>>>()
        .unwrap();

    assert_eq!(foreign_keys, 1);
    assert_eq!(journal_mode.to_lowercase(), "delete");
    assert_eq!(version, 2);
    for required in [
        "sources",
        "portraits",
        "assets",
        "labels",
        "portrait_labels",
        "suppressed_inferred_labels",
        "user_label_suppressions",
        "selection",
        "search_documents",
        "search_index",
        "operation_state",
    ] {
        assert!(names.contains(required), "missing table {required}");
    }
}

#[test]
fn opening_an_older_schema_creates_a_backup_before_migration() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("old-schema");
    drop(Library::create(&root).unwrap());
    let connection = Connection::open(root.join("library.sqlite3")).unwrap();
    connection.pragma_update(None, "user_version", 0).unwrap();
    drop(connection);

    let library = Library::open(&root).unwrap();
    let backups = std::fs::read_dir(root.join("staging"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("pre-migration-v0-") && name.ends_with(".sqlite3"))
        .collect::<Vec<_>>();

    assert_eq!(backups.len(), 1);
    let version: u32 = library
        .connection()
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 2);
}

#[test]
fn migrating_v1_backfills_legacy_inference_suppressions_for_future_producers() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("v1-suppression");
    drop(Library::create(&root).unwrap());
    let portrait_id = uuid::Uuid::new_v4();
    let connection = Connection::open(root.join("library.sqlite3")).unwrap();
    connection
        .execute(
            "INSERT INTO sources (id, name, kind) VALUES ('source', 'Source', 'folder')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO portraits (id, source_id, name, original_folder) VALUES (?1, 'source', 'Portrait', '')",
            [portrait_id.to_string()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO suppressed_inferred_labels (portrait_id, category, normalized_value, producer, producer_version) VALUES (?1, 'race', 'elf', 'path-vocabulary', '1')",
            [portrait_id.to_string()],
        )
        .unwrap();
    connection
        .execute("DROP TABLE user_label_suppressions", [])
        .unwrap();
    connection.pragma_update(None, "user_version", 1).unwrap();
    drop(connection);

    let mut library = Library::open(&root).unwrap();
    apply_inferred_labels(
        &mut library,
        portrait_id,
        &[Label {
            category: "race".into(),
            value: "elf".into(),
        }],
        "model-classifier",
        "2026.09",
    )
    .unwrap();

    let suppressions: i64 = library
        .connection()
        .query_row(
            "SELECT count(*) FROM user_label_suppressions WHERE portrait_id = ?1 AND category = 'race' AND normalized_value = 'elf'",
            [portrait_id.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    let labels: i64 = library
        .connection()
        .query_row(
            "SELECT count(*) FROM portrait_labels WHERE portrait_id = ?1",
            [portrait_id.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(suppressions, 1);
    assert_eq!(labels, 0);
}

#[test]
fn asset_paths_accept_only_portable_relative_components() {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::create(&temp.path().join("paths")).unwrap();
    let connection = library.connection();
    connection
        .execute(
            "INSERT INTO sources (id, name, kind) VALUES ('source', 'Source', 'folder')",
            [],
        )
        .unwrap();

    let invalid_paths = [
        "",
        "/absolute/Small.png",
        r"C:\portraits\Small.png",
        "C:/portraits/Small.png",
        r"\\server\share\Small.png",
        r"..\Small.png",
        "../Small.png",
        "portraits/..",
        "portraits/.",
        "portraits/../Small.png",
        "portraits/./Small.png",
        "portraits//Small.png",
        "portraits/",
    ];

    for (index, path) in invalid_paths.into_iter().enumerate() {
        let portrait_id = format!("portrait-{index}");
        connection
            .execute(
                "INSERT INTO portraits (id, source_id, name, original_folder) VALUES (?1, 'source', ?1, '')",
                [&portrait_id],
            )
            .unwrap();
        let error = connection
            .execute(
                "INSERT INTO assets (portrait_id, role, relative_path, width, height, file_size) VALUES (?1, 'small', ?2, 185, 242, 1)",
                [&portrait_id, path],
            )
            .unwrap_err();
        assert!(
            error.to_string().contains("CHECK constraint failed"),
            "path {path:?} failed for the wrong reason: {error}"
        );
    }

    connection
        .execute(
            "INSERT INTO portraits (id, source_id, name, original_folder) VALUES ('valid', 'source', 'Valid', '')",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO assets (portrait_id, role, relative_path, width, height, file_size) VALUES ('valid', 'small', 'portraits/portrait-id/Small.png', 185, 242, 1)",
            [],
        )
        .unwrap();
}
