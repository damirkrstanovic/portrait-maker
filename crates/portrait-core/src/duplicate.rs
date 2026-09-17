//! Exact duplicate detection for complete, managed portrait sets.
//!
//! A fingerprint contains the role, decoded dimensions and RGBA pixels of every required
//! image. It deliberately does not hash PNG bytes: equivalent PNG encoders and metadata
//! therefore produce the same result, while a changed crop in any role does not.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::{
    fs,
    io::{BufReader, Read},
};

use image::{Rgba, RgbaImage, imageops};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::catalog::{bump_catalog_revision, refresh_search_document};
use crate::import::{JobContext, validate::AssetSpec};
use crate::types::{
    DuplicateConsolidation, DuplicateConsolidationReport, DuplicateGroup, DuplicateMember,
    DuplicateScanReport, ImportDuplicateMatch, ImportDuplicateReport, ImportRequest, Issue,
    IssueSeverity, Label, Portrait, Role,
};
use crate::{CoreError, Library, Result};

const ROLES: [Role; 3] = [Role::Small, Role::Medium, Role::Large];
const SET_ALGORITHM: &str = "portrait-set-rgba-v2";

#[derive(Clone)]
struct ManagedWrite {
    portrait_id: Uuid,
    fingerprint: String,
    stamps: [String; 3],
}

/// A short-lived cache collector. Reads use SQLite immediately, while misses are committed in
/// one transaction when a scan/import finishes. This keeps the cache an optimisation rather
/// than a source of truth.
pub(crate) struct FingerprintCache {
    pixels: HashMap<(String, String), (String, u32, u32)>,
    pixel_writes: Vec<(String, String, String, u32, u32)>,
    managed_writes: Vec<ManagedWrite>,
}

impl FingerprintCache {
    pub(crate) fn new() -> Self {
        Self {
            pixels: HashMap::new(),
            pixel_writes: Vec::new(),
            managed_writes: Vec::new(),
        }
    }

    fn pixel(
        &mut self,
        connection: &rusqlite::Connection,
        asset: &AssetSpec,
        resize: bool,
    ) -> Result<(String, u32, u32)> {
        for _ in 0..2 {
            let (encoded, stamp) = stable_encoded_sha256(&asset.original_path)?;
            let renderer = renderer(asset.role, resize);
            let key = (encoded.clone(), renderer.clone());
            if let Some(value) = self.pixels.get(&key) {
                return Ok(value.clone());
            }
            if let Some(value) = connection.query_row(
                "SELECT pixel_hash,width,height FROM pixel_fingerprints WHERE encoded_sha256=?1 AND renderer=?2",
                params![encoded, renderer],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            ).optional()? {
                self.pixels.insert(key, value.clone());
                return Ok(value);
            }
            let value = rendered_pixel_hash(asset, resize)?;
            if asset_stamp(&asset.original_path)? != stamp {
                continue;
            }
            self.pixels.insert(key.clone(), value.clone());
            self.pixel_writes
                .push((key.0, key.1, value.0.clone(), value.1, value.2));
            return Ok(value);
        }
        Err(CoreError::InvalidPng(
            "image changed while its duplicate fingerprint was being calculated".into(),
        ))
    }

    pub(crate) fn record_managed(
        &mut self,
        portrait_id: Uuid,
        fingerprint: String,
        stamps: [String; 3],
    ) {
        self.managed_writes.push(ManagedWrite {
            portrait_id,
            fingerprint,
            stamps,
        });
    }

    pub(crate) fn flush(&mut self, connection: &rusqlite::Connection) -> Result<()> {
        if self.pixel_writes.is_empty() && self.managed_writes.is_empty() {
            return Ok(());
        }
        connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            for (encoded, renderer, pixel, width, height) in self.pixel_writes.drain(..) {
                connection.execute("INSERT INTO pixel_fingerprints(encoded_sha256,renderer,pixel_hash,width,height) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(encoded_sha256,renderer) DO UPDATE SET pixel_hash=excluded.pixel_hash,width=excluded.width,height=excluded.height", params![encoded,renderer,pixel,width,height])?;
            }
            for write in self.managed_writes.drain(..) {
                connection.execute("INSERT INTO portrait_fingerprints(portrait_id,algorithm,fingerprint,small_stamp,medium_stamp,large_stamp) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(portrait_id) DO UPDATE SET algorithm=excluded.algorithm,fingerprint=excluded.fingerprint,small_stamp=excluded.small_stamp,medium_stamp=excluded.medium_stamp,large_stamp=excluded.large_stamp", params![write.portrait_id.to_string(), SET_ALGORITHM, write.fingerprint, write.stamps[0], write.stamps[1], write.stamps[2]])?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => connection.execute_batch("COMMIT")?,
            Err(error) => {
                let _ = connection.execute_batch("ROLLBACK");
                return Err(error);
            }
        }
        Ok(())
    }
}

pub(crate) fn fingerprint_assets_cached(
    connection: &rusqlite::Connection,
    cache: &mut FingerprintCache,
    assets: &[AssetSpec],
    resize: bool,
) -> Result<String> {
    let mut by_role = BTreeMap::new();
    for asset in assets {
        by_role.insert(asset.role, asset);
    }
    let mut hash = Sha256::new();
    hash.update(SET_ALGORITHM.as_bytes());
    hash.update([0]);
    for role in ROLES {
        let asset = by_role.get(&role).ok_or(CoreError::IncompletePortraitSet)?;
        let (pixel, width, height) = cache.pixel(connection, asset, resize)?;
        hash.update([role_byte(role)]);
        hash.update(width.to_le_bytes());
        hash.update(height.to_le_bytes());
        hash.update(pixel.as_bytes());
        hash.update([0]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn renderer(role: Role, resize: bool) -> String {
    if resize {
        format!("fit-rgba16-v2-{}", role_byte(role))
    } else {
        "raw-rgba16-v2".into()
    }
}

fn encoded_sha256(path: &std::path::Path) -> Result<String> {
    let mut reader = BufReader::new(fs::File::open(path)?);
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn stable_encoded_sha256(path: &std::path::Path) -> Result<(String, String)> {
    for _ in 0..2 {
        let before = asset_stamp(path)?;
        let hash = encoded_sha256(path)?;
        if asset_stamp(path)? == before {
            return Ok((hash, before));
        }
    }
    Err(CoreError::InvalidPng(
        "image changed while its duplicate fingerprint was being calculated".into(),
    ))
}

fn rendered_pixel_hash(asset: &AssetSpec, resize: bool) -> Result<(String, u32, u32)> {
    let image = crate::import::validate::decode_png(&asset.original_path)?;
    if !resize {
        let image = image.to_rgba16();
        return Ok((hash_rgba16(image.as_raw()), image.width(), image.height()));
    }
    let (width, height) = target_dimensions(asset.role);
    let resized = image
        .resize(width, height, imageops::FilterType::Lanczos3)
        .to_rgba8();
    let mut canvas = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]));
    imageops::overlay(
        &mut canvas,
        &resized,
        i64::from((width - resized.width()) / 2),
        i64::from((height - resized.height()) / 2),
    );
    Ok((hash_rgba8_as_16(canvas.as_raw()), width, height))
}

fn hash_rgba16(channels: &[u16]) -> String {
    let mut hash = Sha256::new();
    let mut bytes = [0_u8; 8192];
    for chunk in channels.chunks(bytes.len() / 2) {
        for (index, channel) in chunk.iter().enumerate() {
            bytes[index * 2..index * 2 + 2].copy_from_slice(&channel.to_le_bytes());
        }
        hash.update(&bytes[..chunk.len() * 2]);
    }
    format!("{:x}", hash.finalize())
}

fn hash_rgba8_as_16(channels: &[u8]) -> String {
    let mut hash = Sha256::new();
    let mut bytes = [0_u8; 8192];
    for chunk in channels.chunks(bytes.len() / 2) {
        for (index, channel) in chunk.iter().enumerate() {
            bytes[index * 2..index * 2 + 2]
                .copy_from_slice(&(u16::from(*channel) * 257).to_le_bytes());
        }
        hash.update(&bytes[..chunk.len() * 2]);
    }
    format!("{:x}", hash.finalize())
}

pub(crate) fn asset_stamps(assets: &[AssetSpec]) -> Result<[String; 3]> {
    let mut stamps = BTreeMap::new();
    for asset in assets {
        stamps.insert(asset.role, asset_stamp(&asset.original_path)?);
    }
    Ok([
        stamps
            .remove(&Role::Small)
            .ok_or(CoreError::IncompletePortraitSet)?,
        stamps
            .remove(&Role::Medium)
            .ok_or(CoreError::IncompletePortraitSet)?,
        stamps
            .remove(&Role::Large)
            .ok_or(CoreError::IncompletePortraitSet)?,
    ])
}

fn asset_stamp(path: &std::path::Path) -> Result<String> {
    let metadata = fs::metadata(path)?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|time| time.as_nanos())
        .unwrap_or_default();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(format!(
            "{}:{}:{}:{}:{}:{}:{}",
            metadata.len(),
            modified,
            metadata.dev(),
            metadata.ino(),
            metadata.ctime(),
            metadata.ctime_nsec(),
            metadata.mtime_nsec()
        ))
    }
    #[cfg(not(unix))]
    Ok(format!("{}:{}", metadata.len(), modified))
}

fn target_dimensions(role: Role) -> (u32, u32) {
    match role {
        Role::Small => (185, 242),
        Role::Medium => (330, 432),
        Role::Large => (692, 1024),
    }
}

fn role_byte(role: Role) -> u8 {
    match role {
        Role::Small => 1,
        Role::Medium => 2,
        Role::Large => 3,
    }
}

pub fn scan_duplicates(library: &Library, job: &JobContext) -> Result<DuplicateScanReport> {
    let mut fingerprints: BTreeMap<String, Vec<Uuid>> = BTreeMap::new();
    let mut issues = Vec::new();
    let ids = active_ids(library)?;
    let mut cache = FingerprintCache::new();
    for (index, id) in ids.iter().enumerate() {
        cancelled(job)?;
        match portrait_fingerprint_cached(library, &mut cache, *id, false) {
            Ok(fingerprint) => fingerprints.entry(fingerprint).or_default().push(*id),
            Err(error) => issues.push(issue(id.to_string(), &error)),
        }
        job.report_status(
            (index + 1) as u64,
            Some(ids.len() as u64),
            "Checking portraits",
        );
    }
    let mut groups = Vec::new();
    for (fingerprint, ids) in fingerprints {
        if ids.len() > 1 {
            groups.push(group(library, fingerprint, &ids)?);
        }
    }
    cache.flush(library.connection())?;
    Ok(DuplicateScanReport { groups, issues })
}

/// Reviews incoming files against active library content and against earlier incoming sets.
/// The source is extracted only for this short-lived scan and is never imported here.
pub fn scan_import_duplicates(
    library: &Library,
    request: &ImportRequest,
    job: &JobContext,
) -> Result<ImportDuplicateReport> {
    let extracted = match request.kind {
        crate::types::ImportKind::Archive => {
            Some(extract_for_duplicate_scan(library, request, job)?)
        }
        crate::types::ImportKind::Folder | crate::types::ImportKind::Game => None,
    };
    let input = extracted.as_deref().unwrap_or(&request.path);
    let result = (|| {
        let mut cache = FingerprintCache::new();
        let mut existing = HashMap::new();
        let library_ids = active_ids(library)?;
        let library_total = library_ids.len() as u64;
        for (index, id) in library_ids.into_iter().enumerate() {
            cancelled(job)?;
            if let Ok(fingerprint) = portrait_fingerprint_cached(library, &mut cache, id, false) {
                existing.entry(fingerprint).or_insert(id);
            }
            job.report_status(
                (index + 1) as u64,
                Some(library_total),
                "Checking library duplicates",
            );
        }
        let candidates = crate::import::scan::scan(input, job, &root_context(request))?;
        let mut batch: HashMap<String, String> = HashMap::new();
        let mut matches = Vec::new();
        let mut issues = Vec::new();
        for (index, candidate) in candidates.iter().enumerate() {
            cancelled(job)?;
            match &candidate.assets {
                Ok(assets) => match fingerprint_assets_cached(
                    library.connection(),
                    &mut cache,
                    assets,
                    request.resize,
                ) {
                    Ok(fingerprint) => {
                        let matching_id = existing.get(&fingerprint).copied();
                        let matching_name = matching_id
                            .map(|id| portrait_name(library, id))
                            .transpose()?;
                        let batch_folder = batch.get(&fingerprint).cloned();
                        if matching_id.is_some() || batch_folder.is_some() {
                            matches.push(ImportDuplicateMatch {
                                folder: candidate.context_folder.clone(),
                                name: candidate_name(candidate),
                                matching_portrait_id: matching_id,
                                matching_name,
                                duplicate_of_in_batch: batch_folder,
                            });
                        }
                        batch
                            .entry(fingerprint)
                            .or_insert_with(|| candidate.context_folder.clone());
                    }
                    Err(error) => issues.push(issue(candidate.context_folder.clone(), &error)),
                },
                Err(error) => issues.push(issue(candidate.context_folder.clone(), error)),
            }
            job.report_status(
                (index + 1) as u64,
                Some(candidates.len() as u64),
                "Checking import duplicates",
            );
        }
        cache.flush(library.connection())?;
        Ok(ImportDuplicateReport { matches, issues })
    })();
    if let Some(path) = extracted {
        let _ = fs::remove_dir_all(path);
    }
    result
}

pub fn consolidate_duplicates(
    library: &mut Library,
    groups: &[DuplicateConsolidation],
) -> Result<DuplicateConsolidationReport> {
    let mut requested = HashSet::new();
    let keep_ids = groups
        .iter()
        .map(|group| group.keep_id)
        .collect::<HashSet<_>>();
    for group in groups {
        if group.remove_ids.is_empty() || group.remove_ids.contains(&group.keep_id) {
            return Err(CoreError::InvalidDuplicateConsolidation);
        }
        for id in &group.remove_ids {
            if keep_ids.contains(id) || !requested.insert(*id) {
                return Err(CoreError::InvalidDuplicateConsolidation);
            }
        }
        let keep_fingerprint = active_fingerprint(library, group.keep_id)?;
        for id in &group.remove_ids {
            if active_fingerprint(library, *id)? != keep_fingerprint {
                return Err(CoreError::InvalidDuplicateConsolidation);
            }
        }
    }
    if groups.is_empty() {
        return Ok(DuplicateConsolidationReport { trashed: 0 });
    }
    let transaction = library.connection().unchecked_transaction()?;
    let mut trashed = 0_u64;
    for group in groups {
        for remove_id in &group.remove_ids {
            transaction.execute(
                "INSERT OR IGNORE INTO portrait_sources (portrait_id, source_id) SELECT ?1, source_id FROM portrait_sources WHERE portrait_id = ?2",
                params![group.keep_id.to_string(), remove_id.to_string()],
            )?;
            transaction.execute(
                "INSERT INTO portrait_labels (portrait_id, label_id, origin, producer, producer_version, provenance) \
                 SELECT ?1, pl.label_id, pl.origin, pl.producer, pl.producer_version, pl.provenance \
                 FROM portrait_labels pl JOIN labels l ON l.id = pl.label_id \
                 WHERE pl.portrait_id = ?2 AND NOT EXISTS ( \
                    SELECT 1 FROM user_label_suppressions suppressed \
                    WHERE suppressed.portrait_id = ?1 AND suppressed.category = l.category \
                    AND suppressed.normalized_value = l.normalized_value) \
                 ON CONFLICT(portrait_id, label_id) DO UPDATE SET \
                    origin = CASE WHEN excluded.origin = 'user' THEN 'user' ELSE portrait_labels.origin END, \
                    producer = CASE WHEN excluded.origin = 'user' THEN excluded.producer ELSE portrait_labels.producer END, \
                    producer_version = CASE WHEN excluded.origin = 'user' THEN excluded.producer_version ELSE portrait_labels.producer_version END, \
                    provenance = CASE WHEN excluded.origin = 'user' THEN excluded.provenance ELSE portrait_labels.provenance END",
                params![group.keep_id.to_string(), remove_id.to_string()],
            )?;
            transaction.execute(
                "UPDATE portraits SET trashed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1 AND trashed_at IS NULL",
                [remove_id.to_string()],
            )?;
            transaction.execute(
                "INSERT OR IGNORE INTO selection (portrait_id) \
                 SELECT ?1 WHERE EXISTS (SELECT 1 FROM selection WHERE portrait_id = ?2)",
                params![group.keep_id.to_string(), remove_id.to_string()],
            )?;
            transaction.execute(
                "DELETE FROM selection WHERE portrait_id = ?1",
                [remove_id.to_string()],
            )?;
            trashed += 1;
        }
        refresh_search_document(&transaction, group.keep_id)?;
    }
    bump_catalog_revision(&transaction)?;
    transaction.commit()?;
    Ok(DuplicateConsolidationReport { trashed })
}

pub(crate) fn active_fingerprints(
    library: &Library,
    job: &JobContext,
) -> Result<HashMap<String, Uuid>> {
    let mut result = HashMap::new();
    let ids = active_ids(library)?;
    let total = ids.len() as u64;
    let mut cache = FingerprintCache::new();
    for (index, id) in ids.into_iter().enumerate() {
        cancelled(job)?;
        if let Ok(fingerprint) = portrait_fingerprint_cached(library, &mut cache, id, false) {
            result.entry(fingerprint).or_insert(id);
        }
        job.report_status(
            (index + 1) as u64,
            Some(total),
            "Checking library duplicates",
        );
    }
    cache.flush(library.connection())?;
    Ok(result)
}

fn portrait_assets(library: &Library, id: Uuid) -> Result<Vec<AssetSpec>> {
    let mut statement = library.connection().prepare(
        "SELECT role, relative_path, width, height FROM assets WHERE portrait_id = ?1 ORDER BY role",
    )?;
    let rows = statement
        .query_map([id.to_string()], |row| {
            let role: String = row.get(0)?;
            Ok((role, row.get::<_, String>(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<std::result::Result<Vec<(String, String, u32, u32)>, _>>()?;
    rows.into_iter()
        .map(|(role, relative_path, width, height)| {
            Ok(AssetSpec {
                role: parse_role(&role)?,
                original_path: library.root().join(relative_path),
                width,
                height,
            })
        })
        .collect::<Result<Vec<_>>>()
}

fn portrait_fingerprint_cached(
    library: &Library,
    cache: &mut FingerprintCache,
    id: Uuid,
    force: bool,
) -> Result<String> {
    let assets = portrait_assets(library, id)?;
    let stamps = asset_stamps(&assets)?;
    if !force
        && let Some(fingerprint) = library.connection().query_row(
            "SELECT fingerprint FROM portrait_fingerprints WHERE portrait_id=?1 AND algorithm=?2 AND small_stamp=?3 AND medium_stamp=?4 AND large_stamp=?5",
            params![id.to_string(), SET_ALGORITHM, stamps[0], stamps[1], stamps[2]],
            |row| row.get(0),
        ).optional()?
    {
        return Ok(fingerprint);
    }
    let fingerprint = fingerprint_assets_cached(library.connection(), cache, &assets, false)?;
    if asset_stamps(&assets)? == stamps {
        cache.record_managed(id, fingerprint.clone(), stamps);
    }
    Ok(fingerprint)
}

fn active_fingerprint(library: &Library, id: Uuid) -> Result<String> {
    let active = library.connection().query_row(
        "SELECT EXISTS(SELECT 1 FROM portraits WHERE id = ?1 AND trashed_at IS NULL)",
        [id.to_string()],
        |row| row.get::<_, bool>(0),
    )?;
    if !active {
        return Err(CoreError::InvalidDuplicateConsolidation);
    }
    // Consolidation is destructive: never trust a stat-cache entry here.
    let mut cache = FingerprintCache::new();
    let fingerprint = portrait_fingerprint_cached(library, &mut cache, id, true)?;
    cache.flush(library.connection())?;
    Ok(fingerprint)
}

fn active_ids(library: &Library) -> Result<Vec<Uuid>> {
    let mut statement = library
        .connection()
        .prepare("SELECT id FROM portraits WHERE trashed_at IS NULL ORDER BY id")?;
    statement
        .query_map([], |row| row.get::<_, String>(0))?
        .map(|id| Uuid::parse_str(&id?).map_err(|error| CoreError::Migration(error.to_string())))
        .collect()
}

fn group(library: &Library, fingerprint: String, ids: &[Uuid]) -> Result<DuplicateGroup> {
    let mut members = Vec::with_capacity(ids.len());
    for id in ids {
        members.push(DuplicateMember {
            portrait: portrait(library, *id)?,
            source_names: source_names(library, *id)?,
        });
    }
    let name_conflict = members
        .iter()
        .map(|m| m.portrait.name.as_str())
        .collect::<HashSet<_>>()
        .len()
        > 1;
    let description_conflict = members
        .iter()
        .map(|m| m.portrait.description.as_deref().unwrap_or(""))
        .collect::<HashSet<_>>()
        .len()
        > 1;
    Ok(DuplicateGroup {
        fingerprint,
        members,
        name_conflict,
        description_conflict,
    })
}

fn portrait(library: &Library, id: Uuid) -> Result<Portrait> {
    let mut result = library.connection().query_row(
        "SELECT p.id,p.source_id,p.name,s.name,p.original_folder,p.description, EXISTS(SELECT 1 FROM selection x WHERE x.portrait_id=p.id),p.trashed_at FROM portraits p JOIN sources s ON s.id=p.source_id WHERE p.id=?1",
        [id.to_string()], |row| Ok(Portrait { id: Uuid::parse_str(&row.get::<_,String>(0)?).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?, source_id: Uuid::parse_str(&row.get::<_,String>(1)?).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?, name:row.get(2)?, source_name:row.get(3)?, original_folder:row.get(4)?, description:row.get(5)?, labels:Vec::new(), selected:row.get(6)?, trashed_at:row.get(7)? }))?;
    let mut labels = library.connection().prepare("SELECT l.category,l.normalized_value FROM portrait_labels pl JOIN labels l ON l.id=pl.label_id WHERE pl.portrait_id=?1 ORDER BY l.category,l.normalized_value")?;
    result.labels = labels
        .query_map([id.to_string()], |r| {
            Ok(Label {
                category: r.get(0)?,
                value: r.get(1)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(result)
}

fn source_names(library: &Library, id: Uuid) -> Result<Vec<String>> {
    let mut statement = library.connection().prepare("SELECT s.name FROM portrait_sources ps JOIN sources s ON s.id=ps.source_id WHERE ps.portrait_id=?1 ORDER BY s.name COLLATE NOCASE")?;
    Ok(statement
        .query_map([id.to_string()], |row| row.get(0))?
        .collect::<std::result::Result<_, _>>()?)
}

fn portrait_name(library: &Library, id: Uuid) -> Result<String> {
    library
        .connection()
        .query_row(
            "SELECT name FROM portraits WHERE id=?1",
            [id.to_string()],
            |row| row.get(0),
        )
        .map_err(Into::into)
}

fn parse_role(value: &str) -> Result<Role> {
    match value {
        "small" => Ok(Role::Small),
        "medium" => Ok(Role::Medium),
        "large" => Ok(Role::Large),
        _ => Err(CoreError::InvalidPng("unknown managed image role".into())),
    }
}
fn candidate_name(candidate: &crate::import::scan::Candidate) -> String {
    candidate
        .directory
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("Portrait")
        .to_owned()
}
fn root_context(request: &ImportRequest) -> String {
    if matches!(request.kind, crate::types::ImportKind::Archive) {
        request.source_name.clone()
    } else {
        request
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .unwrap_or("Portrait")
            .to_owned()
    }
}
fn extract_for_duplicate_scan(
    library: &Library,
    request: &ImportRequest,
    job: &JobContext,
) -> Result<std::path::PathBuf> {
    let path = library
        .root()
        .join("staging")
        .join(format!("duplicate-scan-{}", Uuid::new_v4()));
    crate::import::archive::extract_archive(
        &request.path,
        &path,
        &crate::import::ExtractionLimits::default(),
        job,
    )?;
    Ok(path)
}
fn cancelled(job: &JobContext) -> Result<()> {
    if job.is_cancelled() {
        Err(CoreError::Cancelled)
    } else {
        Ok(())
    }
}
fn issue(path: String, error: &CoreError) -> Issue {
    Issue {
        path,
        code: error.code().into(),
        message: error.to_string(),
        severity: IssueSeverity::Warning,
    }
}
