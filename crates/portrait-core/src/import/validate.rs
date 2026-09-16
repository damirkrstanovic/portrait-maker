use std::fs::File;
use std::path::{Path, PathBuf};

use image::{GenericImageView, ImageFormat, ImageReader, Limits};

use super::JobContext;
use crate::types::Role;
use crate::{CoreError, Result};

const MAX_PIXELS: u64 = 16_000_000;
const MAX_ALLOCATION: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct AssetSpec {
    pub role: Role,
    pub original_path: PathBuf,
    pub width: u32,
    pub height: u32,
}

pub fn validate_portrait(directory: &Path) -> Result<Vec<AssetSpec>> {
    validate_portrait_with_job(directory, &JobContext::default())
}

pub(crate) fn validate_portrait_with_job(
    directory: &Path,
    job: &JobContext,
) -> Result<Vec<AssetSpec>> {
    let mut found = std::collections::BTreeMap::new();
    let mut has_candidate = false;
    let mut unsupported = false;
    for entry in std::fs::read_dir(directory)? {
        check_cancelled(job)?;
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_ascii_lowercase) else {
            unsupported = true;
            continue;
        };
        let role = match name.as_str() {
            "small.png" => Some(Role::Small),
            "medium.png" => Some(Role::Medium),
            "fulllength.png" => Some(Role::Large),
            _ => None,
        };
        if let Some(role) = role {
            has_candidate = true;
            if found.insert(role, entry.path()).is_some() {
                return Err(CoreError::AmbiguousPortraitSet);
            }
        } else {
            unsupported = true;
        }
    }
    if !has_candidate || unsupported {
        return Err(CoreError::InvalidPortraitSet);
    }
    if found.len() != 3 {
        return Err(CoreError::IncompletePortraitSet);
    }

    [Role::Small, Role::Medium, Role::Large]
        .into_iter()
        .map(|role| {
            check_cancelled(job)?;
            let path = found.remove(&role).expect("all roles were checked above");
            let (width, height) = decode_png(&path)?.dimensions();
            Ok(AssetSpec {
                role,
                original_path: path,
                width,
                height,
            })
        })
        .collect()
}

fn check_cancelled(job: &JobContext) -> Result<()> {
    if job.is_cancelled() {
        Err(CoreError::Cancelled)
    } else {
        Ok(())
    }
}

pub(crate) fn decode_png(path: &Path) -> Result<image::DynamicImage> {
    let header =
        ImageReader::with_format(std::io::BufReader::new(File::open(path)?), ImageFormat::Png)
            .into_dimensions()
            .map_err(|error| CoreError::InvalidPng(error.to_string()))?;
    let pixels = u64::from(header.0) * u64::from(header.1);
    if pixels > MAX_PIXELS {
        return Err(CoreError::InvalidPng(format!(
            "image exceeds {MAX_PIXELS} pixels"
        )));
    }
    let mut reader =
        ImageReader::with_format(std::io::BufReader::new(File::open(path)?), ImageFormat::Png);
    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_ALLOCATION);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|error| CoreError::InvalidPng(error.to_string()))
}
