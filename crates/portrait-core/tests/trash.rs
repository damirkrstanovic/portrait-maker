mod support;

use std::fs;

use portrait_core::Library;
use portrait_core::catalog::query_catalog;
use portrait_core::import::{JobContext, import_portraits};
use portrait_core::selection::{SelectionAction, SelectionTarget, change_selection};
use portrait_core::trash::{purge_portraits, restore_portraits, trash_portraits};
use portrait_core::types::{ImportKind, ImportRequest, Page, Query};

fn page(library: &Library, query: Query) -> portrait_core::types::CatalogPage {
    query_catalog(
        library,
        &query,
        Page {
            offset: 0,
            limit: 20,
        },
    )
    .unwrap()
}

#[test]
fn trash_removes_selection_and_restore_does_not_reselect() {
    let temp = tempfile::tempdir().unwrap();
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    let id = support::seed_catalog(&mut library, 1)[0];
    change_selection(
        &mut library,
        SelectionTarget::Ids(vec![id]),
        SelectionAction::Add,
    )
    .unwrap();

    trash_portraits(&mut library, &[id]).unwrap();
    assert_eq!(page(&library, Query::default()).total, 0);
    assert_eq!(
        page(
            &library,
            Query {
                trash: true,
                ..Query::default()
            }
        )
        .total,
        1
    );

    restore_portraits(&mut library, &[id]).unwrap();
    let portrait = page(&library, Query::default()).items.pop().unwrap();
    assert!(!portrait.selected);
    assert!(portrait.trashed_at.is_none());
}

#[test]
fn purge_moves_managed_assets_to_journaled_staging_then_removes_catalog_record() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let mut library = Library::create(&root).unwrap();
    let id = support::seed_catalog(&mut library, 1)[0];
    let assets = root.join("portraits").join(id.to_string());
    fs::create_dir(&assets).unwrap();
    fs::write(assets.join("Small.png"), b"managed-only").unwrap();
    trash_portraits(&mut library, &[id]).unwrap();
    assert_eq!(
        page(
            &library,
            Query {
                text: "Generated".into(),
                trash: true,
                ..Query::default()
            }
        )
        .total,
        1
    );

    purge_portraits(&mut library, &[id], &JobContext::default()).unwrap();

    assert_eq!(
        page(
            &library,
            Query {
                trash: true,
                ..Query::default()
            }
        )
        .total,
        0
    );
    assert!(!assets.exists());
    assert_eq!(
        library
            .connection()
            .query_row(
                "SELECT count(*) FROM operation_state WHERE kind = 'purge'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0,
    );
    assert_eq!(
        library
            .connection()
            .query_row(
                "SELECT count(*) FROM search_documents WHERE portrait_id = ?1",
                [id.to_string()],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn purge_ignores_an_id_restored_before_execution_and_keeps_its_managed_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let mut library = Library::create(&root).unwrap();
    let id = support::seed_catalog(&mut library, 1)[0];
    let assets = root.join("portraits").join(id.to_string());
    fs::create_dir(&assets).unwrap();
    fs::write(assets.join("Small.png"), b"repeat-import-content").unwrap();
    trash_portraits(&mut library, &[id]).unwrap();
    restore_portraits(&mut library, &[id]).unwrap();

    purge_portraits(&mut library, &[id], &JobContext::default()).unwrap();

    assert_eq!(page(&library, Query::default()).total, 1);
    assert!(assets.join("Small.png").is_file());
}

#[test]
fn repeated_import_of_trashed_content_creates_a_separate_active_portrait() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let source = temp.path().join("source");
    support::write_portrait(&source, "elf");
    let mut library = Library::create(&root).unwrap();
    let request = ImportRequest {
        path: source,
        source_name: "Repeated pack".into(),
        kind: ImportKind::Folder,
        resize: false,
        duplicate_policy: Default::default(),
    };
    import_portraits(&mut library, request.clone(), &JobContext::default()).unwrap();
    let first = page(&library, Query::default()).items[0].id;
    trash_portraits(&mut library, &[first]).unwrap();
    import_portraits(&mut library, request, &JobContext::default()).unwrap();

    assert_eq!(page(&library, Query::default()).total, 1);
    assert_eq!(
        page(
            &library,
            Query {
                trash: true,
                ..Query::default()
            }
        )
        .total,
        1
    );
    assert_ne!(page(&library, Query::default()).items[0].id, first);
}

#[test]
fn cancelled_purge_keeps_trashed_catalog_and_files_intact() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let mut library = Library::create(&root).unwrap();
    let id = support::seed_catalog(&mut library, 1)[0];
    let assets = root.join("portraits").join(id.to_string());
    fs::create_dir(&assets).unwrap();
    fs::write(assets.join("Small.png"), b"managed-only").unwrap();
    trash_portraits(&mut library, &[id]).unwrap();
    let job = JobContext::default();
    job.cancel();

    assert_eq!(
        purge_portraits(&mut library, &[id], &job)
            .unwrap_err()
            .code(),
        "CANCELLED"
    );
    assert_eq!(
        page(
            &library,
            Query {
                trash: true,
                ..Query::default()
            }
        )
        .total,
        1
    );
    assert!(assets.join("Small.png").is_file());
}

#[test]
fn cancellation_from_final_progress_callback_rolls_back_staged_assets() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let mut library = Library::create(&root).unwrap();
    let id = support::seed_catalog(&mut library, 1)[0];
    let assets = root.join("portraits").join(id.to_string());
    fs::create_dir(&assets).unwrap();
    fs::write(assets.join("Small.png"), b"managed-only").unwrap();
    trash_portraits(&mut library, &[id]).unwrap();
    let context_slot = std::sync::Arc::new(std::sync::Mutex::new(None::<JobContext>));
    let context_for_callback = std::sync::Arc::clone(&context_slot);
    let job = JobContext::with_progress(move |completed, total| {
        if Some(completed) == total {
            context_for_callback
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .cancel();
        }
    });
    *context_slot.lock().unwrap() = Some(job.clone());

    assert_eq!(
        purge_portraits(&mut library, &[id], &job)
            .unwrap_err()
            .code(),
        "CANCELLED"
    );
    assert_eq!(
        page(
            &library,
            Query {
                trash: true,
                ..Query::default()
            }
        )
        .total,
        1
    );
    assert!(assets.join("Small.png").is_file());
}

#[cfg(unix)]
#[test]
fn opening_rejects_a_symlinked_purge_operation_directory_without_touching_external_data() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let external = temp.path().join("external");
    let mut library = Library::create(&root).unwrap();
    let id = support::seed_catalog(&mut library, 1)[0];
    let operation = uuid::Uuid::new_v4();
    fs::create_dir(&external).unwrap();
    fs::create_dir(external.join(id.to_string())).unwrap();
    fs::write(external.join(id.to_string()).join("Small.png"), b"outside").unwrap();
    symlink(
        &external,
        root.join("staging").join(format!("purge-{operation}")),
    )
    .unwrap();
    library
        .connection()
        .execute(
            "INSERT INTO operation_state (operation_id, kind, state_json) VALUES (?1, 'purge', ?2)",
            rusqlite::params![operation.to_string(), format!(r#"{{"ids":["{id}"]}}"#)],
        )
        .unwrap();
    drop(library);

    assert_eq!(Library::open(&root).unwrap_err().code(), "RECOVERY_FAILED");
    assert_eq!(
        fs::read(external.join(id.to_string()).join("Small.png")).unwrap(),
        b"outside"
    );
}

#[test]
fn opening_surfaces_a_malformed_purge_journal_without_deleting_it() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let library = Library::create(&root).unwrap();
    let operation = uuid::Uuid::new_v4();
    library.connection().execute(
        "INSERT INTO operation_state (operation_id, kind, state_json) VALUES (?1, 'purge', '{bad json')",
        [operation.to_string()],
    ).unwrap();
    drop(library);

    assert_eq!(Library::open(&root).unwrap_err().code(), "RECOVERY_FAILED");
    let connection = rusqlite::Connection::open(root.join("library.sqlite3")).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM operation_state WHERE operation_id = ?1",
                [operation.to_string()],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn opening_recovers_uncommitted_purge_by_restoring_staged_assets() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let mut library = Library::create(&root).unwrap();
    let id = support::seed_catalog(&mut library, 1)[0];
    let assets = root.join("portraits").join(id.to_string());
    fs::create_dir(&assets).unwrap();
    fs::write(assets.join("Small.png"), b"managed-only").unwrap();
    trash_portraits(&mut library, &[id]).unwrap();
    let operation = uuid::Uuid::new_v4();
    let staged = root
        .join("staging")
        .join(format!("purge-{operation}"))
        .join(id.to_string());
    fs::create_dir_all(staged.parent().unwrap()).unwrap();
    fs::rename(&assets, &staged).unwrap();
    library
        .connection()
        .execute(
            "INSERT INTO operation_state (operation_id, kind, state_json) VALUES (?1, 'purge', ?2)",
            rusqlite::params![
                operation.to_string(),
                format!(r#"{{"ids":["{id}"],"committed":false}}"#)
            ],
        )
        .unwrap();
    drop(library);

    let reopened = Library::open(&root).unwrap();
    assert!(assets.join("Small.png").is_file());
    assert_eq!(
        page(
            &reopened,
            Query {
                trash: true,
                ..Query::default()
            }
        )
        .total,
        1
    );
    assert_eq!(
        reopened
            .connection()
            .query_row(
                "SELECT count(*) FROM operation_state WHERE kind = 'purge'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn opening_finishes_committed_purge_cleanup_without_touching_external_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let external = temp.path().join("external");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("keep.txt"), b"keep").unwrap();
    let mut library = Library::create(&root).unwrap();
    let id = support::seed_catalog(&mut library, 1)[0];
    let operation = uuid::Uuid::new_v4();
    let staged = root
        .join("staging")
        .join(format!("purge-{operation}"))
        .join(id.to_string());
    fs::create_dir_all(&staged).unwrap();
    fs::write(staged.join("Small.png"), b"managed-only").unwrap();
    library
        .connection()
        .execute("DELETE FROM portraits WHERE id = ?1", [id.to_string()])
        .unwrap();
    library
        .connection()
        .execute(
            "INSERT INTO operation_state (operation_id, kind, state_json) VALUES (?1, 'purge', ?2)",
            rusqlite::params![
                operation.to_string(),
                format!(
                    r#"{{"ids":["{id}"],"committed":true,"outside":"{}"}}"#,
                    external.display()
                )
            ],
        )
        .unwrap();
    drop(library);

    let reopened = Library::open(&root).unwrap();
    assert!(!staged.exists());
    assert_eq!(fs::read(external.join("keep.txt")).unwrap(), b"keep");
    assert_eq!(
        reopened
            .connection()
            .query_row(
                "SELECT count(*) FROM operation_state WHERE kind = 'purge'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn opening_clears_committed_purge_journal_when_staging_cleanup_already_finished() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let mut library = Library::create(&root).unwrap();
    let id = support::seed_catalog(&mut library, 1)[0];
    let operation = uuid::Uuid::new_v4();
    library
        .connection()
        .execute("DELETE FROM portraits WHERE id = ?1", [id.to_string()])
        .unwrap();
    library
        .connection()
        .execute(
            "INSERT INTO operation_state (operation_id, kind, state_json) VALUES (?1, 'purge', ?2)",
            rusqlite::params![
                operation.to_string(),
                format!(r#"{{"ids":["{id}"],"committed":true}}"#)
            ],
        )
        .unwrap();
    drop(library);

    let reopened = Library::open(&root).unwrap();
    assert_eq!(
        reopened
            .connection()
            .query_row(
                "SELECT count(*) FROM operation_state WHERE kind = 'purge'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}
