pub mod archive;
pub(crate) mod scan;
mod unicode_casefold;
pub(crate) mod validate;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage, imageops};
use rusqlite::params;
use serde::Serialize;
use uuid::Uuid;

use crate::catalog::{bump_catalog_revision, refresh_search_document};
use crate::library::Library;
use crate::metadata::infer_labels;
use crate::types::{
    DuplicatePolicy, ImportKind, ImportReport, ImportRequest, Issue, IssueSeverity, Label, Role,
};
use crate::{CoreError, Result};

pub use validate::{AssetSpec, validate_portrait};

type StatusCallback = dyn Fn(u64, Option<u64>, &str) + Send + Sync + 'static;
type ProgressCallback = dyn Fn(u64, Option<u64>) + Send + Sync + 'static;

#[derive(Clone, Default)]
pub struct JobContext {
    cancelled: Arc<AtomicBool>,
    progress: Option<Arc<ProgressCallback>>,
    status: Option<Arc<StatusCallback>>,
}

impl JobContext {
    #[must_use]
    pub fn with_progress(callback: impl Fn(u64, Option<u64>) + Send + Sync + 'static) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            progress: Some(Arc::new(callback)),
            status: None,
        }
    }

    #[must_use]
    pub fn with_status_progress(
        callback: impl Fn(u64, Option<u64>, &str) + Send + Sync + 'static,
    ) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            progress: None,
            status: Some(Arc::new(callback)),
        }
    }
    pub(crate) fn report_status(&self, completed: u64, total: Option<u64>, message: &str) {
        if let Some(callback) = &self.status {
            callback(completed, total, message);
        }
        self.report_progress(completed, total);
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn report_progress(&self, completed: u64, total: Option<u64>) {
        if let Some(callback) = &self.progress {
            callback(completed, total);
        }
    }
}

pub use JobContext as ArchiveJobContext;
pub use archive::ExtractionLimits;

const CANONICAL_DIMENSIONS: [(Role, u32, u32); 3] = [
    (Role::Small, 185, 242),
    (Role::Medium, 330, 432),
    (Role::Large, 692, 1024),
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportIntent {
    portrait_id: Uuid,
}

pub fn import_portraits(
    library: &mut Library,
    request: ImportRequest,
    job: &JobContext,
) -> Result<ImportReport> {
    let mut report = ImportReport {
        source_id: None,
        imported: 0,
        skipped: 0,
        issues: Vec::new(),
        cancelled: false,
    };
    if job.is_cancelled() {
        report.cancelled = true;
        return Ok(report);
    }
    let extracted = match request.kind {
        ImportKind::Archive => match extract_source(library, &request, job) {
            Ok(path) => Some(path),
            Err(CoreError::Cancelled) => {
                report.cancelled = true;
                return Ok(report);
            }
            Err(error) => return Err(error),
        },
        ImportKind::Folder | ImportKind::Game => None,
    };
    let input = extracted.as_deref().unwrap_or(&request.path);
    if matches!(request.kind, ImportKind::Folder | ImportKind::Game) {
        reject_library_overlap(library.root(), input)?;
    }
    let scanned = match scan::scan(input, job, &root_context(&request)) {
        Ok(scanned) => scanned,
        Err(error) => {
            if let Some(staging) = extracted {
                let _ = fs::remove_dir_all(staging);
            }
            if matches!(error, CoreError::Cancelled) {
                report.cancelled = true;
                return Ok(report);
            }
            return Err(error);
        }
    };
    let total = scanned.len() as u64;
    // Pixel fingerprints are computed during the import itself, including Keep imports. This
    // seeds later duplicate reviews without decoding the same PNGs again.
    let mut fingerprint_cache = crate::duplicate::FingerprintCache::new();
    // A single scan keeps duplicate-aware imports linear. Trashed rows are intentionally
    // absent, so importing a trashed portrait creates a usable active copy.
    let mut active_fingerprints = if request.duplicate_policy == DuplicatePolicy::Skip {
        match crate::duplicate::active_fingerprints(library, job) {
            Ok(fingerprints) => fingerprints,
            Err(error) => {
                if let Some(staging) = extracted {
                    let _ = fs::remove_dir_all(staging);
                }
                if matches!(error, CoreError::Cancelled) {
                    report.cancelled = true;
                    return Ok(report);
                }
                return Err(error);
            }
        }
    } else {
        HashMap::new()
    };
    let mut source_id = None;
    for (index, candidate) in scanned.into_iter().enumerate() {
        if job.is_cancelled() {
            report.cancelled = true;
            break;
        }
        let assets = match &candidate.assets {
            Ok(assets) => assets,
            Err(error) => {
                report.skipped += 1;
                report.issues.push(issue(
                    &candidate.context_folder,
                    error,
                    IssueSeverity::Error,
                ));
                job.report_progress((index + 1) as u64, Some(total));
                continue;
            }
        };
        let fingerprint = match crate::duplicate::fingerprint_assets_cached(
            library.connection(),
            &mut fingerprint_cache,
            assets,
            request.resize,
        ) {
            Ok(fingerprint) => Some(fingerprint),
            Err(CoreError::Cancelled) => {
                report.cancelled = true;
                break;
            }
            Err(error) => {
                report.skipped += 1;
                report.issues.push(issue(
                    &candidate.context_folder,
                    &error,
                    IssueSeverity::Error,
                ));
                job.report_progress((index + 1) as u64, Some(total));
                continue;
            }
        };
        for asset in assets {
            if !is_canonical(asset) {
                report.issues.push(Issue {
                    path: candidate.context_folder.clone(),
                    code: "NONSTANDARD_DIMENSIONS".into(),
                    message: format!(
                        "{} is {} × {}; game-ready dimensions are {} × {}.",
                        role_name(asset.role),
                        asset.width,
                        asset.height,
                        target_dimensions(asset.role).0,
                        target_dimensions(asset.role).1
                    ),
                    severity: IssueSeverity::Warning,
                });
            }
        }
        if let Some(existing_portrait) = fingerprint
            .as_ref()
            .and_then(|value| active_fingerprints.get(value))
            .copied()
        {
            match attach_duplicate(
                library,
                &request,
                existing_portrait,
                source_id,
                &candidate.inference_path,
            ) {
                Ok(committed_source) => {
                    source_id = Some(committed_source);
                    report.source_id = source_id;
                    report.skipped += 1;
                }
                Err(error) => {
                    report.skipped += 1;
                    report.issues.push(issue(
                        &candidate.context_folder,
                        &error,
                        IssueSeverity::Error,
                    ));
                }
            }
        } else {
            match stage_and_commit(
                library,
                &request,
                &candidate,
                assets,
                source_id,
                job,
                &mut fingerprint_cache,
            ) {
                Ok(committed) => {
                    source_id = Some(committed.source_id);
                    report.source_id = source_id;
                    report.imported += 1;
                    if request.duplicate_policy == DuplicatePolicy::Skip {
                        active_fingerprints.insert(committed.fingerprint, committed.portrait_id);
                    }
                }
                Err(CoreError::Cancelled) => {
                    report.cancelled = true;
                    break;
                }
                Err(error) => {
                    report.skipped += 1;
                    report.issues.push(issue(
                        &candidate.context_folder,
                        &error,
                        IssueSeverity::Error,
                    ));
                }
            }
        }
        job.report_progress((index + 1) as u64, Some(total));
    }
    if let Some(staging) = extracted {
        let _ = fs::remove_dir_all(staging);
    }
    fingerprint_cache.flush(library.connection())?;
    Ok(report)
}

fn extract_source(library: &Library, request: &ImportRequest, job: &JobContext) -> Result<PathBuf> {
    let operation = Uuid::new_v4();
    let staging = library
        .root()
        .join("staging")
        .join(format!("extract-{operation}"));
    match archive::extract_archive(&request.path, &staging, &ExtractionLimits::default(), job) {
        Ok(()) => Ok(staging),
        Err(CoreError::Cancelled) => Err(CoreError::Cancelled),
        Err(error) => Err(error),
    }
}

fn reject_library_overlap(library_root: &Path, source: &Path) -> Result<()> {
    let source = source.canonicalize()?;
    let library = library_root.canonicalize()?;
    if source.starts_with(&library) || library.starts_with(&source) {
        return Err(CoreError::ImportOverlapsLibrary);
    }
    Ok(())
}

struct ImportCommit {
    source_id: Uuid,
    portrait_id: Uuid,
    fingerprint: String,
}

fn stage_and_commit(
    library: &mut Library,
    request: &ImportRequest,
    candidate: &scan::Candidate,
    assets: &[AssetSpec],
    existing_source: Option<Uuid>,
    job: &JobContext,
    fingerprint_cache: &mut crate::duplicate::FingerprintCache,
) -> Result<ImportCommit> {
    let portrait_id = Uuid::new_v4();
    let operation_id = Uuid::new_v4();
    let staging = library
        .root()
        .join("staging")
        .join(format!("import-{operation_id}"));
    let final_path = library
        .root()
        .join("portraits")
        .join(portrait_id.to_string());
    library.connection().execute(
        "INSERT INTO operation_state (operation_id, kind, state_json) VALUES (?1, 'import', ?2)",
        params![
            operation_id.to_string(),
            serde_json::to_string(&ImportIntent { portrait_id })?
        ],
    )?;
    let result = (|| {
        fs::create_dir(&staging)?;
        let mut outputs = Vec::new();
        for asset in assets {
            if job.is_cancelled() {
                return Err(CoreError::Cancelled);
            }
            let name = role_filename(asset.role);
            let output = staging.join(name);
            let (width, height) = if request.resize {
                resize_to_canvas(&asset.original_path, asset.role, &output)?
            } else {
                fs::copy(&asset.original_path, &output)?;
                (asset.width, asset.height)
            };
            outputs.push((
                asset.role,
                name,
                width,
                height,
                i64::try_from(fs::metadata(output)?.len())
                    .map_err(|_| CoreError::InvalidPng("managed file is too large".into()))?,
            ));
        }
        if job.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        fs::rename(&staging, &final_path)?;
        // Cache the actual managed files. In particular, this does not attach source-file
        // stamps to a copied/resized library asset.
        let managed_assets = outputs
            .iter()
            .map(|(role, filename, width, height, _)| AssetSpec {
                role: *role,
                original_path: final_path.join(filename),
                width: *width,
                height: *height,
            })
            .collect::<Vec<_>>();
        let managed_stamps = crate::duplicate::asset_stamps(&managed_assets)?;
        let managed_fingerprint = crate::duplicate::fingerprint_assets_cached(
            library.connection(),
            fingerprint_cache,
            &managed_assets,
            false,
        )?;
        let source_id = existing_source.unwrap_or_else(Uuid::new_v4);
        let transaction = library.connection().unchecked_transaction()?;
        if existing_source.is_none() {
            transaction.execute(
                "INSERT INTO sources (id, name, kind, original_location) VALUES (?1, ?2, ?3, ?4)",
                params![
                    source_id.to_string(),
                    request.source_name,
                    import_kind(request.kind),
                    request.path.to_string_lossy()
                ],
            )?;
        }
        let name = candidate
            .directory
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("Portrait");
        transaction.execute(
            "INSERT INTO portraits (id, source_id, name, original_folder, provenance) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![portrait_id.to_string(), source_id.to_string(), name, candidate.original_folder, "import-v1"],
        )?;
        transaction.execute(
            "INSERT INTO portrait_sources (portrait_id, source_id) VALUES (?1, ?2)",
            params![portrait_id.to_string(), source_id.to_string()],
        )?;
        for (role, filename, width, height, size) in outputs {
            transaction.execute(
                "INSERT INTO assets (portrait_id, role, relative_path, width, height, file_size) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![portrait_id.to_string(), role_value(role), format!("portraits/{portrait_id}/{filename}"), width, height, size],
            )?;
        }
        insert_labels(&transaction, portrait_id, &candidate.inference_path)?;
        refresh_search_document(&transaction, portrait_id)?;
        bump_catalog_revision(&transaction)?;
        transaction.execute(
            "DELETE FROM operation_state WHERE operation_id = ?1",
            [operation_id.to_string()],
        )?;
        transaction.commit()?;
        // A modified file must never be paired with a fingerprint calculated from earlier
        // bytes. It simply remains uncached and will be safely recalculated on review.
        if crate::duplicate::asset_stamps(&managed_assets)? == managed_stamps {
            fingerprint_cache.record_managed(
                portrait_id,
                managed_fingerprint.clone(),
                managed_stamps,
            );
        }
        Ok(ImportCommit {
            source_id,
            portrait_id,
            fingerprint: managed_fingerprint,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
        let _ = library.connection().execute(
            "DELETE FROM operation_state WHERE operation_id = ?1",
            [operation_id.to_string()],
        );
        if final_path.exists()
            && !library
                .connection()
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM portraits WHERE id = ?1)",
                    [portrait_id.to_string()],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap_or(false)
        {
            let _ = fs::remove_dir_all(&final_path);
        }
    }
    result
}

fn attach_duplicate(
    library: &mut Library,
    request: &ImportRequest,
    portrait_id: Uuid,
    existing_source: Option<Uuid>,
    inference_path: &str,
) -> Result<Uuid> {
    let transaction = library.connection().unchecked_transaction()?;
    let source_id = existing_source.unwrap_or_else(Uuid::new_v4);
    if existing_source.is_none() {
        transaction.execute(
            "INSERT INTO sources (id, name, kind, original_location) VALUES (?1, ?2, ?3, ?4)",
            params![
                source_id.to_string(),
                request.source_name,
                import_kind(request.kind),
                request.path.to_string_lossy()
            ],
        )?;
    }
    transaction.execute(
        "INSERT OR IGNORE INTO portrait_sources (portrait_id, source_id) VALUES (?1, ?2)",
        params![portrait_id.to_string(), source_id.to_string()],
    )?;
    insert_labels(&transaction, portrait_id, inference_path)?;
    refresh_search_document(&transaction, portrait_id)?;
    bump_catalog_revision(&transaction)?;
    transaction.commit()?;
    Ok(source_id)
}

fn insert_labels(
    transaction: &rusqlite::Transaction<'_>,
    portrait_id: Uuid,
    path: &str,
) -> Result<()> {
    for Label { category, value } in infer_labels(path) {
        transaction.execute(
            "INSERT OR IGNORE INTO labels (category, normalized_value, display_value) VALUES (?1, ?2, ?2)",
            params![category, value],
        )?;
        let label_id: i64 = transaction.query_row(
            "SELECT id FROM labels WHERE category = ?1 AND normalized_value = ?2",
            params![category, value],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO portrait_labels (portrait_id, label_id, origin, producer, producer_version) \
             SELECT ?1, ?2, 'filename', 'path-vocabulary', '1' \
             WHERE NOT EXISTS (SELECT 1 FROM user_label_suppressions WHERE portrait_id = ?1 AND category = ?3 AND normalized_value = ?4) \
             ON CONFLICT(portrait_id, label_id) DO NOTHING",
            params![portrait_id.to_string(), label_id, category, value],
        )?;
    }
    Ok(())
}

fn resize_to_canvas(input: &Path, role: Role, output: &Path) -> Result<(u32, u32)> {
    let image = validate::decode_png(input)?;
    let (width, height) = target_dimensions(role);
    let resized = image
        .resize(width, height, imageops::FilterType::Lanczos3)
        .to_rgba8();
    let mut canvas = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]));
    let x = i64::from((width - resized.width()) / 2);
    let y = i64::from((height - resized.height()) / 2);
    imageops::overlay(&mut canvas, &resized, x, y);
    canvas
        .save(output)
        .map_err(|error| CoreError::InvalidPng(error.to_string()))?;
    Ok((width, height))
}

fn issue(path: &str, error: &CoreError, severity: IssueSeverity) -> Issue {
    Issue {
        path: path.into(),
        code: error.code().into(),
        message: error.to_string(),
        severity,
    }
}

fn target_dimensions(role: Role) -> (u32, u32) {
    CANONICAL_DIMENSIONS
        .iter()
        .find(|(candidate, _, _)| *candidate == role)
        .map(|(_, width, height)| (*width, *height))
        .expect("all roles are canonical")
}

fn is_canonical(asset: &AssetSpec) -> bool {
    (asset.width, asset.height) == target_dimensions(asset.role)
}
fn role_filename(role: Role) -> &'static str {
    match role {
        Role::Small => "Small.png",
        Role::Medium => "Medium.png",
        Role::Large => "Fulllength.png",
    }
}
fn role_name(role: Role) -> &'static str {
    match role {
        Role::Small => "Small.png",
        Role::Medium => "Medium.png",
        Role::Large => "Fulllength.png",
    }
}
fn role_value(role: Role) -> &'static str {
    match role {
        Role::Small => "small",
        Role::Medium => "medium",
        Role::Large => "large",
    }
}
fn import_kind(kind: ImportKind) -> &'static str {
    match kind {
        ImportKind::Folder => "folder",
        ImportKind::Archive => "archive",
        ImportKind::Game => "game",
    }
}

fn root_context(request: &ImportRequest) -> String {
    if matches!(request.kind, ImportKind::Archive) {
        return request.source_name.clone();
    }
    request
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Portrait")
        .to_owned()
}
