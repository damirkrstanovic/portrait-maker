use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use image::imageops::FilterType;
use rusqlite::params;
use uuid::Uuid;

use crate::import::validate::decode_png;
use crate::types::Role;
use crate::{CoreError, Library, Result};

pub const MAX_THUMBNAIL_EDGE: u32 = 1024;
pub const MAX_DECODE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_WORKERS: usize = 2;

/// A checked managed image path plus the location where its thumbnails belong.
///
/// It is deliberately independent of the SQLite connection.  Desktop callers
/// look this up while holding their library lock, then can release that lock
/// before doing image decoding and PNG encoding.
#[derive(Debug, Clone)]
pub struct ThumbnailSource {
    path: PathBuf,
    file_size: i64,
    cache_directory: PathBuf,
}

#[derive(Debug, Clone)]
struct Asset {
    path: PathBuf,
    file_size: i64,
}

#[derive(Default)]
struct ThumbnailBudget {
    workers: usize,
    decode_bytes: u64,
}

fn budget() -> &'static (Mutex<ThumbnailBudget>, Condvar) {
    static BUDGET: OnceLock<(Mutex<ThumbnailBudget>, Condvar)> = OnceLock::new();
    BUDGET.get_or_init(|| (Mutex::new(ThumbnailBudget::default()), Condvar::new()))
}

struct BudgetPermit {
    bytes: u64,
}

impl Drop for BudgetPermit {
    fn drop(&mut self) {
        let (lock, wake) = budget();
        if let Ok(mut state) = lock.lock() {
            state.workers -= 1;
            state.decode_bytes -= self.bytes;
            wake.notify_all();
        }
    }
}

fn acquire_budget(bytes: u64) -> Result<BudgetPermit> {
    if bytes > MAX_DECODE_BYTES {
        return Err(CoreError::InvalidPng(
            "image exceeds thumbnail memory budget".into(),
        ));
    }
    let (lock, wake) = budget();
    let mut state = lock
        .lock()
        .map_err(|error| CoreError::Migration(format!("thumbnail worker state: {error}")))?;
    while state.workers >= MAX_WORKERS || state.decode_bytes + bytes > MAX_DECODE_BYTES {
        state = wake
            .wait(state)
            .map_err(|error| CoreError::Migration(format!("thumbnail worker state: {error}")))?;
    }
    state.workers += 1;
    state.decode_bytes += bytes;
    Ok(BudgetPermit { bytes })
}

pub fn thumbnail(library: &Library, id: Uuid, role: Role, edge: u32) -> Result<PathBuf> {
    let source = thumbnail_source(library, id, role)?;
    thumbnail_from_source(&source, id, role, edge)
}

/// Resolve and validate a managed image while a caller has access to a
/// [`Library`].  Rendering the thumbnail itself can happen later without
/// keeping the library (and its SQLite connection) locked.
pub fn thumbnail_source(library: &Library, id: Uuid, role: Role) -> Result<ThumbnailSource> {
    let asset = asset(library, id, role)?;
    Ok(ThumbnailSource {
        path: asset.path,
        file_size: asset.file_size,
        cache_directory: library.root().join("cache").join("thumbnails"),
    })
}

/// Render or retrieve a thumbnail for a previously checked managed image.
pub fn thumbnail_from_source(
    source: &ThumbnailSource,
    id: Uuid,
    role: Role,
    edge: u32,
) -> Result<PathBuf> {
    if !(1..=MAX_THUMBNAIL_EDGE).contains(&edge) {
        return Err(CoreError::InvalidThumbnailEdge);
    }
    let metadata = fs::metadata(&source.path)?;
    let modified = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CoreError::InvalidPng(format!("invalid asset timestamp: {error}")))?;
    let cache_name = format!(
        "{}-{}-{edge}-{}-{}-{}.png",
        id,
        role_name(role),
        source.file_size,
        metadata.len(),
        modified.as_nanos(),
    );
    fs::create_dir_all(&source.cache_directory)?;
    let output = source.cache_directory.join(&cache_name);
    if output.is_file() {
        return Ok(output);
    }

    let dimensions = image::image_dimensions(&source.path)
        .map_err(|error| CoreError::InvalidPng(error.to_string()))?;
    let _permit = acquire_budget(u64::from(dimensions.0) * u64::from(dimensions.1) * 4)?;
    let image = decode_png(&source.path)?;
    let target_edge = edge.min(dimensions.0.max(dimensions.1));
    let resized = image.resize(target_edge, target_edge, FilterType::Triangle);
    let temporary = source
        .cache_directory
        .join(format!(".{cache_name}.{}.tmp", Uuid::new_v4()));
    resized
        .save_with_format(&temporary, image::ImageFormat::Png)
        .map_err(|error| CoreError::InvalidPng(error.to_string()))?;
    match fs::rename(&temporary, &output) {
        Ok(()) => Ok(output),
        Err(_error) if output.is_file() => {
            let _ = fs::remove_file(temporary);
            Ok(output)
        }
        Err(error) => {
            let _ = fs::remove_file(temporary);
            Err(error.into())
        }
    }
}

pub fn asset_path(library: &Library, id: Uuid, role: Role) -> Result<PathBuf> {
    Ok(asset(library, id, role)?.path)
}

fn asset(library: &Library, id: Uuid, role: Role) -> Result<Asset> {
    let relative = library
        .connection()
        .query_row(
            "SELECT relative_path, file_size FROM assets WHERE portrait_id = ?1 AND role = ?2",
            params![id.to_string(), role_name(role)],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => CoreError::PortraitNotFound,
            other => CoreError::Database(other),
        })?;
    let relative_path = Path::new(&relative.0);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(CoreError::UnsafeManagedDirectory);
    }
    let root = library.root().canonicalize()?;
    let path = library.root().join(relative_path);
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(&root) || !canonical.is_file() {
        return Err(CoreError::UnsafeManagedDirectory);
    }
    Ok(Asset {
        path: canonical,
        file_size: relative.1,
    })
}

const fn role_name(role: Role) -> &'static str {
    match role {
        Role::Small => "small",
        Role::Medium => "medium",
        Role::Large => "large",
    }
}
