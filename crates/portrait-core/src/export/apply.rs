use super::{
    ExportHistory, StoredExportPlan,
    journal::{self, ExportReceipt, Journal, Move, Stamp, checked_parent, mkdir, split, stamp},
    plan, stable_folder_name,
};
use crate::{
    CoreError, Library, Result,
    import::JobContext,
    types::{ExportActionKind as Kind, ExportOutput, ExportReport, Issue, IssueSeverity, Role},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Mutex, OnceLock},
};

/// Observer errors simulate process interruption: retain durable intent for startup recovery.
/// Production uses the no-op observer; ordinary I/O/cancellation errors recover immediately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportBoundary {
    BeforeJournal,
    AfterJournal,
    BeforeCreateDirectory,
    AfterCreateDirectory,
    BeforeStage,
    AfterStage,
    AfterStageCreate,
    AfterStageChunk,
    BeforeBackup,
    AfterBackup,
    BeforePromote,
    AfterPromote,
    BeforeCommit,
    AfterCommit,
    BeforeCleanup,
    AfterCleanup,
    BeforeRecoveryRename,
    AfterRecoveryRename,
    BeforeRecoveryUnlink,
    AfterRecoveryUnlink,
    BeforeRecoveryRetire,
    AfterRecoveryRetire,
}
type Observer<'a> = &'a mut dyn FnMut(ExportBoundary) -> Result<()>;
struct Hooks<'a> {
    callback: Observer<'a>,
    interrupted: bool,
}
impl Hooks<'_> {
    fn hit(&mut self, b: ExportBoundary) -> Result<()> {
        let r = (self.callback)(b);
        self.interrupted |= r.is_err();
        r
    }
    fn save(&mut self, j: &mut Journal, l: &Library) -> Result<()> {
        self.hit(ExportBoundary::BeforeJournal)?;
        j.save(l.connection())?;
        self.hit(ExportBoundary::AfterJournal)
    }
}

pub fn apply_export(
    lib: &Library,
    plan: &StoredExportPlan,
    confirmed: bool,
    job: &JobContext,
) -> Result<ExportReport> {
    apply_export_with_observer(lib, plan, confirmed, job, &mut |_| Ok(()))
}
pub fn apply_export_with_observer(
    lib: &Library,
    requested: &StoredExportPlan,
    confirmed: bool,
    job: &JobContext,
    observer: Observer<'_>,
) -> Result<ExportReport> {
    // Serialize app exports across libraries as well as the desktop library writer lock.
    static EXPORT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _lock = EXPORT_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| CoreError::Recovery("Export lock poisoned".into()))?;
    journal::recover_exports(lib.root(), lib.connection())?;
    let registered = lib.export_plan(requested.plan.id)?;
    let _destination_lock = destination_lock(&registered.plan.target)?;
    let stored = lib.consume_export_plan(requested.plan.id, confirmed)?; // Ignore all caller-owned fields except registry lookup ID.
    check_cancel(job)?;
    let mut hooks = Hooks {
        callback: observer,
        interrupted: false,
    };
    let mut ancestor = stored
        .plan
        .target
        .parent()
        .ok_or(CoreError::ExportTargetUnsafe)?;
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or(CoreError::ExportTargetUnsafe)?;
    }
    if stored.request.output == ExportOutput::Directory && stored.plan.target.is_dir() {
        ancestor = &stored.plan.target;
    }
    let staging = ancestor.join(format!(".portrait-export-{}", stored.plan.id));
    let mut j = Journal {
        version: 2,
        staged_creation: BTreeMap::new(),
        pending: Vec::new(),
        sequence: 0,
        persisted: false,
        id: stored.plan.id,
        output: stored.request.output,
        target: stored.plan.target.clone(),
        staging,
        committed: false,
        parents: BTreeMap::new(),
        created: Vec::new(),
        moves: Vec::new(),
        staged_files: Vec::new(),
        staged_stamps: BTreeMap::new(),
        report: ExportReport::default(),
        history: ExportHistory {
            library_id: Some(lib.id()),
            destination_id: stored.destination_id,
            destination: Some(stored.plan.target.clone()),
            file_digests: BTreeMap::new(),
        },
    };
    j.track_parent(ancestor)?;
    hooks.save(&mut j, lib)?;
    let result = run(lib, &stored, job, &mut j, &mut hooks);
    match result {
        Ok(()) => Ok(j.report),
        Err(e) if hooks.interrupted => Err(e),
        Err(e) => {
            job.report_status(0, None, "Restoring previous files");
            match journal::recover_one(lib.connection(), &j) {
                Ok(()) => Err(e),
                Err(recovery) if matches!(e, CoreError::Cancelled) => {
                    Err(CoreError::CancelledRecoveryPending(recovery.to_string()))
                }
                Err(recovery) => Err(CoreError::Recovery(format!("{e}; {recovery}"))),
            }
        }
    }
}
fn run(
    lib: &Library,
    p: &StoredExportPlan,
    job: &JobContext,
    j: &mut Journal,
    h: &mut Hooks<'_>,
) -> Result<()> {
    h.hit(ExportBoundary::BeforeCreateDirectory)?;
    mkdir(&j.staging, &j.parents)?;
    j.track_parent(&j.staging.clone())?;
    h.save(j, lib)?;
    h.hit(ExportBoundary::AfterCreateDirectory)?;
    // Source validation/copy uses opened non-link parents and bounded, decoded PNGs.
    let mut staged = BTreeMap::new();
    job.report_status(
        0,
        Some(p.portrait_ids.len() as u64),
        "Staging portrait sets",
    );
    for (index, id) in p.portrait_ids.iter().enumerate() {
        check_cancel(job)?;
        let active: bool = lib.connection().query_row(
            "SELECT EXISTS(SELECT 1 FROM portraits WHERE id=? AND trashed_at IS NULL)",
            [id.to_string()],
            |r| r.get(0),
        )?;
        if !active {
            return Err(CoreError::ExportPlanStale);
        }
        for (role, name) in [
            (Role::Small, "Small.png"),
            (Role::Medium, "Medium.png"),
            (Role::Large, "Fulllength.png"),
        ] {
            let role = match role {
                Role::Small => "small",
                Role::Medium => "medium",
                Role::Large => "large",
            };
            let raw: String = lib.connection().query_row(
                "SELECT relative_path FROM assets WHERE portrait_id=? AND role=?",
                rusqlite::params![id.to_string(), role],
                |r| r.get(0),
            )?;
            let rel = Path::new(&raw);
            if rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
                return Err(CoreError::UnsafeManagedDirectory);
            }
            let source = lib.root().join(rel);
            let (parent, name_source) =
                split(&source).map_err(|_| CoreError::UnsafeManagedDirectory)?;
            let mut file = parent
                .child(name_source, false)
                .map_err(|_| CoreError::UnsafeManagedDirectory)?;
            let meta = file.metadata()?;
            if !meta.is_file() || meta.len() > 64 * 1024 * 1024 {
                return Err(CoreError::UnsafeManagedDirectory);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if meta.nlink() != 1 {
                    return Err(CoreError::UnsafeManagedDirectory);
                }
            }
            let dest = j.staging.join(format!("asset-{}", staged.len()));
            j.intent_stage(dest.clone());
            h.save(j, lib)?;
            h.hit(ExportBoundary::BeforeStage)?;
            check_cancel(job)?;
            let (parent, n) = checked_parent(&dest, &j.parents)?;
            let mut output = parent.child(n, true)?;
            j.created_stage(dest.clone(), &output)?;
            let mut written = Sha256::new();
            let creation = j.staged_creation[&dest].clone();
            j.staged_stamps.insert(
                dest.clone(),
                Stamp {
                    identity: creation.clone(),
                    digest: format!("{:x}", written.clone().finalize()),
                },
            );
            h.save(j, lib)?;
            h.hit(ExportBoundary::AfterStageCreate)?;
            let mut buffer = [0; 65536];
            let mut size = 0;
            loop {
                check_cancel(job)?;
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                size += n;
                if size > 64 * 1024 * 1024 {
                    return Err(CoreError::UnsafeManagedDirectory);
                }
                output.write_all(&buffer[..n])?;
                written.update(&buffer[..n]);
                // In-process cleanup can prove the exact successful writes. Crash recovery
                // remains conservative until the completed stamp is durably recorded.
                j.staged_stamps.insert(
                    dest.clone(),
                    Stamp {
                        identity: creation.clone(),
                        digest: format!("{:x}", written.clone().finalize()),
                    },
                );
                h.hit(ExportBoundary::AfterStageChunk)?;
            }
            output.sync_all()?;
            parent.sync()?;
            crate::import::validate::decode_png(&dest)?;
            let expected = stamp(&dest)?.ok_or(CoreError::InvalidPortraitSet)?;
            if j.staged_stamps.get(&dest) != Some(&expected) {
                return Err(journal::conflict(j, &dest));
            }
            j.complete_stage(dest.clone(), expected.clone());
            h.save(j, lib)?;
            h.hit(ExportBoundary::AfterStage)?;
            let key = PathBuf::from(stable_folder_name(lib.id(), *id)).join(name);
            j.history.file_digests.insert(key.clone(), expected.digest);
            staged.insert(key, dest);
        }
        job.report_status(
            (index + 1) as u64,
            Some(p.portrait_ids.len() as u64),
            "Staging portrait sets",
        );
    }
    if p.request.output == ExportOutput::Zip {
        let archive = j.staging.join("archive.zip");
        j.intent_stage(archive.clone());
        h.save(j, lib)?;
        h.hit(ExportBoundary::BeforeStage)?;
        check_cancel(job)?;
        let (parent, n) = checked_parent(&archive, &j.parents)?;
        let archive_file = parent.child(n, true)?;
        j.created_stage(archive.clone(), &archive_file)?;
        h.save(j, lib)?;
        let live = super::live_zip::LiveZip::new(archive_file)?;
        let mut writer =
            zip::ZipWriter::new(std::io::BufWriter::with_capacity(65536, live.clone()));
        // Keep the writer in this scope until errors have frozen its Drop writes.
        let writing: Result<()> = (|| {
            h.hit(ExportBoundary::AfterStageCreate)?;
            job.report_status(0, Some(staged.len() as u64), "Writing ZIP entries");
            for (entry_index, (key, path)) in staged.iter().enumerate() {
                check_cancel(job)?;
                let size = fs::metadata(path)?.len();
                writer
                    .start_file(
                        key.to_string_lossy().replace('\\', "/"),
                        zip::write::SimpleFileOptions::default()
                            .compression_method(zip::CompressionMethod::Stored)
                            .large_file(size >= u64::from(u32::MAX)),
                    )
                    .map_err(zip_error)?;
                if stamp(path)?.as_ref() != j.staged_stamps.get(path) {
                    return Err(journal::conflict(j, path));
                }
                let (directory, name) = checked_parent(path, &j.parents)?;
                let mut f = directory.child(name, false)?;
                let mut buf = [0; 65536];
                loop {
                    check_cancel(job)?;
                    let n = f.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    writer.write_all(&buf[..n])?;
                    writer.flush()?;
                    h.hit(ExportBoundary::AfterStageChunk)?;
                }
                job.report_status(
                    (entry_index + 1) as u64,
                    Some(staged.len() as u64),
                    "Writing ZIP entries",
                );
            }
            check_cancel(job)?;
            Ok(())
        })();
        if let Err(error) = writing {
            live.freeze();
            drop(writer);
            if !h.interrupted {
                if let Ok(proof) = live.verified_stamp() {
                    j.staged_stamps.insert(archive.clone(), proof);
                }
            }
            return Err(error);
        }
        writer.finish().map_err(zip_error)?.flush()?;
        live.sync_all()?;
        // Record live proof before later cancellation/error checks. This is not
        // persisted until the completed stage boundary below.
        let proof = live.verified_stamp()?;
        j.staged_stamps.insert(archive.clone(), proof.clone());
        for path in staged.values() {
            check_cancel(job)?;
            if stamp(path)?.as_ref() != j.staged_stamps.get(path) {
                return Err(journal::conflict(j, path));
            }
        }
        parent.sync()?;
        j.complete_stage(archive.clone(), proof);
        h.save(j, lib)?;
        h.hit(ExportBoundary::AfterStage)?;
        staged.clear();
        staged.insert(p.plan.target.clone(), archive);
    }
    // Revalidate the exact target fingerprint, excluding only this owned staging subtree.
    check_cancel(job)?;
    for path in staged.values() {
        check_cancel(job)?;
        if stamp(path)?.as_ref() != j.staged_stamps.get(path) {
            return Err(journal::conflict(j, path));
        }
    }
    job.report_status(0, None, "Checking the reviewed destination");
    revalidate(lib, p, &j.staging)?;
    for action in &p.plan.actions {
        check_cancel(job)?;
        if p.request.output == ExportOutput::Zip && !action.path.is_absolute() {
            continue;
        }
        match action.kind {
            Kind::Add => j.report.added += 1,
            Kind::Overwrite => j.report.overwritten += 1,
            Kind::Remove => j.report.removed += 1,
            Kind::Preserve => {
                j.report.preserved += 1;
                continue;
            }
        }
        let key = if p.request.output == ExportOutput::Zip {
            action.path.as_path()
        } else {
            action
                .path
                .strip_prefix(&p.plan.target)
                .map_err(|_| CoreError::ExportTargetUnsafe)?
        };
        let new = if action.kind == Kind::Remove {
            None
        } else {
            Some(
                staged
                    .get(key)
                    .ok_or(CoreError::InvalidPortraitSet)?
                    .clone(),
            )
        };
        let old = stamp(&action.path)?;
        if (action.kind == Kind::Add && old.is_some())
            || (action.kind != Kind::Add && old.is_none())
        {
            return Err(CoreError::ExportPlanStale);
        }
        j.moves.push(Move {
            target: action.path.clone(),
            backup: j.staging.join(format!("old-{}", j.moves.len())),
            old,
            new: new
                .as_ref()
                .map(|p| stamp(p).and_then(|s| s.ok_or(CoreError::InvalidPortraitSet)))
                .transpose()?,
            staged: new,
        });
    }
    j.record_moves();
    h.save(j, lib)?;
    let parents: BTreeSet<_> = j
        .moves
        .iter()
        .filter_map(|m| m.target.parent().map(Path::to_owned))
        .collect();
    let parent_count = parents.len() as u64;
    job.report_status(0, Some(parent_count), "Preparing destination folders");
    for (index, parent) in parents.into_iter().enumerate() {
        check_cancel(job)?;
        ensure_directory(lib, j, &parent, h, job)?;
        job.report_status(
            (index + 1) as u64,
            Some(parent_count),
            "Preparing destination folders",
        );
    }
    check_cancel(job)?;
    job.report_status(0, Some(j.moves.len() as u64), "Writing portrait files");
    for (index, m) in j.moves.clone().into_iter().enumerate() {
        check_cancel(job)?;
        if stamp(&m.target)? != m.old {
            return Err(CoreError::ExportPlanStale);
        }
        if m.old.is_some() {
            h.hit(ExportBoundary::BeforeBackup)?;
            check_cancel(job)?;
            if stamp(&m.target)? != m.old {
                return Err(CoreError::ExportPlanStale);
            }
            j.rename(&m.target, &m.backup)?;
            h.hit(ExportBoundary::AfterBackup)?;
            if stamp(&m.backup)? != m.old {
                let _ = j.rename(&m.backup, &m.target);
                return Err(journal::conflict(j, &m.target));
            }
        }
        if let Some(staged) = &m.staged {
            h.hit(ExportBoundary::BeforePromote)?;
            check_cancel(job)?;
            j.rename(staged, &m.target)?;
            h.hit(ExportBoundary::AfterPromote)?;
            if stamp(&m.target)? != m.new {
                return Err(journal::conflict(j, &m.target));
            }
        }
        job.report_status(
            (index + 1) as u64,
            Some(j.moves.len() as u64),
            "Writing portrait files",
        );
    }
    h.hit(ExportBoundary::BeforeCommit)?;
    job.report_status(0, Some(j.moves.len() as u64), "Verifying exported files");
    check_cancel(job)?;
    j.check_parents_with_job(Some(job))?;
    for (index, m) in j.moves.iter().enumerate() {
        check_cancel(job)?;
        if stamp(&m.target)? != m.new {
            return Err(journal::conflict(j, &m.target));
        }
        job.report_status(
            (index + 1) as u64,
            Some(j.moves.len() as u64),
            "Verifying exported files",
        );
    }
    check_cancel(job)?;
    // Intent state and receipt become durable together. Only this receipt may update machine-local history.
    let tx = lib.connection().unchecked_transaction()?;
    let mut committed = j.clone();
    committed.mark_committed();
    committed.save(&tx)?;
    let receipt = ExportReceipt {
        id: j.id,
        report: j.report.clone(),
        history: j.history.clone(),
        target_identity: journal::target_identity(&j.target)?,
    };
    tx.execute(
        "INSERT INTO operation_state(operation_id,kind,state_json) VALUES(?,'export_receipt',?)",
        rusqlite::params![
            format!("export-receipt-{}", j.id),
            serde_json::to_string(&receipt)?
        ],
    )?;
    tx.commit()?;
    j.committed = true;
    h.hit(ExportBoundary::AfterCommit)?;
    job.report_status(0, None, "Finishing export cleanup");
    h.hit(ExportBoundary::BeforeCleanup)?;
    if let Err(e) = journal::recover_one(lib.connection(), j) {
        j.report.issues.push(Issue {
            path: j.staging.to_string_lossy().into_owned(),
            code: "EXPORT_CLEANUP_PENDING".into(),
            message: e.to_string(),
            severity: IssueSeverity::Warning,
        });
    }
    h.hit(ExportBoundary::AfterCleanup)?;
    Ok(())
}
fn ensure_directory(
    lib: &Library,
    j: &mut Journal,
    path: &Path,
    h: &mut Hooks<'_>,
    job: &JobContext,
) -> Result<()> {
    check_cancel(job)?;
    if j.parents.contains_key(path) {
        return Ok(());
    }
    if path.exists() {
        j.track_parent(path)?;
        h.save(j, lib)?;
        return Ok(());
    }
    ensure_directory(
        lib,
        j,
        path.parent().ok_or(CoreError::ExportTargetUnsafe)?,
        h,
        job,
    )?;
    j.created_directory(path.into());
    h.save(j, lib)?;
    check_cancel(job)?;
    h.hit(ExportBoundary::BeforeCreateDirectory)?;
    mkdir(path, &j.parents)?;
    h.hit(ExportBoundary::AfterCreateDirectory)?;
    j.track_parent(path)?;
    h.save(j, lib)
}
fn check_cancel(job: &JobContext) -> Result<()> {
    if job.is_cancelled() {
        Err(CoreError::Cancelled)
    } else {
        Ok(())
    }
}
fn revalidate(lib: &Library, p: &StoredExportPlan, staging: &Path) -> Result<()> {
    let matches = (|| -> Result<bool> {
        Ok(
            plan::safe_target(lib, &p.request.target, p.request.output)? == p.plan.target
                && plan::revision(lib)? == p.catalog_revision
                && plan::destination_fingerprint_excluding(&p.plan.target, Some(staging))?
                    == p.destination_fingerprint,
        )
    })();
    if matches!(matches, Ok(true)) {
        Ok(())
    } else {
        Err(CoreError::ExportPlanStale)
    }
}
fn zip_error(e: zip::result::ZipError) -> CoreError {
    CoreError::Io(std::io::Error::other(e))
}

fn destination_lock(target: &Path) -> Result<fs::File> {
    let digest = format!(
        "{:x}",
        Sha256::digest(target.as_os_str().as_encoded_bytes())
    );
    let lock_path = std::env::temp_dir().join(format!(".portrait-export-lock-{digest}"));
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .mode(0o600);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000);
    }
    let file = options.open(&lock_path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(CoreError::ExportTargetUnsafe);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 || metadata.uid() != unsafe { libc::geteuid() } {
            return Err(CoreError::ExportTargetUnsafe);
        }
    }
    fs2::FileExt::try_lock_exclusive(&file).map_err(|_| CoreError::ExportDestinationBusy)?;
    Ok(file)
}
