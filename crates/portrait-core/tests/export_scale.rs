mod support;
use portrait_core::{
    Library,
    export::{ExportBoundary, ExportHistory, apply_export_with_observer, plan_export},
    import::JobContext,
    types::*,
};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
#[test]
fn generated_1395_portrait_export_has_linear_journal_and_responsive_preparation_cancel() {
    let temp = tempfile::tempdir().unwrap();
    let mut lib = Library::create(&temp.path().join("library")).unwrap();
    let ids = support::seed_catalog(&mut lib, 1395);
    let source = support::write_portrait_with_dimensions(
        temp.path(),
        "generated-pixels",
        (2, 3),
        (3, 4),
        (4, 5),
    );
    let tx = lib.connection().unchecked_transaction().unwrap();
    for id in ids {
        let folder = PathBuf::from("portraits").join(id.to_string());
        fs::create_dir(lib.root().join(&folder)).unwrap();
        for (role, name, w, h) in [
            ("small", "Small.png", 2, 3),
            ("medium", "Medium.png", 3, 4),
            ("large", "Fulllength.png", 4, 5),
        ] {
            let relative = folder.join(name);
            let size = fs::copy(source.join(name), lib.root().join(&relative)).unwrap();
            tx.execute("INSERT INTO assets(portrait_id,role,relative_path,width,height,file_size) VALUES(?,?,?,?,?,?)",rusqlite::params![id.to_string(),role,relative.to_string_lossy(),w,h,size as i64]).unwrap();
        }
    }
    tx.commit().unwrap();
    let request = |name: &str| ExportRequest {
        scope: ExportScope::All,
        target: temp.path().join(name),
        output: ExportOutput::Directory,
        mode: ExportMode::Merge,
    };
    let plan = plan_export(&lib, request("out"), &ExportHistory::default()).unwrap();
    let stored = lib.export_plan(plan.id).unwrap();
    let began = Instant::now();
    let phases = Arc::new(Mutex::new(Vec::<(String, f64)>::new()));
    let capture = phases.clone();
    let job = JobContext::with_status_progress(move |_, _, phase| {
        let mut list = capture.lock().unwrap();
        if list.last().is_none_or(|(p, _)| p != phase) {
            list.push((phase.to_owned(), began.elapsed().as_secs_f64()));
        }
    });
    let mut journal_size = 0;
    let mut journal_rows = 0;
    let report=apply_export_with_observer(&lib,&stored,true,&job,&mut|boundary|{if boundary==ExportBoundary::BeforeCommit{let header:i64=lib.connection().query_row("SELECT length(state_json) FROM operation_state WHERE kind='export'",[],|r|r.get(0)).unwrap();assert!(header<2048);(journal_size,journal_rows)=lib.connection().query_row("SELECT sum(length(state_json)),count(*) FROM operation_state WHERE kind IN ('export','export_delta')",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?))).unwrap();}Ok(())}).unwrap();
    let elapsed = began.elapsed();
    assert_eq!(report.added, 4185);
    assert!(
        elapsed < Duration::from_secs(30),
        "synthetic export took {elapsed:?}"
    );
    assert!(
        journal_size < 12_000_000,
        "journal should grow linearly: {journal_size}"
    );
    assert_eq!(fs::read_dir(temp.path().join("out")).unwrap().count(), 1395);
    assert_eq!(
        lib.connection()
            .query_row(
                "SELECT count(*) FROM operation_state WHERE kind IN ('export','export_delta')",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    eprintln!(
        "SCALE 1395 portraits/4185 files: elapsed={:.3}s journal_bytes={} journal_rows={} phase_times={:?}",
        elapsed.as_secs_f64(),
        journal_size,
        journal_rows,
        phases.lock().unwrap()
    );
    let plan = plan_export(&lib, request("cancelled-out"), &ExportHistory::default()).unwrap();
    let stored = lib.export_plan(plan.id).unwrap();
    let control = Arc::new(Mutex::new(None::<JobContext>));
    let handle = control.clone();
    let cancelled_at = Arc::new(Mutex::new(None));
    let at = cancelled_at.clone();
    let job = JobContext::with_status_progress(move |completed, _, phase| {
        if phase == "Preparing destination folders" && completed == 10 {
            *at.lock().unwrap() = Some(Instant::now());
            handle.lock().unwrap().as_ref().unwrap().cancel();
        }
    });
    *control.lock().unwrap() = Some(job.clone());
    let mut promoted = 0;
    let e = apply_export_with_observer(&lib, &stored, true, &job, &mut |b| {
        if b == ExportBoundary::AfterPromote {
            promoted += 1;
        }
        Ok(())
    })
    .unwrap_err();
    assert_eq!(e.code(), "CANCELLED");
    assert_eq!(promoted, 0);
    let delay = cancelled_at.lock().unwrap().unwrap().elapsed();
    assert!(
        delay < Duration::from_secs(10),
        "cancellation delay {delay:?}"
    );
    eprintln!(
        "SCALE cancellation after 10 prepared folders: returned in {:.3}s, promoted=0",
        delay.as_secs_f64()
    );
}
