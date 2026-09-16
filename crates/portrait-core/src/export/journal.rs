//! Durable intent lives in the library database; all staged bytes live on the destination filesystem.
use super::{ExportHistory, plan::identity};
use crate::{CoreError, Library, Result, types::ExportReport};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct Stamp {
    pub identity: Vec<u8>,
    pub digest: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Move {
    pub target: PathBuf,
    pub staged: Option<PathBuf>,
    pub backup: PathBuf,
    pub old: Option<Stamp>,
    pub new: Option<Stamp>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Journal {
    #[serde(default = "legacy_version")]
    pub version: u8,
    #[serde(default)]
    pub staged_creation: BTreeMap<PathBuf, Vec<u8>>,
    #[serde(skip)]
    pub pending: Vec<JournalEvent>,
    #[serde(skip)]
    pub sequence: u64,
    #[serde(skip)]
    pub persisted: bool,
    pub id: Uuid,
    pub output: crate::types::ExportOutput,
    pub target: PathBuf,
    pub staging: PathBuf,
    pub committed: bool,
    pub parents: BTreeMap<PathBuf, Vec<u8>>,
    pub created: Vec<PathBuf>,
    pub moves: Vec<Move>,
    pub staged_files: Vec<PathBuf>,
    pub staged_stamps: BTreeMap<PathBuf, Stamp>,
    pub report: ExportReport,
    pub history: ExportHistory,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReceipt {
    pub id: Uuid,
    pub report: ExportReport,
    pub history: ExportHistory,
    pub target_identity: String,
}

/// Stable object identity is separate from content provenance. Missing targets are bound to their nearest existing ancestor.
pub fn target_identity(path: &Path) -> Result<String> {
    let mut p = path;
    loop {
        match fs::symlink_metadata(p) {
            Ok(m) => return Ok(format!("{}:{:?}", p.display(), identity(&m))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                p = p.parent().ok_or(CoreError::ExportTargetUnsafe)?
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Holds an opened, non-link directory. Unix child operations are relative to its descriptor.
pub(super) struct Directory {
    pub file: File,
    #[cfg(not(unix))]
    path: PathBuf,
}
impl Directory {
    pub fn open(path: &Path) -> Result<Self> {
        if !path.is_absolute() || path.components().any(|p| matches!(p, Component::ParentDir)) {
            return Err(CoreError::ExportTargetUnsafe);
        }
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};
            let mut file = File::open("/")?;
            for part in path.components() {
                if let Component::Normal(part) = part {
                    let name = cname(part)?;
                    let fd = unsafe {
                        libc::openat(
                            file.as_raw_fd(),
                            name.as_ptr(),
                            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                        )
                    };
                    if fd < 0 {
                        return Err(std::io::Error::last_os_error().into());
                    }
                    file = unsafe { File::from_raw_fd(fd) };
                }
            }
            Ok(Self { file })
        }
        #[cfg(not(unix))]
        {
            for p in path.ancestors() {
                let m = fs::symlink_metadata(p)?;
                if !m.is_dir() || m.file_type().is_symlink() {
                    return Err(CoreError::ExportTargetUnsafe);
                }
            }
            Ok(Self {
                file: OpenOptions::new()
                    .read(true)
                    .custom_flags(0x02000000)
                    .open(path)?,
                path: path.into(),
            })
        }
    }
    pub fn identity(&self) -> Result<Vec<u8>> {
        Ok(identity(&self.file.metadata()?))
    }
    pub fn sync(&self) -> Result<()> {
        #[cfg(unix)]
        self.file.sync_all()?;
        Ok(())
    }
    pub fn child(&self, name: &std::ffi::OsStr, create: bool) -> Result<File> {
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};
            let name = cname(name)?;
            let flags = if create {
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL
            } else {
                libc::O_RDONLY | libc::O_NONBLOCK
            };
            let fd = unsafe {
                libc::openat(
                    self.file.as_raw_fd(),
                    name.as_ptr(),
                    flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    0o600,
                )
            };
            if fd < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(unsafe { File::from_raw_fd(fd) })
        }
        #[cfg(not(unix))]
        {
            let path = self.path.join(name);
            if !create {
                let m = fs::symlink_metadata(&path)?;
                if m.file_type().is_symlink() {
                    return Err(CoreError::ExportTargetUnsafe);
                }
            }
            Ok(OpenOptions::new()
                .read(true)
                .write(create)
                .create_new(create)
                .open(path)?)
        }
    }
}
#[cfg(windows)]
use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};
#[cfg(unix)]
fn cname(name: &std::ffi::OsStr) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(name.as_bytes()).map_err(|_| CoreError::ExportTargetUnsafe)
}
pub(super) fn split(path: &Path) -> Result<(Directory, &std::ffi::OsStr)> {
    Ok((
        Directory::open(path.parent().ok_or(CoreError::ExportTargetUnsafe)?)?,
        path.file_name().ok_or(CoreError::ExportTargetUnsafe)?,
    ))
}
pub(super) fn stamp(path: &Path) -> Result<Option<Stamp>> {
    let (parent, name) = match split(path) {
        Ok(v) => v,
        Err(CoreError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut file = match parent.child(name, false) {
        Ok(f) => f,
        Err(CoreError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let m = file.metadata()?;
    if !m.is_file() {
        return Err(CoreError::ExportTargetUnsafe);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if m.nlink() != 1 {
            return Err(CoreError::ExportTargetUnsafe);
        }
    }
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(Some(Stamp {
        identity: identity(&m),
        digest: format!("{:x}", hash.finalize()),
    }))
}
pub(super) fn mkdir(path: &Path, parents: &BTreeMap<PathBuf, Vec<u8>>) -> Result<()> {
    let (p, n) = checked_parent(path, parents)?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let n = cname(n)?;
        if unsafe { libc::mkdirat(p.file.as_raw_fd(), n.as_ptr(), 0o700) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(not(unix))]
    fs::create_dir(p.path.join(n))?;
    p.sync()
}
/// Atomic no-clobber promotion. Linux/macOS operate on pinned parent handles.
fn rename_new(from: &Path, to: &Path, parents: &BTreeMap<PathBuf, Vec<u8>>) -> Result<()> {
    let (a, an) = checked_parent(from, parents)?;
    let (b, bn) = checked_parent(to, parents)?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let an = cname(an)?;
        let bn = cname(bn)?;
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::renameat2(
                a.file.as_raw_fd(),
                an.as_ptr(),
                b.file.as_raw_fd(),
                bn.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(target_os = "macos")]
        let result = unsafe {
            libc::renameatx_np(
                a.file.as_raw_fd(),
                an.as_ptr(),
                b.file.as_raw_fd(),
                bn.as_ptr(),
                libc::RENAME_EXCL,
            )
        };
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let result = -1;
        if result != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
        }
        let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 8) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let _ = (an, bn);
    }
    a.sync()?;
    b.sync()
}
fn unlink(path: &Path, directory: bool, parents: &BTreeMap<PathBuf, Vec<u8>>) -> Result<()> {
    let (p, n) = checked_parent(path, parents)?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let n = cname(n)?;
        if unsafe {
            libc::unlinkat(
                p.file.as_raw_fd(),
                n.as_ptr(),
                if directory { libc::AT_REMOVEDIR } else { 0 },
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(not(unix))]
    {
        if directory {
            fs::remove_dir(p.path.join(n))?
        } else {
            fs::remove_file(p.path.join(n))?
        }
    }
    p.sync()
}
pub(super) fn checked_parent<'a>(
    path: &'a Path,
    parents: &BTreeMap<PathBuf, Vec<u8>>,
) -> Result<(Directory, &'a std::ffi::OsStr)> {
    let (directory, name) = split(path)?;
    if parents.get(path.parent().ok_or(CoreError::ExportTargetUnsafe)?)
        != Some(&directory.identity()?)
    {
        return Err(CoreError::ExportPlanStale);
    }
    Ok((directory, name))
}
impl Journal {
    pub fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        rename_new(from, to, &self.parents)
    }
    pub fn unlink(&self, path: &Path, directory: bool) -> Result<()> {
        unlink(path, directory, &self.parents)
    }

    pub fn validate(&self) -> Result<()> {
        let absolute = |p: &Path| {
            p.is_absolute()
                && p.components().all(|c| {
                    matches!(
                        c,
                        Component::RootDir | Component::Prefix(_) | Component::Normal(_)
                    )
                })
        };
        let fail = || {
            CoreError::Recovery(format!(
                "Invalid export journal {}. Retain backup {}",
                self.id,
                self.staging.display()
            ))
        };
        if !absolute(&self.target)
            || !absolute(&self.staging)
            || self.staging.file_name().and_then(|n| n.to_str())
                != Some(&format!(".portrait-export-{}", self.id))
            || self
                .staging
                .parent()
                .is_none_or(|p| !self.target.starts_with(p))
        {
            return Err(fail());
        }
        let staged: BTreeSet<_> = self.staged_files.iter().map(PathBuf::as_path).collect();
        if staged.len() != self.staged_files.len() {
            return Err(fail());
        }
        let move_parents: BTreeSet<_> = self
            .moves
            .iter()
            .filter_map(|m| m.target.parent())
            .collect();
        let allowed_parents: BTreeSet<_> = move_parents
            .iter()
            .flat_map(|p| p.ancestors())
            .chain(self.staging.ancestors())
            .chain(self.target.ancestors())
            .collect();
        for (index, m) in self.moves.iter().enumerate() {
            if !absolute(&m.target)
                || !(m.target == self.target
                    || m.target
                        .strip_prefix(&self.target)
                        .is_ok_and(|p| p.components().count() == 2))
                || m.backup != self.staging.join(format!("old-{index}"))
                || m.staged
                    .as_ref()
                    .is_some_and(|p| !staged.contains(p.as_path()))
                || m.new.is_some() != m.staged.is_some()
            {
                return Err(fail());
            }
        }
        if self
            .staged_stamps
            .keys()
            .any(|p| !staged.contains(p.as_path()))
        {
            return Err(fail());
        }
        for p in &self.staged_files {
            let name = p.file_name().and_then(|p| p.to_str()).unwrap_or("");
            if p.parent() != Some(self.staging.as_path())
                || !(name == "archive.zip"
                    || name
                        .strip_prefix("asset-")
                        .is_some_and(|n| n.parse::<usize>().is_ok()))
            {
                return Err(fail());
            }
        }
        for p in self.staged_creation.keys() {
            if !staged.contains(p.as_path()) {
                return Err(fail());
            }
        }
        if self.version == 2
            && self
                .staged_stamps
                .iter()
                .any(|(p, s)| self.staged_creation.get(p) != Some(&s.identity))
        {
            return Err(fail());
        }
        for p in self.parents.keys() {
            if !absolute(p) || !allowed_parents.contains(p.as_path()) {
                return Err(fail());
            }
        }
        for p in &self.created {
            if !absolute(p) || !(self.target.starts_with(p) || move_parents.contains(p.as_path())) {
                return Err(fail());
            }
        }
        Ok(())
    }

    pub fn save(&mut self, c: &rusqlite::Connection) -> Result<()> {
        if !self.persisted {
            c.execute(
                "INSERT INTO operation_state(operation_id,kind,state_json) VALUES(?1,'export',?2)",
                rusqlite::params![self.id.to_string(), serde_json::to_string(self)?],
            )?;
            self.persisted = true;
        } else if !self.pending.is_empty() {
            let sequence = self.sequence + 1;
            let delta = Delta {
                operation: self.id,
                sequence,
                events: self.pending.clone(),
            };
            c.execute("INSERT INTO operation_state(operation_id,kind,state_json) VALUES(?1,'export_delta',?2)",rusqlite::params![format!("export-delta-{}-{sequence:010}",self.id),serde_json::to_string(&delta)?])?;
            self.sequence = sequence;
        }
        self.pending.clear();
        Ok(())
    }
    pub fn intent_stage(&mut self, path: PathBuf) {
        self.staged_files.push(path.clone());
        self.pending.push(JournalEvent::StageIntent { path });
    }
    pub fn created_stage(&mut self, path: PathBuf, file: &File) -> Result<()> {
        let id = identity(&file.metadata()?);
        self.staged_creation.insert(path.clone(), id.clone());
        self.pending
            .push(JournalEvent::StageCreated { path, identity: id });
        Ok(())
    }
    pub fn complete_stage(&mut self, path: PathBuf, stamp: Stamp) {
        self.staged_stamps.insert(path.clone(), stamp.clone());
        self.pending
            .push(JournalEvent::StageComplete { path, stamp });
    }
    pub fn created_directory(&mut self, path: PathBuf) {
        self.created.push(path.clone());
        self.pending.push(JournalEvent::CreatedDirectory { path });
    }
    pub fn record_moves(&mut self) {
        self.pending.push(JournalEvent::Moves {
            moves: self.moves.clone(),
        });
    }
    pub fn mark_committed(&mut self) {
        self.committed = true;
        self.pending.push(JournalEvent::Committed);
    }
    fn removable_directories(&self) -> BTreeSet<&Path> {
        if !self.committed || self.output != crate::types::ExportOutput::Directory {
            return BTreeSet::new();
        }
        self.moves
            .iter()
            .filter(|m| m.new.is_none())
            .filter_map(|m| m.target.parent())
            .filter(|p| *p != self.target)
            .collect()
    }
    pub fn check_parents(&self) -> Result<()> {
        self.check_parents_with_job(None)
    }
    pub fn check_parents_with_job(&self, job: Option<&crate::import::JobContext>) -> Result<()> {
        let created: BTreeSet<_> = self.created.iter().map(PathBuf::as_path).collect();
        let removable = self.removable_directories();
        for (p, id) in &self.parents {
            if job.is_some_and(|j| j.is_cancelled()) {
                return Err(CoreError::Cancelled);
            }
            let missing =
                fs::symlink_metadata(p).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
            if missing
                && (p == &self.staging
                    || created.contains(p.as_path())
                    || removable.contains(p.as_path()))
            {
                continue;
            }
            if Directory::open(p).and_then(|d| d.identity()).ok().as_ref() != Some(id) {
                return Err(CoreError::Recovery(format!(
                    "Directory changed or disconnected: {}. Retain export backup {}",
                    p.display(),
                    self.staging.display()
                )));
            }
        }
        Ok(())
    }
    pub fn track_parent(&mut self, p: &Path) -> Result<()> {
        for p in p.ancestors() {
            if let std::collections::btree_map::Entry::Vacant(entry) = self.parents.entry(p.into())
            {
                let id = Directory::open(p)?.identity()?;
                entry.insert(id.clone());
                self.pending.push(JournalEvent::Parent {
                    path: p.into(),
                    identity: id,
                });
            }
        }
        Ok(())
    }
}
fn legacy_version() -> u8 {
    1
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(super) enum JournalEvent {
    Parent { path: PathBuf, identity: Vec<u8> },
    CreatedDirectory { path: PathBuf },
    StageIntent { path: PathBuf },
    StageCreated { path: PathBuf, identity: Vec<u8> },
    StageComplete { path: PathBuf, stamp: Stamp },
    Moves { moves: Vec<Move> },
    Committed,
}
#[derive(Serialize, Deserialize)]
struct Delta {
    operation: Uuid,
    sequence: u64,
    events: Vec<JournalEvent>,
}
impl Journal {
    fn replay(&mut self, event: JournalEvent) {
        match event {
            JournalEvent::Parent { path, identity } => {
                self.parents.insert(path, identity);
            }
            JournalEvent::CreatedDirectory { path } => self.created.push(path),
            JournalEvent::StageIntent { path } => self.staged_files.push(path),
            JournalEvent::StageCreated { path, identity } => {
                self.staged_creation.insert(path, identity);
            }
            JournalEvent::StageComplete { path, stamp } => {
                self.staged_stamps.insert(path, stamp);
            }
            JournalEvent::Moves { moves } => self.moves = moves,
            JournalEvent::Committed => self.committed = true,
        }
    }
}

pub(super) fn conflict(j: &Journal, path: &Path) -> CoreError {
    CoreError::Recovery(format!(
        "External content changed at {}. Recovery retained {} and journal {}. Move the conflicting file aside and reopen the library to retry.",
        path.display(),
        j.staging.display(),
        j.id
    ))
}

pub(super) fn recover_one(c: &rusqlite::Connection, j: &Journal) -> Result<()> {
    recover_one_observed(c, j, &mut |_| Ok(()))
}
fn recover_one_observed(
    c: &rusqlite::Connection,
    j: &Journal,
    observer: &mut dyn FnMut(super::ExportBoundary) -> Result<()>,
) -> Result<()> {
    use super::ExportBoundary as Boundary;
    j.validate()?;
    j.check_parents()?;
    if !j.committed {
        for m in j.moves.iter().rev() {
            let quarantine = j.staging.join(format!(
                "undo-{}",
                m.backup.file_name().unwrap().to_string_lossy()
            ));
            if let Some(pending) = stamp(&quarantine)? {
                if Some(pending) != m.new {
                    return Err(conflict(j, &quarantine));
                }
                observer(Boundary::BeforeRecoveryUnlink)?;
                j.unlink(&quarantine, false)?;
                observer(Boundary::AfterRecoveryUnlink)?;
            }
            let backup = stamp(&m.backup)?;
            let current = stamp(&m.target)?;
            if m.old.is_some() && backup.is_none() && current != m.old {
                return Err(conflict(j, &m.target));
            }
            if current.is_some() && current == m.new {
                let quarantine = j.staging.join(format!(
                    "undo-{}",
                    m.backup.file_name().unwrap().to_string_lossy()
                ));
                observer(Boundary::BeforeRecoveryRename)?;
                j.rename(&m.target, &quarantine)?;
                observer(Boundary::AfterRecoveryRename)?;
                if stamp(&quarantine)? != m.new {
                    let _ = j.rename(&quarantine, &m.target);
                    return Err(conflict(j, &m.target));
                }
                observer(Boundary::BeforeRecoveryUnlink)?;
                j.unlink(&quarantine, false)?;
                observer(Boundary::AfterRecoveryUnlink)?;
            } else if current.is_some() && (backup.is_some() || current != m.old) {
                return Err(conflict(j, &m.target));
            }
            if backup.is_some() {
                if backup != m.old {
                    return Err(conflict(j, &m.backup));
                }
                observer(Boundary::BeforeRecoveryRename)?;
                j.rename(&m.backup, &m.target)
                    .map_err(|_| conflict(j, &m.target))?;
                observer(Boundary::AfterRecoveryRename)?;
            }
        }
    }
    // Delete only enumerated files, never recursively clear a staging folder.
    for m in &j.moves {
        if let Some(s) = stamp(&m.backup)? {
            if Some(s) != m.old {
                return Err(conflict(j, &m.backup));
            }
            observer(Boundary::BeforeRecoveryUnlink)?;
            j.unlink(&m.backup, false)?;
            observer(Boundary::AfterRecoveryUnlink)?;
        }
    }
    for p in &j.staged_files {
        if let Some(current) = stamp(p)? {
            if j.staged_stamps.get(p) != Some(&current) {
                return Err(conflict(j, p));
            }
            observer(Boundary::BeforeRecoveryUnlink)?;
            j.unlink(p, false)?;
            observer(Boundary::AfterRecoveryUnlink)?;
        }
    }
    // A name/parent identity alone never establishes ownership of an unstamped file.
    observer(Boundary::BeforeRecoveryUnlink)?;
    match j.unlink(&j.staging, true) {
        Ok(()) => {}
        Err(CoreError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    observer(Boundary::AfterRecoveryUnlink)?;
    if !j.committed {
        for p in j.created.iter().rev() {
            if let Some(id) = j.parents.get(p)
                && Directory::open(p).and_then(|d| d.identity()).ok().as_ref() == Some(id)
            {
                observer(Boundary::BeforeRecoveryUnlink)?;
                let _ = j.unlink(p, true);
                observer(Boundary::AfterRecoveryUnlink)?;
            }
        }
    }
    for parent in j.removable_directories() {
        observer(Boundary::BeforeRecoveryUnlink)?;
        let _ = j.unlink(parent, true);
        observer(Boundary::AfterRecoveryUnlink)?;
    }
    observer(Boundary::BeforeRecoveryRetire)?;
    let transaction = c.unchecked_transaction()?;
    transaction.execute(
        "DELETE FROM operation_state WHERE kind='export_delta' AND operation_id LIKE ?",
        [format!("export-delta-{}-%", j.id)],
    )?;
    transaction.execute(
        "DELETE FROM operation_state WHERE operation_id=? AND kind='export'",
        [j.id.to_string()],
    )?;
    transaction.commit()?;
    observer(Boundary::AfterRecoveryRetire)?;
    Ok(())
}
pub(crate) fn recover_exports(root: &Path, c: &rusqlite::Connection) -> Result<()> {
    recover_export_operations_with_observer(root, c, &mut |_| Ok(()))
}
/// Recovery interruption harness. Trusted Rust only; never exposed over IPC.
pub fn recover_export_operations_with_observer(
    root: &Path,
    c: &rusqlite::Connection,
    observer: &mut dyn FnMut(super::ExportBoundary) -> Result<()>,
) -> Result<()> {
    let mut q = c.prepare("SELECT state_json FROM operation_state WHERE kind='export'")?;
    let rows = q
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(q);
    for row in rows {
        let mut j: Journal = serde_json::from_str(&row)
            .map_err(|e| CoreError::Recovery(format!("Malformed export journal: {e}")))?;
        if j.version == 2 {
            let mut query=c.prepare("SELECT state_json FROM operation_state WHERE kind='export_delta' AND operation_id LIKE ? ORDER BY operation_id")?;
            let rows = query.query_map([format!("export-delta-{}-%", j.id)], |r| {
                r.get::<_, String>(0)
            })?;
            for row in rows {
                let delta: Delta = serde_json::from_str(&row?)
                    .map_err(|e| CoreError::Recovery(format!("Malformed export delta: {e}")))?;
                if delta.operation != j.id || delta.sequence != j.sequence + 1 {
                    return Err(conflict(&j, &j.staging));
                }
                j.sequence = delta.sequence;
                for event in delta.events {
                    j.replay(event);
                }
            }
        } else if j.version != 1 {
            return Err(CoreError::Recovery(format!(
                "Unsupported export journal version {}",
                j.version
            )));
        }
        j.persisted = true;
        let output = j.output;
        if super::plan::safe_target_root(root, &j.target, output)
            .ok()
            .as_ref()
            != Some(&j.target)
        {
            return Err(conflict(&j, &j.target));
        }
        recover_one_observed(c, &j, observer)?;
    }
    Ok(())
}
pub fn pending_export_receipts(lib: &Library) -> Result<Vec<ExportReceipt>> {
    let mut q = lib
        .connection()
        .prepare("SELECT state_json FROM operation_state WHERE kind='export_receipt'")?;
    q.query_map([], |r| r.get::<_, String>(0))?
        .map(|s| Ok(serde_json::from_str(&s?)?))
        .collect()
}
pub fn acknowledge_export_receipt(lib: &Library, id: Uuid) -> Result<()> {
    lib.connection().execute(
        "DELETE FROM operation_state WHERE operation_id=? AND kind='export_receipt'",
        [format!("export-receipt-{id}")],
    )?;
    Ok(())
}
