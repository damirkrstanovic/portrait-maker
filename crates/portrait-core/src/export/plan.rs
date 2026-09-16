use crate::{
    CoreError, Library, Result,
    types::{
        ExportAction, ExportActionKind as Kind, ExportMode, ExportOutput, ExportPlan,
        ExportRequest, ExportScope,
    },
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

const FILES: [&str; 3] = ["Small.png", "Medium.png", "Fulllength.png"];
const MAX_ENTRIES: usize = 100_000;
const MAX_PLANS: usize = 8;

/// Machine-local overwrite provenance only; never used for library deduplication.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportHistory {
    pub library_id: Option<Uuid>,
    pub destination_id: Option<Uuid>,
    pub destination: Option<PathBuf>,
    pub file_digests: BTreeMap<PathBuf, String>,
}
#[derive(Debug, Clone)]
pub struct StoredExportPlan {
    pub plan: ExportPlan,
    pub request: ExportRequest,
    pub portrait_ids: Vec<Uuid>,
    pub catalog_revision: u64,
    pub destination_fingerprint: String,
    pub destination_id: Option<Uuid>,
}
#[derive(Default)]
pub(crate) struct PlanRegistry {
    plans: HashMap<Uuid, StoredExportPlan>,
    designated: HashMap<PathBuf, String>,
}

impl Library {
    /// Explicit UI designation, held only for this open library and exact directory state.
    pub fn designate_export_target(&self, path: &Path) -> Result<PathBuf> {
        let target = safe_target(self, path, ExportOutput::Directory)?;
        let fingerprint = destination_fingerprint(&target)?;
        let mut registry = self.export_registry.borrow_mut();
        if registry.designated.len() >= 32 {
            registry.designated.clear();
        }
        registry.designated.insert(target.clone(), fingerprint);
        Ok(target)
    }
    pub fn export_plan(&self, id: Uuid) -> Result<StoredExportPlan> {
        self.export_registry
            .borrow()
            .plans
            .get(&id)
            .cloned()
            .ok_or(CoreError::ExportPlanNotFound)
    }
    pub fn discard_export_plan(&self, id: Uuid) {
        self.export_registry.borrow_mut().plans.remove(&id);
    }
    /// Read-only revalidation. A stale preview is retired, never silently refreshed.
    pub fn validate_export_plan(&self, id: Uuid) -> Result<ExportPlan> {
        let stored = self.export_plan(id)?;
        let valid = (|| -> Result<bool> {
            let target = safe_target(self, &stored.request.target, stored.request.output)?;
            Ok(target == stored.plan.target
                && revision(self)? == stored.catalog_revision
                && destination_fingerprint(&target)? == stored.destination_fingerprint)
        })();
        if !matches!(valid, Ok(true)) {
            self.discard_export_plan(id);
            return Err(CoreError::ExportPlanStale);
        }
        Ok(stored.plan)
    }
    /// Task 11 apply handoff: call under the library writer lock immediately before staging.
    /// Confirmation cannot override staleness. Successful consumption is one-use.
    pub fn consume_export_plan(&self, id: Uuid, confirmed: bool) -> Result<StoredExportPlan> {
        let plan = self.validate_export_plan(id)?;
        if plan.requires_confirmation && !confirmed {
            return Err(CoreError::ExportConfirmationRequired);
        }
        self.export_registry
            .borrow_mut()
            .plans
            .remove(&id)
            .ok_or(CoreError::ExportPlanNotFound)
    }
}

pub fn stable_folder_name(library_id: Uuid, portrait_id: Uuid) -> String {
    format!("pm-{library_id}-{portrait_id}")
}

/// Conservative entrypoint when the desktop has not resolved a routing identity.
/// A manifest cannot establish its own current destination identity.
pub fn plan_export(
    lib: &Library,
    request: ExportRequest,
    history: &ExportHistory,
) -> Result<ExportPlan> {
    build_plan(lib, request, None, history)
}

/// The desktop resolves this ID independently from saved destination settings or
/// its persistent arbitrary-target registry before loading the matching manifest.
pub fn plan_export_for_destination(
    lib: &Library,
    request: ExportRequest,
    destination_id: Uuid,
    history: &ExportHistory,
) -> Result<ExportPlan> {
    build_plan(lib, request, Some(destination_id), history)
}

fn build_plan(
    lib: &Library,
    request: ExportRequest,
    destination_id: Option<Uuid>,
    history: &ExportHistory,
) -> Result<ExportPlan> {
    let mut statement = lib.connection().prepare("SELECT p.id FROM portraits p WHERE p.trashed_at IS NULL AND (?1=0 OR EXISTS(SELECT 1 FROM selection s WHERE s.portrait_id=p.id)) ORDER BY p.id")?;
    let ids = statement
        .query_map([i64::from(request.scope == ExportScope::Selected)], |row| {
            row.get::<_, String>(0)
        })?
        .map(|r| {
            let raw = r?;
            Uuid::parse_str(&raw).map_err(|_| CoreError::PortraitNotFound)
        })
        .collect::<Result<Vec<_>>>()?;
    if request.scope == ExportScope::Selected && ids.is_empty() {
        return Err(CoreError::EmptySelection);
    }
    let target = safe_target(lib, &request.target, request.output)?;
    let before = destination_fingerprint(&target)?;
    if request.output == ExportOutput::Directory
        && request.mode == ExportMode::Replace
        && lib.export_registry.borrow().designated.get(&target) != Some(&before)
    {
        return Err(CoreError::ExportTargetNotDesignated);
    }
    let mut actions = Vec::new();
    let mut warnings = Vec::new();
    let mut confirmation = request.mode == ExportMode::Replace;
    let desired: BTreeSet<_> = ids
        .iter()
        .map(|id| stable_folder_name(lib.id(), *id))
        .collect();
    if request.output == ExportOutput::Zip {
        let exists = target.exists();
        confirmation |= exists;
        actions.push(action(
            if exists { Kind::Overwrite } else { Kind::Add },
            target.clone(),
            "Game-ready ZIP archive",
        ));
        for folder in &desired {
            for name in FILES {
                actions.push(action(
                    Kind::Add,
                    PathBuf::from(folder).join(name),
                    "Archive entry",
                ));
            }
        }
    } else {
        if target.is_dir() {
            for entry in sorted_entries(&target)? {
                let name = entry.file_name().to_string_lossy().into_owned();
                if desired.contains(&name.to_ascii_lowercase()) && !desired.contains(&name) {
                    return Err(CoreError::ExportTargetUnsafe);
                }
            }
        }
        for folder in &desired {
            let directory = target.join(folder);
            if let Ok(meta) = fs::symlink_metadata(&directory) {
                if !meta.is_dir() || meta.file_type().is_symlink() {
                    return Err(CoreError::ExportTargetUnsafe);
                }
            }
            if directory.is_dir() {
                for entry in sorted_entries(&directory)? {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if FILES
                        .iter()
                        .any(|expected| expected.eq_ignore_ascii_case(&name))
                        && !FILES.contains(&name.as_str())
                    {
                        return Err(CoreError::ExportTargetUnsafe);
                    }
                }
            }
            for name in FILES {
                let path = directory.join(name);
                let mut reason = "Missing portrait image";
                let kind = match fs::symlink_metadata(&path) {
                    Ok(meta) => {
                        if !meta.is_file()
                            || meta.file_type().is_symlink()
                            || has_multiple_links(&meta)
                        {
                            return Err(CoreError::ExportTargetUnsafe);
                        }
                        let owned = destination_id.is_some()
                            && history.destination_id == destination_id
                            && history.library_id == Some(lib.id())
                            && history.destination.as_ref() == Some(&target)
                            && history.file_digests.get(&PathBuf::from(folder).join(name))
                                == Some(&file_digest(&path)?);
                        // Provenance affects explanation only. Every overwrite confirms.
                        confirmation = true;
                        reason = if owned {
                            "Previously exported portrait image; overwrite requires confirmation"
                        } else {
                            "Unknown or locally modified portrait image"
                        };
                        if !owned {
                            warnings.push(format!(
                                "Unknown or locally modified file: {}",
                                path.display()
                            ));
                        }
                        Kind::Overwrite
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Kind::Add,
                    Err(e) => return Err(e.into()),
                };
                actions.push(action(kind, path, reason));
            }
        }
        if target.is_dir() {
            for entry in sorted_entries(&target)? {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                let meta = fs::symlink_metadata(&path)?;
                if desired.contains(&name) {
                    for extra in sorted_entries(&path)? {
                        if !FILES.iter().any(|name| extra.file_name() == *name) {
                            preserve_tree(&extra.path(), &mut actions, 0)?;
                            warnings.push(format!("Extra content preserved in {}", path.display()));
                        }
                    }
                } else if request.mode == ExportMode::Replace
                    && meta.is_dir()
                    && !meta.file_type().is_symlink()
                    && recognized_set(&path)?
                {
                    for image in sorted_entries(&path)? {
                        actions.push(action(
                            Kind::Remove,
                            image.path(),
                            "Portrait folder outside the frozen export set",
                        ));
                    }
                } else {
                    preserve_tree(&path, &mut actions, 0)?;
                    if request.mode == ExportMode::Replace && meta.is_dir() {
                        warnings.push(format!("Preserved {}: contains extra content or is not a verified complete portrait set", path.display()));
                    }
                }
            }
        }
    }
    let nonstandard: i64 = lib.connection().query_row("SELECT count(DISTINCT a.portrait_id) FROM assets a JOIN portraits p ON p.id=a.portrait_id WHERE p.trashed_at IS NULL AND (?1=0 OR EXISTS(SELECT 1 FROM selection s WHERE s.portrait_id=p.id)) AND ((a.role='small' AND (a.width<>185 OR a.height<>242)) OR (a.role='medium' AND (a.width<>330 OR a.height<>432)) OR (a.role='large' AND (a.width<>692 OR a.height<>1024)))", [i64::from(request.scope == ExportScope::Selected)], |r|r.get(0))?;
    if nonstandard > 0 {
        warnings.push(format!("{nonstandard} portraits have nonstandard image dimensions. Export keeps their original dimensions."));
        confirmation = true;
    }
    if request.mode == ExportMode::Replace {
        warnings.push("Removing existing portrait folders may affect saved characters. Saved-game references are not migrated.".into());
    }
    if destination_fingerprint(&target)? != before {
        return Err(CoreError::ExportPlanStale);
    }
    actions.sort_by(|a, b| a.path.cmp(&b.path));
    warnings.sort();
    warnings.dedup();
    let plan = ExportPlan {
        id: Uuid::new_v4(),
        target,
        portrait_count: ids.len() as u64,
        actions,
        requires_confirmation: confirmation,
        warnings,
    };
    let stored = StoredExportPlan {
        plan: plan.clone(),
        request,
        portrait_ids: ids,
        catalog_revision: revision(lib)?,
        destination_fingerprint: before,
        destination_id,
    };
    let mut registry = lib.export_registry.borrow_mut();
    // Bounded ephemeral previews; opening a new preview can retire old windows' plans.
    if registry.plans.len() >= MAX_PLANS {
        registry.plans.clear();
    }
    registry.plans.insert(plan.id, stored);
    Ok(plan)
}
fn action(kind: Kind, path: PathBuf, reason: &str) -> ExportAction {
    ExportAction {
        kind,
        path,
        reason: reason.into(),
    }
}
pub(super) fn revision(lib: &Library) -> Result<u64> {
    Ok(lib.connection().query_row("SELECT json_extract(state_json,'$.revision') FROM operation_state WHERE operation_id='catalog_revision'", [], |r|r.get::<_, i64>(0))? as u64)
}
fn recognized_set(path: &Path) -> Result<bool> {
    let entries = sorted_entries(path)?;
    if entries.len() != 3
        || entries
            .iter()
            .any(|e| !e.file_type().is_ok_and(|t| t.is_file()))
    {
        return Ok(false);
    }
    Ok(crate::import::validate::validate_portrait(path).is_ok())
}
fn sorted_entries(path: &Path) -> Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    if entries.len() > MAX_ENTRIES {
        return Err(CoreError::ExportPreviewLimit);
    }
    entries.sort_by_key(|a| a.file_name());
    Ok(entries)
}
fn preserve_tree(path: &Path, actions: &mut Vec<ExportAction>, depth: usize) -> Result<()> {
    if depth > 64 || actions.len() >= MAX_ENTRIES {
        return Err(CoreError::ExportPreviewLimit);
    }
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() && !meta.file_type().is_symlink() {
        let entries = sorted_entries(path)?;
        if entries.is_empty() {
            actions.push(action(
                Kind::Preserve,
                path.into(),
                "Unrelated empty directory",
            ));
        }
        for entry in entries {
            preserve_tree(&entry.path(), actions, depth + 1)?;
        }
    } else {
        actions.push(action(
            Kind::Preserve,
            path.into(),
            "Unrelated content remains unchanged",
        ));
    }
    Ok(())
}

pub(super) fn safe_target(lib: &Library, path: &Path, output: ExportOutput) -> Result<PathBuf> {
    safe_target_root(lib.root(), path, output)
}
pub(super) fn safe_target_root(root: &Path, path: &Path, output: ExportOutput) -> Result<PathBuf> {
    let target = resolve_target(path)?;
    let library = fs::canonicalize(root)?;
    if target.starts_with(&library) || library.starts_with(&target) || target.parent().is_none() {
        return Err(CoreError::ExportTargetUnsafe);
    }
    for key in ["HOME", "USERPROFILE"] {
        if let Some(home) = std::env::var_os(key).and_then(|p| fs::canonicalize(p).ok()) {
            if home.starts_with(&target) {
                return Err(CoreError::ExportTargetUnsafe);
            }
        }
    }
    let name = target
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if [
        "home",
        "users",
        "steam",
        "steamapps",
        "common",
        "compatdata",
        "drive_c",
        "program files",
        "program files (x86)",
        "pathfinder kingmaker",
        "pathfinder wrath of the righteous",
        "pathfinder second adventure",
    ]
    .contains(&name.as_str())
    {
        return Err(CoreError::ExportTargetUnsafe);
    }
    if target.is_dir()
        && [
            "GameAssembly.dll",
            "UnityPlayer.dll",
            "UnityPlayer.so",
            "steamapps",
            "Kingmaker.exe",
            "Wrath.exe",
        ]
        .iter()
        .any(|p| target.join(p).exists())
    {
        return Err(CoreError::ExportTargetUnsafe);
    }
    if let Ok(meta) = fs::symlink_metadata(&target) {
        if (output == ExportOutput::Directory && !meta.is_dir())
            || (output == ExportOutput::Zip && (!meta.is_file() || has_multiple_links(&meta)))
        {
            return Err(CoreError::ExportTargetUnsafe);
        }
    }
    Ok(target)
}
pub(super) fn resolve_target(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(CoreError::ExportTargetUnsafe);
    }
    let mut ancestor = path.to_path_buf();
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(&ancestor) {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    ancestor
                        .file_name()
                        .ok_or(CoreError::ExportTargetUnsafe)?
                        .to_os_string(),
                );
                if !ancestor.pop() {
                    return Err(CoreError::ExportTargetUnsafe);
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    let mut resolved = fs::canonicalize(&ancestor)?;
    if !missing.is_empty() && !resolved.is_dir() {
        return Err(CoreError::ExportTargetUnsafe);
    }
    for part in missing.iter().rev() {
        resolved.push(part);
    }
    Ok(resolved)
}
/// SHA-256 is only destination overwrite/staleness evidence, never import identity.
pub fn file_digest(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub(super) fn destination_fingerprint(target: &Path) -> Result<String> {
    destination_fingerprint_excluding(target, None)
}
pub(super) fn destination_fingerprint_excluding(
    target: &Path,
    excluded: Option<&Path>,
) -> Result<String> {
    let mut hash = Sha256::new();
    let mut count = 0;
    // Parent identities detect replaced directories even for a previously missing target.
    let mut parent = target.parent();
    while let Some(path) = parent {
        if let Ok(meta) = fs::symlink_metadata(path) {
            hash.update(path.as_os_str().as_encoded_bytes());
            hash.update(identity(&meta));
        }
        parent = path.parent();
    }
    fingerprint_tree(target, &mut hash, &mut count, 0, excluded)?;
    Ok(format!("{:x}", hash.finalize()))
}
fn fingerprint_tree(
    path: &Path,
    hash: &mut Sha256,
    count: &mut usize,
    depth: usize,
    excluded: Option<&Path>,
) -> Result<()> {
    if excluded == Some(path) {
        return Ok(());
    }
    *count += 1;
    if *count > MAX_ENTRIES || depth > 64 {
        return Err(CoreError::ExportPreviewLimit);
    }
    hash.update((path.as_os_str().as_encoded_bytes().len() as u64).to_le_bytes());
    hash.update(path.as_os_str().as_encoded_bytes());
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            hash.update(b"missing");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    hash.update(identity(&meta));
    if meta.file_type().is_symlink() {
        hash.update(b"link");
        hash.update(fs::read_link(path)?.as_os_str().as_encoded_bytes());
    } else if meta.is_dir() {
        hash.update(b"directory");
        for entry in sorted_entries(path)? {
            fingerprint_tree(&entry.path(), hash, count, depth + 1, excluded)?;
        }
    } else if meta.is_file() {
        hash.update(b"file");
        hash.update(meta.len().to_le_bytes());
        hash.update(file_digest(path)?);
    } else {
        hash.update(b"special");
    }
    Ok(())
}
#[cfg(unix)]
pub(super) fn identity(meta: &fs::Metadata) -> Vec<u8> {
    use std::os::unix::fs::MetadataExt;
    [
        meta.dev().to_le_bytes(),
        meta.ino().to_le_bytes(),
        u64::from(meta.mode()).to_le_bytes(),
    ]
    .concat()
}
#[cfg(windows)]
pub(super) fn identity(meta: &fs::Metadata) -> Vec<u8> {
    use std::os::windows::fs::MetadataExt;
    [
        meta.creation_time().to_le_bytes(),
        u64::from(meta.file_attributes()).to_le_bytes(),
    ]
    .concat()
}
#[cfg(not(any(unix, windows)))]
pub(super) fn identity(meta: &fs::Metadata) -> Vec<u8> {
    format!("{:?}", meta.created()).into_bytes()
}
#[cfg(unix)]
fn has_multiple_links(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.nlink() > 1
}
#[cfg(not(unix))]
fn has_multiple_links(_meta: &fs::Metadata) -> bool {
    false
}
