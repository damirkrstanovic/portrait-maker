use portrait_core::Library;
use portrait_core::types::{ImportKind, ImportRequest, JobState};
use portrait_manager_lib::state::DesktopState;

#[test]
fn desktop_state_remembers_a_library_after_close_and_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let library_path = temp.path().join("Portable portraits");
    let state = DesktopState::new(temp.path().join("settings"));

    let created = state
        .activate(Library::create(&library_path).unwrap())
        .unwrap();
    assert_eq!(created.name, "Portable portraits");
    assert_eq!(created.path, library_path.to_string_lossy());
    state.close().unwrap();

    assert_eq!(
        state.last_library_path().unwrap(),
        Some(library_path.clone())
    );
    let reopened = state
        .activate(Library::open(&library_path).unwrap())
        .unwrap();
    assert_eq!(reopened.id, created.id);
}

#[test]
fn desktop_error_for_a_locked_library_keeps_the_stable_core_code() {
    let temp = tempfile::tempdir().unwrap();
    let library_path = temp.path().join("locked");
    let _writer = Library::create(&library_path).unwrap();

    let error = Library::open(&library_path).unwrap_err();
    let app_error = DesktopState::app_error(error);

    assert_eq!(app_error.code, "LIBRARY_LOCKED");
    assert!(app_error.recoverable);
    assert!(app_error.message.contains("another window"));
}

#[test]
fn import_job_can_be_cancelled_without_waiting_for_the_library_worker() {
    let temp = tempfile::tempdir().unwrap();
    let library_path = temp.path().join("library");
    let state = DesktopState::new(temp.path().join("settings"));
    state
        .activate(Library::create(&library_path).unwrap())
        .unwrap();

    let job = state
        .start_import(ImportRequest {
            path: temp.path().join("missing"),
            source_name: "Missing".into(),
            kind: ImportKind::Folder,
            resize: false,
            duplicate_policy: Default::default(),
        })
        .unwrap();
    state.cancel_job(job.id).unwrap();

    for _ in 0..50 {
        if state.job(job.id).unwrap().unwrap().state != JobState::Running {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        state.job(job.id).unwrap().unwrap().state,
        JobState::Cancelled
    );
}

#[test]
fn purge_uses_the_shared_job_registry_and_counts_nonexistent_ids_as_zero() {
    let temp = tempfile::tempdir().unwrap();
    let state = DesktopState::new(temp.path().join("config"));
    let library = Library::create(&temp.path().join("library")).unwrap();
    state.activate(library).unwrap();

    let job = state.start_purge(vec![uuid::Uuid::new_v4()]).unwrap();
    let mut terminal = None;
    for _ in 0..50 {
        let current = state.job(job.id).unwrap().unwrap();
        if current.state != portrait_core::types::JobState::Running {
            terminal = Some(current);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let terminal = terminal.expect("purge job should reach a terminal state");
    assert_eq!(terminal.state, portrait_core::types::JobState::Done);
    assert_eq!(terminal.total, Some(0));
    assert_eq!(terminal.completed, 0);
    assert_eq!(terminal.message, "Permanently deleted 0 portraits");
}
