mod support;
use portrait_core::{
    Library,
    export::{
        ExportBoundary, ExportHistory, apply_export, apply_export_with_observer, plan_export,
    },
    import::{JobContext, import_portraits},
    types::*,
};
use std::{fs, path::PathBuf};
fn fixture() -> (tempfile::TempDir, Library) {
    let t = tempfile::tempdir().unwrap();
    let mut l = Library::create(&t.path().join("library")).unwrap();
    support::write_portrait_with_dimensions(t.path(), "source", (2, 3), (3, 4), (4, 5));
    import_portraits(
        &mut l,
        ImportRequest {
            path: t.path().join("source"),
            source_name: "test".into(),
            kind: ImportKind::Folder,
            resize: false,
        },
        &JobContext::default(),
    )
    .unwrap();
    (t, l)
}
fn preview(
    l: &Library,
    target: PathBuf,
    output: ExportOutput,
    mode: ExportMode,
) -> portrait_core::export::StoredExportPlan {
    if mode == ExportMode::Replace && output == ExportOutput::Directory {
        l.designate_export_target(&target).unwrap();
    }
    let p = plan_export(
        l,
        ExportRequest {
            target,
            scope: ExportScope::All,
            output,
            mode,
        },
        &ExportHistory::default(),
    )
    .unwrap();
    l.export_plan(p.id).unwrap()
}
#[test]
fn stale_and_confirmation_reject_before_any_write() {
    let (t, l) = fixture();
    let out = t.path().join("out");
    let p = preview(
        &l,
        out.clone(),
        ExportOutput::Directory,
        ExportMode::Replace,
    );
    assert_eq!(
        apply_export(&l, &p, false, &JobContext::default())
            .unwrap_err()
            .code(),
        "EXPORT_CONFIRMATION_REQUIRED"
    );
    assert!(!out.exists());
    fs::create_dir(&out).unwrap();
    fs::write(out.join("user.txt"), "external").unwrap();
    assert_eq!(
        apply_export(&l, &p, true, &JobContext::default())
            .unwrap_err()
            .code(),
        "EXPORT_PLAN_STALE"
    );
    assert_eq!(fs::read_dir(out).unwrap().count(), 1);
}
#[test]
fn merge_twice_and_exact_replacement_preserve_notes() {
    let (t, l) = fixture();
    let out = t.path().join("out");
    support::write_portrait_with_dimensions(&out, "old", (2, 3), (3, 4), (4, 5));
    support::write_portrait_with_dimensions(&out, "notes", (2, 3), (3, 4), (4, 5));
    fs::write(out.join("notes/notes.txt"), "keep").unwrap();
    let p = preview(&l, out.clone(), ExportOutput::Directory, ExportMode::Merge);
    let r = apply_export(&l, &p, true, &JobContext::default()).unwrap();
    assert_eq!(r.added, 3);
    assert!(out.join("old").exists());
    assert_eq!(
        apply_export(&l, &p, true, &JobContext::default())
            .unwrap_err()
            .code(),
        "EXPORT_PLAN_NOT_FOUND"
    );
    let p = preview(
        &l,
        out.clone(),
        ExportOutput::Directory,
        ExportMode::Replace,
    );
    let r = apply_export(&l, &p, true, &JobContext::default()).unwrap();
    assert_eq!(r.overwritten, 3);
    assert_eq!(r.removed, 3);
    assert!(!out.join("old").exists());
    assert_eq!(
        fs::read_to_string(out.join("notes/notes.txt")).unwrap(),
        "keep"
    );
}
#[test]
fn zip_roundtrip_and_overwrite_confirmation() {
    let (t, l) = fixture();
    let out = t.path().join("portraits.zip");
    let p = preview(&l, out.clone(), ExportOutput::Zip, ExportMode::Merge);
    apply_export(&l, &p, true, &JobContext::default()).unwrap();
    let mut z = zip::ZipArchive::new(fs::File::open(&out).unwrap()).unwrap();
    assert_eq!(z.len(), 3);
    for n in 0..z.len() {
        let f = z.by_index(n).unwrap();
        assert!(f.name().starts_with("pm-"));
        assert!(f.name().ends_with(".png"));
    }
    let extracted = t.path().join("zip-extracted");
    z.extract(&extracted).unwrap();
    drop(z);
    let mut imported = Library::create(&t.path().join("roundtrip-library")).unwrap();
    let report = import_portraits(
        &mut imported,
        ImportRequest {
            path: extracted,
            source_name: "ZIP roundtrip".into(),
            kind: ImportKind::Folder,
            resize: false,
        },
        &JobContext::default(),
    )
    .unwrap();
    assert_eq!(report.imported, 1);
    assert_eq!(report.skipped, 0);
    let before = fs::read(&out).unwrap();
    let p = preview(&l, out.clone(), ExportOutput::Zip, ExportMode::Merge);
    assert_eq!(
        apply_export(&l, &p, false, &JobContext::default())
            .unwrap_err()
            .code(),
        "EXPORT_CONFIRMATION_REQUIRED"
    );
    assert_eq!(before, fs::read(out).unwrap());
}
#[test]
fn every_boundary_recovers_precommit_or_finishes_commit() {
    let (t, l) = fixture();
    let out = t.path().join("trace");
    let p = preview(&l, out, ExportOutput::Directory, ExportMode::Merge);
    let mut trace = Vec::new();
    apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
        trace.push(b);
        Ok(())
    })
    .unwrap();
    assert!(trace.contains(&ExportBoundary::BeforeCommit));
    assert!(trace.contains(&ExportBoundary::AfterCommit));
    for fail in 0..trace.len() {
        let (t, l) = fixture();
        let out = t.path().join("out");
        let root = l.root().to_owned();
        let p = preview(&l, out.clone(), ExportOutput::Directory, ExportMode::Merge);
        let mut n = 0;
        let mut committed = false;
        let r = apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
            if b == ExportBoundary::AfterCommit {
                committed = true;
            }
            let stop = n == fail;
            n += 1;
            if stop {
                Err(portrait_core::CoreError::Recovery("simulated crash".into()))
            } else {
                Ok(())
            }
        });
        assert!(r.is_err(), "boundary {fail}");
        let unstamped = interrupted_files(&l);
        drop(l);
        let recovered = reopen_after_interruption(&root, unstamped);
        let pngs = if out.exists() {
            fs::read_dir(&out)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .map(|e| fs::read_dir(e.path()).unwrap().count())
                .sum::<usize>()
        } else {
            0
        };
        assert_eq!(pngs, if committed { 3 } else { 0 }, "boundary {fail}");
        drop(recovered);
    }
}
fn interrupted_files(l: &Library) -> Vec<(PathBuf, Vec<u8>)> {
    let mut q=l.connection().prepare("SELECT kind,state_json FROM operation_state WHERE kind IN ('export','export_delta') ORDER BY operation_id").unwrap();
    let rows = q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut intended = std::collections::BTreeSet::new();
    let mut complete = std::collections::BTreeSet::new();
    let mut moved = false;
    for (kind, raw) in rows {
        let j: serde_json::Value = serde_json::from_str(&raw).unwrap();
        if kind == "export" {
            for p in j["staged_files"].as_array().unwrap() {
                intended.insert(PathBuf::from(p.as_str().unwrap()));
            }
            for p in j["staged_stamps"].as_object().unwrap().keys() {
                complete.insert(PathBuf::from(p));
            }
            moved |= !j["moves"].as_array().unwrap().is_empty();
        } else {
            for e in j["events"].as_array().unwrap() {
                match e["kind"].as_str().unwrap() {
                    "stageIntent" => {
                        intended.insert(PathBuf::from(e["path"].as_str().unwrap()));
                    }
                    "stageComplete" => {
                        complete.insert(PathBuf::from(e["path"].as_str().unwrap()));
                    }
                    "moves" => moved = true,
                    _ => {}
                }
            }
        }
    }
    let files: Vec<_> = intended
        .difference(&complete)
        .filter(|p| p.is_file())
        .map(|p| (p.clone(), fs::read(p).unwrap()))
        .collect();
    if !files.is_empty() {
        assert!(!moved, "ambiguous staging must precede all target moves");
    }
    files
}
fn reopen_after_interruption(
    root: &std::path::Path,
    unstamped: Vec<(PathBuf, Vec<u8>)>,
) -> Library {
    if unstamped.is_empty() {
        return Library::open(root).unwrap();
    }
    let error = Library::open(root).unwrap_err();
    assert_eq!(error.code(), "RECOVERY_FAILED");
    for (path, bytes) in unstamped {
        assert_eq!(
            fs::read(&path).unwrap(),
            bytes,
            "ambiguous file retained byte-for-byte"
        );
        fs::remove_file(path).unwrap();
    }
    // Simulate the user moving the specifically identified ambiguous scratch file aside.
    Library::open(root).unwrap()
}
fn tree(path: &std::path::Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn walk(
        root: &std::path::Path,
        path: &std::path::Path,
        out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>,
    ) {
        if !path.exists() {
            return;
        }
        for e in fs::read_dir(path).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                walk(root, &e.path(), out)
            } else {
                out.insert(
                    e.path().strip_prefix(root).unwrap().into(),
                    fs::read(e.path()).unwrap(),
                );
            }
        }
    }
    let mut result = Default::default();
    walk(path, path, &mut result);
    result
}
#[test]
fn overwrite_and_zip_crash_boundaries_restore_original_bytes() {
    for output in [ExportOutput::Directory, ExportOutput::Zip] {
        let (t, l) = fixture();
        let target = t.path().join("out");
        let first = preview(&l, target.clone(), output, ExportMode::Merge);
        apply_export(&l, &first, true, &JobContext::default()).unwrap();
        let p = preview(&l, target, output, ExportMode::Merge);
        let mut trace = Vec::new();
        apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
            trace.push(b);
            Ok(())
        })
        .unwrap();
        assert!(trace.contains(&ExportBoundary::AfterBackup));
        for fail in 0..trace.len() {
            let (t, l) = fixture();
            let root = l.root().to_owned();
            let out = t.path().join("out");
            let first = preview(&l, out.clone(), output, ExportMode::Merge);
            apply_export(&l, &first, true, &JobContext::default()).unwrap();
            if output == ExportOutput::Zip {
                fs::write(&out, b"original archive bytes").unwrap()
            } else {
                let f = tree(&out).keys().next().unwrap().clone();
                fs::write(out.join(f), b"locally modified").unwrap()
            };
            let before = if output == ExportOutput::Zip {
                fs::read(&out).unwrap()
            } else {
                serde_json::to_vec(&tree(&out)).unwrap()
            };
            let p = preview(&l, out.clone(), output, ExportMode::Merge);
            let mut n = 0;
            let mut committed = false;
            let r = apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
                if b == ExportBoundary::AfterCommit {
                    committed = true
                }
                let stop = n == fail;
                n += 1;
                if stop {
                    Err(portrait_core::CoreError::Recovery("simulated crash".into()))
                } else {
                    Ok(())
                }
            });
            assert!(r.is_err());
            let unstamped = interrupted_files(&l);
            drop(l);
            let reopened = reopen_after_interruption(&root, unstamped);
            if !committed {
                let after = if output == ExportOutput::Zip {
                    fs::read(&out).unwrap()
                } else {
                    serde_json::to_vec(&tree(&out)).unwrap()
                };
                assert_eq!(after, before, "{output:?} boundary {fail}");
            }
            drop(reopened);
        }
    }
}
#[test]
fn cancellation_before_commit_rolls_back_after_commit_finishes() {
    for boundary in [
        ExportBoundary::AfterBackup,
        ExportBoundary::BeforeCommit,
        ExportBoundary::AfterCommit,
    ] {
        let (t, l) = fixture();
        let out = t.path().join("out");
        let p = preview(&l, out.clone(), ExportOutput::Directory, ExportMode::Merge);
        apply_export(&l, &p, true, &JobContext::default()).unwrap();
        let before = tree(&out);
        let p = preview(&l, out.clone(), ExportOutput::Directory, ExportMode::Merge);
        let job = JobContext::default();
        let r = apply_export_with_observer(&l, &p, true, &job, &mut |b| {
            if b == boundary {
                job.cancel()
            }
            Ok(())
        });
        if boundary == ExportBoundary::AfterCommit {
            assert!(r.is_ok())
        } else {
            assert_eq!(r.unwrap_err().code(), "CANCELLED");
            assert_eq!(tree(&out), before)
        };
        assert_eq!(
            l.connection()
                .query_row(
                    "SELECT count(*) FROM operation_state WHERE kind='export'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }
}
#[test]
fn external_change_after_backup_is_preserved_and_recovery_retries() {
    let (t, l) = fixture();
    let root = l.root().to_owned();
    let out = t.path().join("out");
    let p = preview(&l, out.clone(), ExportOutput::Directory, ExportMode::Merge);
    apply_export(&l, &p, true, &JobContext::default()).unwrap();
    let p = preview(&l, out.clone(), ExportOutput::Directory, ExportMode::Merge);
    let target = p
        .plan
        .actions
        .iter()
        .find(|a| a.kind == ExportActionKind::Overwrite)
        .unwrap()
        .path
        .clone();
    let mut changed = false;
    let r = apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
        if b == ExportBoundary::AfterBackup && !changed {
            fs::write(&target, "new user content").unwrap();
            changed = true
        }
        Ok(())
    });
    assert_eq!(r.unwrap_err().code(), "RECOVERY_FAILED");
    assert_eq!(fs::read_to_string(&target).unwrap(), "new user content");
    drop(l);
    assert_eq!(Library::open(&root).unwrap_err().code(), "RECOVERY_FAILED");
    fs::rename(&target, t.path().join("saved-user-file")).unwrap();
    let reopened = Library::open(&root).unwrap();
    assert!(image::image_dimensions(&target).is_ok());
    drop(reopened);
}
#[test]
fn missing_backup_and_original_must_not_silently_retire_journal() {
    let (t, l) = fixture();
    let root = l.root().to_owned();
    let out = t.path().join("out");
    let p = preview(&l, out.clone(), ExportOutput::Directory, ExportMode::Merge);
    apply_export(&l, &p, true, &JobContext::default()).unwrap();
    let p = preview(&l, out, ExportOutput::Directory, ExportMode::Merge);
    let target = p.plan.actions[0].path.clone();
    let _ = apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
        if b == ExportBoundary::BeforeBackup {
            fs::remove_file(&target).unwrap();
            Err(portrait_core::CoreError::Recovery("crash".into()))
        } else {
            Ok(())
        }
    });
    drop(l);
    assert_eq!(Library::open(&root).unwrap_err().code(), "RECOVERY_FAILED");
}
#[test]
fn source_links_and_staging_time_destination_changes_do_not_write_target() {
    let (t, l) = fixture();
    let out = t.path().join("out");
    let p = preview(&l, out.clone(), ExportOutput::Directory, ExportMode::Merge);
    let mut changed = false;
    let r = apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
        if b == ExportBoundary::AfterStage && !changed {
            fs::create_dir(&out).unwrap();
            fs::write(out.join("external"), "keep").unwrap();
            changed = true
        }
        Ok(())
    });
    assert_eq!(r.unwrap_err().code(), "EXPORT_PLAN_STALE");
    assert_eq!(tree(&out).len(), 1);
    #[cfg(unix)]
    {
        let asset: String = l
            .connection()
            .query_row("SELECT relative_path FROM assets LIMIT 1", [], |r| r.get(0))
            .unwrap();
        let asset = l.root().join(asset);
        let original = fs::read(&asset).unwrap();
        fs::write(t.path().join("outside.png"), original).unwrap();
        fs::remove_file(&asset).unwrap();
        std::os::unix::fs::symlink(t.path().join("outside.png"), asset).unwrap();
        let p = preview(
            &l,
            t.path().join("other"),
            ExportOutput::Directory,
            ExportMode::Merge,
        );
        assert_eq!(
            apply_export(&l, &p, true, &JobContext::default())
                .unwrap_err()
                .code(),
            "LIBRARY_MANAGED_PATH_UNSAFE"
        );
        assert!(!t.path().join("other").exists());
    }
}
#[test]
fn staged_directory_replacement_and_disconnect_preserve_both_trees() {
    for link in [false, true] {
        let (t, l) = fixture();
        let out = t.path().join("out");
        let p = preview(&l, out.clone(), ExportOutput::Directory, ExportMode::Merge);
        let stage = t.path().join(format!(".portrait-export-{}", p.plan.id));
        let detached = t.path().join("detached");
        let mut changed = false;
        let r = apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
            if b == ExportBoundary::BeforeStage && !changed {
                fs::rename(&stage, &detached).unwrap();
                if link {
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(&detached, &stage).unwrap();
                } else {
                    fs::create_dir(&stage).unwrap();
                    fs::write(stage.join("user.txt"), "keep").unwrap();
                }
                changed = true;
            }
            Ok(())
        });
        assert!(r.is_err());
        assert!(!out.exists());
        assert_eq!(fs::read_dir(&detached).unwrap().count(), 0);
        if !link {
            assert_eq!(fs::read_to_string(stage.join("user.txt")).unwrap(), "keep")
        }
    }
}
#[test]
fn faults_report_insufficient_space_and_disconnection_without_touching_old_archive() {
    for code in [libc::ENOSPC, libc::EIO, libc::ENODEV] {
        let (t, l) = fixture();
        let out = t.path().join("out.zip");
        fs::write(&out, "old archive").unwrap();
        let p = preview(&l, out.clone(), ExportOutput::Zip, ExportMode::Merge);
        let root = l.root().to_owned();
        let r = apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
            if b == ExportBoundary::BeforePromote {
                Err(std::io::Error::from_raw_os_error(code).into())
            } else {
                Ok(())
            }
        });
        assert_eq!(r.unwrap_err().code(), "FILESYSTEM_ERROR");
        drop(l);
        let reopened = Library::open(&root).unwrap();
        assert_eq!(fs::read_to_string(out).unwrap(), "old archive");
        drop(reopened);
    }
}
#[test]
fn recovery_rejects_tampered_journal_paths() {
    let (t, l) = fixture();
    let out = t.path().join("out");
    let p = preview(&l, out, ExportOutput::Directory, ExportMode::Merge);
    let root = l.root().to_owned();
    let _ = apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
        if b == ExportBoundary::BeforeCommit {
            Err(portrait_core::CoreError::Recovery("crash".into()))
        } else {
            Ok(())
        }
    });
    let raw: String = l
        .connection()
        .query_row(
            "SELECT state_json FROM operation_state WHERE kind='export'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let unrelated = t.path().join("unrelated.txt");
    fs::write(&unrelated, "keep").unwrap();
    json["staged_files"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!(unrelated));
    l.connection()
        .execute(
            "UPDATE operation_state SET state_json=? WHERE kind='export'",
            [json.to_string()],
        )
        .unwrap();
    drop(l);
    assert_eq!(Library::open(&root).unwrap_err().code(), "RECOVERY_FAILED");
    assert_eq!(fs::read_to_string(unrelated).unwrap(), "keep");
}
#[test]
fn externally_modified_staging_file_is_not_exported_or_deleted() {
    let (t, l) = fixture();
    let out = t.path().join("out");
    let p = preview(&l, out.clone(), ExportOutput::Directory, ExportMode::Merge);
    let file = t
        .path()
        .join(format!(".portrait-export-{}/asset-0", p.plan.id));
    let mut changed = false;
    let result = apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
        if b == ExportBoundary::AfterStage && !changed {
            fs::write(&file, "external staging data").unwrap();
            changed = true;
        }
        Ok(())
    });
    assert!(result.is_err());
    assert!(!out.exists());
    assert_eq!(fs::read_to_string(file).unwrap(), "external staging data");
}
#[test]
fn recovery_itself_can_be_interrupted_at_every_move_cleanup_and_journal_boundary() {
    fn pending() -> (
        tempfile::TempDir,
        Library,
        PathBuf,
        std::collections::BTreeMap<PathBuf, Vec<u8>>,
    ) {
        let (t, l) = fixture();
        let out = t.path().join("out");
        let p = preview(&l, out.clone(), ExportOutput::Directory, ExportMode::Merge);
        apply_export(&l, &p, true, &JobContext::default()).unwrap();
        support::write_portrait_with_dimensions(&out, "old", (2, 3), (3, 4), (4, 5));
        let before = tree(&out);
        let p = preview(
            &l,
            out.clone(),
            ExportOutput::Directory,
            ExportMode::Replace,
        );
        let r = apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
            if b == ExportBoundary::BeforeCommit {
                Err(portrait_core::CoreError::Recovery("crash".into()))
            } else {
                Ok(())
            }
        });
        assert!(r.is_err());
        (t, l, out, before)
    }
    let (_t, l, _, _) = pending();
    let mut trace = Vec::new();
    portrait_core::export::recover_export_operations_with_observer(
        l.root(),
        l.connection(),
        &mut |b| {
            trace.push(b);
            Ok(())
        },
    )
    .unwrap();
    assert!(trace.contains(&ExportBoundary::AfterRecoveryRename));
    for fail in 0..trace.len() {
        let (_t, l, out, before) = pending();
        let root = l.root().to_owned();
        let mut n = 0;
        let r = portrait_core::export::recover_export_operations_with_observer(
            l.root(),
            l.connection(),
            &mut |_| {
                let stop = n == fail;
                n += 1;
                if stop {
                    Err(portrait_core::CoreError::Recovery("second crash".into()))
                } else {
                    Ok(())
                }
            },
        );
        assert!(r.is_err());
        drop(l);
        let recovered = Library::open(&root).unwrap();
        assert_eq!(tree(&out), before, "recovery boundary {fail}");
        drop(recovered);
    }
}
#[test]
fn existing_collection_stages_on_its_own_filesystem() {
    let (t, l) = fixture();
    let out = t.path().join("existing-collection");
    fs::create_dir(&out).unwrap();
    let p = preview(&l, out.clone(), ExportOutput::Directory, ExportMode::Merge);
    let mut checked = false;
    apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
        if b == ExportBoundary::AfterStage && !checked {
            assert!(
                out.join(format!(".portrait-export-{}", p.plan.id)).is_dir(),
                "stage inside the destination so an existing mount point uses its own filesystem"
            );
            checked = true;
        }
        Ok(())
    })
    .unwrap();
    assert!(checked);
}
#[test]
fn committed_cleanup_can_be_interrupted_without_rolling_back_the_archive() {
    fn committed() -> (tempfile::TempDir, Library, PathBuf, Vec<u8>) {
        let (t, l) = fixture();
        let out = t.path().join("out.zip");
        fs::write(&out, "old archive").unwrap();
        let p = preview(&l, out.clone(), ExportOutput::Zip, ExportMode::Merge);
        let r = apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
            if b == ExportBoundary::AfterCommit {
                Err(portrait_core::CoreError::Recovery("crash".into()))
            } else {
                Ok(())
            }
        });
        assert!(r.is_err());
        let expected = fs::read(&out).unwrap();
        (t, l, out, expected)
    }
    let (_t, l, _, _) = committed();
    let mut trace = Vec::new();
    portrait_core::export::recover_export_operations_with_observer(
        l.root(),
        l.connection(),
        &mut |b| {
            trace.push(b);
            Ok(())
        },
    )
    .unwrap();
    assert!(trace.contains(&ExportBoundary::AfterRecoveryUnlink));
    for fail in 0..trace.len() {
        let (_t, l, out, expected) = committed();
        let root = l.root().to_owned();
        let mut n = 0;
        let r = portrait_core::export::recover_export_operations_with_observer(
            l.root(),
            l.connection(),
            &mut |_| {
                let stop = n == fail;
                n += 1;
                if stop {
                    Err(portrait_core::CoreError::Recovery("second crash".into()))
                } else {
                    Ok(())
                }
            },
        );
        assert!(r.is_err());
        drop(l);
        let recovered = Library::open(&root).unwrap();
        assert_eq!(fs::read(out).unwrap(), expected);
        drop(recovered);
    }
}

#[test]
fn committed_replacement_directory_cleanup_retries_after_directory_removal() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("library");
    let l = Library::create(&root).unwrap();
    let out = t.path().join("out");
    support::write_portrait_with_dimensions(&out, "old", (2, 3), (3, 4), (4, 5));
    let p = preview(
        &l,
        out.clone(),
        ExportOutput::Directory,
        ExportMode::Replace,
    );
    apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
        if b == ExportBoundary::AfterCommit {
            Err(portrait_core::CoreError::Recovery("crash".into()))
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    portrait_core::export::recover_export_operations_with_observer(
        l.root(),
        l.connection(),
        &mut |b| {
            if b == ExportBoundary::BeforeRecoveryRetire {
                Err(portrait_core::CoreError::Recovery("cleanup crash".into()))
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert!(!out.join("old").exists());
    drop(l);
    let recovered = Library::open(&root).unwrap();
    assert!(out.is_dir());
    drop(recovered);
}
#[test]
fn exclusive_creation_collision_is_retained_without_cleanup_ownership() {
    let (t, l) = fixture();
    let root = l.root().to_owned();
    let out = t.path().join("out");
    let p = preview(&l, out.clone(), ExportOutput::Directory, ExportMode::Merge);
    let file = t
        .path()
        .join(format!(".portrait-export-{}/asset-0", p.plan.id));
    let mut once = false;
    let e = apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
        if b == ExportBoundary::BeforeStage && !once {
            fs::write(&file, b"external user data").unwrap();
            once = true;
        }
        Ok(())
    })
    .unwrap_err();
    assert_eq!(e.code(), "RECOVERY_FAILED");
    assert_eq!(fs::read(&file).unwrap(), b"external user data");
    assert!(!out.exists());
    drop(l);
    assert!(Library::open(&root).is_err());
    assert_eq!(fs::read(file).unwrap(), b"external user data");
}
#[test]
fn export_journal_header_stays_bounded_while_changes_are_durable() {
    let (t, l) = fixture();
    let p = preview(
        &l,
        t.path().join("out"),
        ExportOutput::Directory,
        ExportMode::Merge,
    );
    let mut inspected = false;
    let result = apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
        if b == ExportBoundary::BeforeCommit {
            let size: i64 = l
                .connection()
                .query_row(
                    "SELECT length(state_json) FROM operation_state WHERE kind='export'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(
                size < 2048,
                "journal header is {size} bytes; changes must use incremental durable records"
            );
            inspected = true;
        }
        Ok(())
    });
    assert!(result.is_ok());
    assert!(inspected);
}
#[test]
fn interrupted_partial_staging_retains_bytes_without_a_durable_complete_stamp() {
    let (t, l) = fixture();
    let root = l.root().to_owned();
    let out = t.path().join("out");
    let p = preview(&l, out.clone(), ExportOutput::Directory, ExportMode::Merge);
    let file = t
        .path()
        .join(format!(".portrait-export-{}/asset-0", p.plan.id));
    apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
        if b == ExportBoundary::AfterStageChunk {
            Err(portrait_core::CoreError::Recovery(
                "crash during copy".into(),
            ))
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    let bytes = fs::read(&file).unwrap();
    assert!(!bytes.is_empty());
    drop(l);
    assert_eq!(Library::open(&root).unwrap_err().code(), "RECOVERY_FAILED");
    assert_eq!(fs::read(file).unwrap(), bytes);
    assert!(!out.exists());
}
#[test]
fn cancel_during_directory_preparation_stops_before_promotion_and_reports_phase() {
    let (t, l) = fixture();
    let out = t.path().join("out");
    let p = preview(&l, out.clone(), ExportOutput::Directory, ExportMode::Merge);
    let shared = std::sync::Arc::new(std::sync::Mutex::new(None::<JobContext>));
    let capture = shared.clone();
    let phases = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let events = phases.clone();
    let job = JobContext::with_status_progress(move |completed, _, phase| {
        events.lock().unwrap().push(phase.to_owned());
        if phase == "Preparing destination folders" && completed == 1 {
            capture.lock().unwrap().as_ref().unwrap().cancel();
        }
    });
    *shared.lock().unwrap() = Some(job.clone());
    let e = apply_export(&l, &p, true, &job).unwrap_err();
    assert_eq!(e.code(), "CANCELLED");
    assert_eq!(tree(&out).len(), 0);
    let phases = phases.lock().unwrap();
    assert!(phases.iter().any(|p| p == "Staging portrait sets"));
    assert!(phases.iter().any(|p| p == "Preparing destination folders"));
    assert!(!phases.iter().any(|p| p == "Writing portrait files"));
}
fn convert_to_legacy_journal(l: &Library) {
    let mut snapshot: serde_json::Value = serde_json::from_str(
        &l.connection()
            .query_row(
                "SELECT state_json FROM operation_state WHERE kind='export'",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap(),
    )
    .unwrap();
    let mut query=l.connection().prepare("SELECT state_json FROM operation_state WHERE kind='export_delta' ORDER BY operation_id").unwrap();
    let rows = query
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    drop(query);
    for raw in rows {
        let delta: serde_json::Value = serde_json::from_str(&raw).unwrap();
        for event in delta["events"].as_array().unwrap() {
            let path = event["path"].as_str().unwrap_or("");
            match event["kind"].as_str().unwrap() {
                "parent" => {
                    snapshot["parents"]
                        .as_object_mut()
                        .unwrap()
                        .insert(path.into(), event["identity"].clone());
                }
                "createdDirectory" => snapshot["created"]
                    .as_array_mut()
                    .unwrap()
                    .push(event["path"].clone()),
                "stageIntent" => snapshot["staged_files"]
                    .as_array_mut()
                    .unwrap()
                    .push(event["path"].clone()),
                "stageCreated" => {}
                "stageComplete" => {
                    snapshot["staged_stamps"]
                        .as_object_mut()
                        .unwrap()
                        .insert(path.into(), event["stamp"].clone());
                }
                "moves" => snapshot["moves"] = event["moves"].clone(),
                "committed" => snapshot["committed"] = serde_json::json!(true),
                _ => panic!("unexpected event"),
            }
        }
    }
    snapshot.as_object_mut().unwrap().remove("version");
    snapshot.as_object_mut().unwrap().remove("staged_creation");
    let tx = l.connection().unchecked_transaction().unwrap();
    tx.execute(
        "UPDATE operation_state SET state_json=? WHERE kind='export'",
        [snapshot.to_string()],
    )
    .unwrap();
    tx.execute("DELETE FROM operation_state WHERE kind='export_delta'", [])
        .unwrap();
    tx.commit().unwrap();
}
#[test]
fn original_unversioned_journals_recover_precommit_and_committed_replacement() {
    for committed in [false, true] {
        let (t, l) = fixture();
        let root = l.root().to_owned();
        let out = t.path().join("out");
        support::write_portrait_with_dimensions(&out, "old", (2, 3), (3, 4), (4, 5));
        let before = tree(&out);
        let p = preview(
            &l,
            out.clone(),
            ExportOutput::Directory,
            ExportMode::Replace,
        );
        let boundary = if committed {
            ExportBoundary::AfterCommit
        } else {
            ExportBoundary::AfterPromote
        };
        apply_export_with_observer(&l, &p, true, &JobContext::default(), &mut |b| {
            if b == boundary {
                Err(portrait_core::CoreError::Recovery(
                    "crash in old version".into(),
                ))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        convert_to_legacy_journal(&l);
        if committed {
            portrait_core::export::recover_export_operations_with_observer(
                l.root(),
                l.connection(),
                &mut |b| {
                    if b == ExportBoundary::BeforeRecoveryRetire {
                        Err(portrait_core::CoreError::Recovery(
                            "legacy cleanup crash".into(),
                        ))
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap_err();
            assert!(!out.join("old").exists());
        }
        drop(l);
        let reopened = Library::open(&root).unwrap();
        if !committed {
            assert_eq!(tree(&out), before)
        } else {
            assert_eq!(tree(&out).len(), 3)
        }
        drop(reopened);
    }
}
#[test]
fn incomplete_zip_is_retained_after_crash_and_keeps_previous_archive() {
    let (t, l) = fixture();
    let root = l.root().to_owned();
    let out = t.path().join("out.zip");
    fs::write(&out, "previous archive").unwrap();
    let p = preview(&l, out.clone(), ExportOutput::Zip, ExportMode::Merge);
    let archive = t
        .path()
        .join(format!(".portrait-export-{}/archive.zip", p.plan.id));
    let mut chunks = 0;
    let job = JobContext::default();
    let e = apply_export_with_observer(&l, &p, true, &job, &mut |b| {
        if b == ExportBoundary::AfterStageChunk {
            chunks += 1;
            if chunks == 4 {
                return Err(portrait_core::CoreError::Recovery("crash mid ZIP".into()));
            }
        }
        Ok(())
    })
    .unwrap_err();
    assert_eq!(e.code(), "RECOVERY_FAILED");
    let bytes = fs::read(&archive).unwrap();
    assert!(!bytes.is_empty());
    assert_eq!(fs::read_to_string(&out).unwrap(), "previous archive");
    drop(l);
    assert_eq!(Library::open(&root).unwrap_err().code(), "RECOVERY_FAILED");
    assert_eq!(fs::read(archive).unwrap(), bytes);
    assert_eq!(fs::read_to_string(out).unwrap(), "previous archive");
}

#[test]
fn ordinary_mid_zip_cancellation_retires_its_journal_and_allows_reopen_and_export() {
    for cancellation_chunk in 3..=6 {
        let (t, l) = fixture();
        let root = l.root().to_owned();
        let out = t.path().join("out.zip");
        fs::write(&out, "previous archive").unwrap();
        let p = preview(&l, out.clone(), ExportOutput::Zip, ExportMode::Merge);
        let stage = t.path().join(format!(".portrait-export-{}", p.plan.id));
        let job = JobContext::default();
        let mut chunks = 0;
        let e = apply_export_with_observer(&l, &p, true, &job, &mut |b| {
            if cancellation_chunk == 3
                && b == ExportBoundary::AfterStageCreate
                && stage.join("archive.zip").exists()
            {
                job.cancel();
            }
            if cancellation_chunk != 3 && b == ExportBoundary::AfterStageChunk {
                chunks += 1;
                if chunks == cancellation_chunk {
                    job.cancel()
                }
            }
            Ok(())
        })
        .unwrap_err();
        assert_eq!(e.code(), "CANCELLED");
        assert!(!e.to_string().contains("Recovery data was retained"));
        assert_eq!(fs::read_to_string(&out).unwrap(), "previous archive");
        assert!(!stage.exists());
        assert_eq!(
            l.connection()
                .query_row(
                    "SELECT count(*) FROM operation_state WHERE kind IN ('export','export_delta')",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        drop(l);
        let reopened = Library::open(&root).unwrap();
        let p = preview(&reopened, out.clone(), ExportOutput::Zip, ExportMode::Merge);
        apply_export(&reopened, &p, true, &JobContext::default()).unwrap();
        assert_eq!(
            zip::ZipArchive::new(fs::File::open(out).unwrap())
                .unwrap()
                .len(),
            3
        );
    }
}

#[test]
fn external_edit_during_live_zip_cancellation_is_retained() {
    for same_length in [false, true] {
        let (t, l) = fixture();
        let root = l.root().to_owned();
        let out = t.path().join("out.zip");
        fs::write(&out, "previous archive").unwrap();
        let p = preview(&l, out.clone(), ExportOutput::Zip, ExportMode::Merge);
        let archive = t
            .path()
            .join(format!(".portrait-export-{}/archive.zip", p.plan.id));
        let job = JobContext::default();
        let mut chunks = 0;
        let mut external_bytes = Vec::new();
        let e = apply_export_with_observer(&l, &p, true, &job, &mut |b| {
            if b == ExportBoundary::AfterStageChunk {
                chunks += 1;
                if chunks == 4 {
                    external_bytes = if same_length {
                        let mut bytes = fs::read(&archive).unwrap();
                        bytes[0] ^= 255;
                        bytes
                    } else {
                        b"external archive data".to_vec()
                    };
                    fs::write(&archive, &external_bytes).unwrap();
                    job.cancel()
                }
            }
            Ok(())
        })
        .unwrap_err();
        assert_eq!(e.code(), "CANCELLED");
        assert_eq!(fs::read(&archive).unwrap(), external_bytes);
        assert_eq!(fs::read_to_string(&out).unwrap(), "previous archive");
        drop(l);
        assert_eq!(Library::open(&root).unwrap_err().code(), "RECOVERY_FAILED");
        assert_eq!(fs::read(archive).unwrap(), external_bytes);
    }
}
