mod support;

use std::fs;
use std::sync::{Arc, Mutex};

use portrait_core::Library;
use portrait_core::import::{JobContext, import_portraits, validate_portrait};
use portrait_core::types::{ImportKind, ImportRequest};

fn request(path: std::path::PathBuf, resize: bool) -> ImportRequest {
    ImportRequest {
        path,
        source_name: "Pack".into(),
        kind: ImportKind::Folder,
        resize,
        duplicate_policy: Default::default(),
    }
}

#[test]
fn repeated_imports_remain_separate_entries() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    support::write_portrait(&input, "female_elf_archer");
    let mut library = Library::create(&temp.path().join("library")).unwrap();

    for _ in 0..2 {
        assert_eq!(
            import_portraits(
                &mut library,
                request(input.clone(), false),
                &JobContext::default()
            )
            .unwrap()
            .imported,
            1
        );
    }

    assert_eq!(
        library
            .connection()
            .query_row("SELECT count(*) FROM portraits", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn nested_mixed_case_sets_are_imported_and_partial_sets_are_reported() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    support::write_portrait(&input, "nested/FEMALE_ELF_ARCHER");
    let incomplete = input.join("broken");
    fs::create_dir_all(&incomplete).unwrap();
    fs::copy(
        input.join("nested/FEMALE_ELF_ARCHER/Small.png"),
        incomplete.join("Small.PNG"),
    )
    .unwrap();
    let mut library = Library::create(&temp.path().join("library")).unwrap();

    let report =
        import_portraits(&mut library, request(input, false), &JobContext::default()).unwrap();

    assert_eq!(report.imported, 1, "{report:?}");
    assert_eq!(report.skipped, 1);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == "INCOMPLETE_SET")
    );
}

#[test]
fn default_preserves_nonstandard_dimensions_and_opt_in_resize_normalizes_copies() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    let source =
        support::write_portrait_with_dimensions(&input, "odd", (100, 100), (200, 100), (300, 100));
    let original = fs::read(source.join("Small.png")).unwrap();

    let mut unchanged = Library::create(&temp.path().join("unchanged")).unwrap();
    let report = import_portraits(
        &mut unchanged,
        request(input.clone(), false),
        &JobContext::default(),
    )
    .unwrap();
    assert_eq!(report.imported, 1);
    assert_eq!(report.skipped, 0);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == "NONSTANDARD_DIMENSIONS")
    );
    let dimensions: (i64, i64) = unchanged
        .connection()
        .query_row(
            "SELECT width, height FROM assets WHERE role = 'small'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(dimensions, (100, 100));

    let mut normalized = Library::create(&temp.path().join("normalized")).unwrap();
    let report = import_portraits(
        &mut normalized,
        request(input, true),
        &JobContext::default(),
    )
    .unwrap();
    assert_eq!(report.imported, 1);
    let dimensions: (i64, i64) = normalized
        .connection()
        .query_row(
            "SELECT width, height FROM assets WHERE role = 'small'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(dimensions, (185, 242));
    assert_eq!(fs::read(source.join("Small.png")).unwrap(), original);
}

#[test]
fn validation_rejects_ambiguous_non_png_and_corrupted_sets() {
    let temp = tempfile::tempdir().unwrap();
    let directory = support::write_portrait(temp.path(), "portrait");
    fs::copy(directory.join("Small.png"), directory.join("small.PNG")).unwrap();
    assert_eq!(
        validate_portrait(&directory).unwrap_err().code(),
        "AMBIGUOUS_SET"
    );
    fs::remove_file(directory.join("small.PNG")).unwrap();
    fs::write(directory.join("notes.jpg"), "not png").unwrap();
    assert_eq!(
        validate_portrait(&directory).unwrap_err().code(),
        "INVALID_PORTRAIT_SET"
    );
    fs::remove_file(directory.join("notes.jpg")).unwrap();
    fs::write(directory.join("Medium.png"), "not a png").unwrap();
    assert_eq!(
        validate_portrait(&directory).unwrap_err().code(),
        "INVALID_PNG"
    );
}

#[test]
fn cancellation_leaves_committed_sets_and_removes_empty_source() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    support::write_portrait(&input, "one");
    support::write_portrait(&input, "two");
    let holder = Arc::new(Mutex::new(None::<JobContext>));
    let callback_holder = Arc::clone(&holder);
    let job = JobContext::with_progress(move |completed, _| {
        if completed >= 1 {
            callback_holder.lock().unwrap().as_ref().unwrap().cancel();
        }
    });
    *holder.lock().unwrap() = Some(job.clone());
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    let report = import_portraits(&mut library, request(input, false), &job).unwrap();
    assert!(report.cancelled);
    assert!(report.imported <= 1);
    assert!(
        library
            .connection()
            .query_row("SELECT count(*) FROM sources", [], |r| r.get::<_, i64>(0))
            .unwrap()
            <= 1
    );
}

#[test]
fn cancellation_before_archive_extraction_returns_a_cancelled_report() {
    let temp = tempfile::tempdir().unwrap();
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    let job = JobContext::default();
    job.cancel();

    let report = import_portraits(
        &mut library,
        ImportRequest {
            path: temp.path().join("missing.zip"),
            source_name: "Archive".into(),
            kind: ImportKind::Archive,
            resize: false,
            duplicate_policy: Default::default(),
        },
        &job,
    )
    .unwrap();

    assert!(report.cancelled);
    assert_eq!(report.imported, 0);
}

#[test]
fn opening_recovers_only_valid_import_intents_under_the_library_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let portrait_id = uuid::Uuid::new_v4();
    let operation_id = uuid::Uuid::new_v4();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep"), "keep").unwrap();
    let library = Library::create(&root).unwrap();
    fs::create_dir(root.join("staging").join(format!("import-{operation_id}"))).unwrap();
    fs::create_dir(root.join("portraits").join(portrait_id.to_string())).unwrap();
    library.connection().execute(
        "INSERT INTO operation_state (operation_id, kind, state_json) VALUES (?1, 'import', ?2)",
        [operation_id.to_string(), format!(r#"{{"portraitId":"{portrait_id}"}}"#)],
    ).unwrap();
    library.connection().execute(
        "INSERT INTO operation_state (operation_id, kind, state_json) VALUES ('bad', 'import', ?1)",
        [format!(r#"{{"portraitId":"{}","final":"{}"}}"#, uuid::Uuid::new_v4(), outside.display())],
    ).unwrap();
    drop(library);

    let reopened = Library::open(&root).unwrap();

    assert!(
        !root
            .join("staging")
            .join(format!("import-{operation_id}"))
            .exists()
    );
    assert!(
        !root
            .join("portraits")
            .join(portrait_id.to_string())
            .exists()
    );
    assert_eq!(fs::read_to_string(outside.join("keep")).unwrap(), "keep");
    assert_eq!(
        reopened
            .connection()
            .query_row(
                "SELECT count(*) FROM operation_state WHERE operation_id = ?1",
                [operation_id.to_string()],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[cfg(unix)]
#[test]
fn opening_rejects_symlinked_managed_parents_without_deleting_external_intents() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let outside = temp.path().join("outside");
    let operation_id = uuid::Uuid::new_v4();
    let portrait_id = uuid::Uuid::new_v4();
    fs::create_dir_all(&outside).unwrap();
    let library = Library::create(&root).unwrap();
    library.connection().execute(
        "INSERT INTO operation_state (operation_id, kind, state_json) VALUES (?1, 'import', ?2)",
        [operation_id.to_string(), format!(r#"{{"portraitId":"{portrait_id}"}}"#)],
    ).unwrap();
    drop(library);
    fs::remove_dir(root.join("staging")).unwrap();
    fs::create_dir(outside.join(format!("import-{operation_id}"))).unwrap();
    fs::write(
        outside.join(format!("import-{operation_id}")).join("keep"),
        "keep",
    )
    .unwrap();
    symlink(&outside, root.join("staging")).unwrap();

    let error = Library::open(&root).unwrap_err();

    assert_eq!(error.code(), "LIBRARY_MANAGED_PATH_UNSAFE");
    assert_eq!(
        fs::read_to_string(outside.join(format!("import-{operation_id}")).join("keep")).unwrap(),
        "keep"
    );
}

#[test]
fn opening_removes_only_uuid_shaped_abandoned_extraction_staging() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let extraction_id = uuid::Uuid::new_v4();
    let library = Library::create(&root).unwrap();
    fs::create_dir(
        root.join("staging")
            .join(format!("extract-{extraction_id}")),
    )
    .unwrap();
    fs::create_dir(root.join("staging").join("keep-me")).unwrap();
    drop(library);

    let _reopened = Library::open(&root).unwrap();

    assert!(
        !root
            .join("staging")
            .join(format!("extract-{extraction_id}"))
            .exists()
    );
    assert!(root.join("staging").join("keep-me").exists());
}

#[test]
fn candidate_sets_with_extra_files_are_reported_instead_of_silently_ignored() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    let candidate = support::write_portrait(&input, "candidate");
    fs::write(candidate.join("notes.txt"), "not an asset").unwrap();
    let mut library = Library::create(&temp.path().join("library")).unwrap();

    let report =
        import_portraits(&mut library, request(input, false), &JobContext::default()).unwrap();

    assert_eq!(report.imported, 0);
    assert_eq!(report.skipped, 1);
    assert_eq!(report.issues[0].path, "candidate");
    assert_eq!(report.issues[0].code, "INVALID_PORTRAIT_SET");
}

#[test]
fn root_level_set_infers_labels_from_its_folder_name() {
    let temp = tempfile::tempdir().unwrap();
    let input = support::write_portrait(temp.path(), "female_elf_archer");
    let mut library = Library::create(&temp.path().join("library")).unwrap();

    let report =
        import_portraits(&mut library, request(input, false), &JobContext::default()).unwrap();

    assert_eq!(report.imported, 1);
    assert_eq!(
        library
            .connection()
            .query_row("SELECT count(*) FROM portrait_labels", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        3
    );
}

#[test]
#[ignore = "private user archives are read-only smoke fixtures"]
fn private_archives_import_complete_sets_with_expected_dimensions() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let temp = tempfile::tempdir().unwrap();
    let mut p1 = Library::create(&temp.path().join("p1-library")).unwrap();
    let p1_report = import_portraits(
        &mut p1,
        ImportRequest {
            path: root.join("p1.zip"),
            source_name: "p1".into(),
            kind: ImportKind::Archive,
            resize: false,
            duplicate_policy: Default::default(),
        },
        &JobContext::default(),
    )
    .unwrap();
    assert_eq!(p1_report.imported, 45);
    assert_eq!(p1_report.skipped, 0);
    let p1_dimensions: (i64, i64) = p1
        .connection()
        .query_row(
            "SELECT width, height FROM assets WHERE role = 'small' LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(p1_dimensions, (260, 336));
    drop(p1);

    let mut resized = Library::create(&temp.path().join("resized-library")).unwrap();
    let resized_report = import_portraits(
        &mut resized,
        ImportRequest {
            path: root.join("p1.zip"),
            source_name: "p1".into(),
            kind: ImportKind::Archive,
            resize: true,
            duplicate_policy: Default::default(),
        },
        &JobContext::default(),
    )
    .unwrap();
    assert_eq!(resized_report.imported, 45);
    let resized_dimensions: (i64, i64) = resized
        .connection()
        .query_row(
            "SELECT width, height FROM assets WHERE role = 'large' LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(resized_dimensions, (692, 1024));
    drop(resized);

    let mut heroes = Library::create(&temp.path().join("heroes-library")).unwrap();
    let heroes_report = import_portraits(
        &mut heroes,
        ImportRequest {
            path: root.join("heroes.rar"),
            source_name: "heroes".into(),
            kind: ImportKind::Archive,
            resize: false,
            duplicate_policy: Default::default(),
        },
        &JobContext::default(),
    )
    .unwrap();
    assert_eq!(heroes_report.imported, 377);
    assert_eq!(heroes_report.skipped, 0);
}
