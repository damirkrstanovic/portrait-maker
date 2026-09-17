use std::path::{Path, PathBuf};

use crate::Result;

use super::{
    JobContext,
    validate::{AssetSpec, inspect_portrait_with_job},
};

#[derive(Debug)]
pub(crate) struct Candidate {
    pub directory: PathBuf,
    pub original_folder: String,
    pub context_folder: String,
    pub inference_path: String,
    pub assets: Result<Vec<AssetSpec>>,
}

pub(crate) fn scan(root: &Path, job: &JobContext, root_context: &str) -> Result<Vec<Candidate>> {
    let mut candidates = Vec::new();
    visit(root, root, &mut candidates, job, root_context)?;
    Ok(candidates)
}

fn visit(
    root: &Path,
    directory: &Path,
    candidates: &mut Vec<Candidate>,
    job: &JobContext,
    root_context: &str,
) -> Result<()> {
    if job.is_cancelled() {
        return Err(crate::CoreError::Cancelled);
    }
    let metadata = std::fs::symlink_metadata(directory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Ok(());
    }
    let assets = inspect_portrait_with_job(directory, job);
    match &assets {
        Ok(_) | Err(crate::CoreError::AmbiguousPortraitSet) => {
            candidates.push(candidate(root, directory, root_context, assets))
        }
        Err(crate::CoreError::InvalidPortraitSet) if !has_role_file(directory, job)? => {}
        Err(_) => candidates.push(candidate(root, directory, root_context, assets)),
    }
    for entry in std::fs::read_dir(directory)? {
        if job.is_cancelled() {
            return Err(crate::CoreError::Cancelled);
        }
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() && !file_type.is_symlink() {
            visit(root, &entry.path(), candidates, job, root_context)?;
        }
    }
    Ok(())
}

fn candidate(
    root: &Path,
    directory: &Path,
    root_context: &str,
    assets: Result<Vec<AssetSpec>>,
) -> Candidate {
    let original_folder = directory
        .strip_prefix(root)
        .unwrap_or(directory)
        .to_string_lossy()
        .replace('\\', "/");
    let context_folder = if original_folder.is_empty() {
        root_context.to_owned()
    } else {
        original_folder.clone()
    };
    Candidate {
        directory: directory.to_path_buf(),
        original_folder,
        inference_path: context_folder.clone(),
        context_folder,
        assets,
    }
}

fn has_role_file(directory: &Path, job: &JobContext) -> Result<bool> {
    for entry in std::fs::read_dir(directory)? {
        if job.is_cancelled() {
            return Err(crate::CoreError::Cancelled);
        }
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry.file_name().to_str().is_some_and(|name| {
                matches!(
                    name.to_ascii_lowercase().as_str(),
                    "small.png" | "medium.png" | "fulllength.png"
                )
            })
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::JobContext;
    use crate::metadata::infer_labels;
    use image::{Rgba, RgbaImage};

    #[test]
    fn candidate_classification_stops_when_cancelled() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("Small.png"), "placeholder").unwrap();
        let job = JobContext::default();
        job.cancel();

        let error = has_role_file(temp.path(), &job).unwrap_err();

        assert_eq!(error.code(), "CANCELLED");
    }

    #[test]
    fn archive_root_keeps_empty_provenance_and_uses_source_name_for_inference() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp
            .path()
            .join(format!("extract-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let job = JobContext::default();

        for (name, width, height) in [
            ("Small.png", 185, 242),
            ("Medium.png", 330, 432),
            ("Fulllength.png", 692, 1024),
        ] {
            RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]))
                .save(root.join(name))
                .unwrap();
        }
        let candidate = scan(&root, &job, "female_elf_archer")
            .unwrap()
            .pop()
            .unwrap();

        assert_eq!(candidate.original_folder, "");
        assert_eq!(candidate.context_folder, "female_elf_archer");
        assert_eq!(candidate.inference_path, "female_elf_archer");
        assert!(!candidate.context_folder.contains("extract-"));
        assert_eq!(infer_labels(&candidate.inference_path).len(), 3);
    }
}
