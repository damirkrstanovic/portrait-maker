#![allow(dead_code)] // Shared fixture helpers are deliberately used by different integration targets.

use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};
use portrait_core::Library;
use portrait_core::catalog::refresh_search_document;
use rusqlite::params;
use uuid::Uuid;

pub const SMALL: (u32, u32) = (185, 242);
pub const MEDIUM: (u32, u32) = (330, 432);
pub const LARGE: (u32, u32) = (692, 1024);

pub fn write_portrait(root: &Path, folder: &str) -> PathBuf {
    write_portrait_with_dimensions(root, folder, SMALL, MEDIUM, LARGE)
}

pub fn write_portrait_with_dimensions(
    root: &Path,
    folder: &str,
    small: (u32, u32),
    medium: (u32, u32),
    large: (u32, u32),
) -> PathBuf {
    let directory = root.join(folder);
    std::fs::create_dir_all(&directory).unwrap();
    for (name, (width, height)) in [
        ("Small.png", small),
        ("Medium.png", medium),
        ("Fulllength.png", large),
    ] {
        RgbaImage::from_pixel(width, height, Rgba([30, 40, 50, 255]))
            .save(directory.join(name))
            .unwrap();
    }
    directory
}

/// Seed complete, queryable catalog records in one transaction for scale tests.
pub fn seed_catalog(library: &mut Library, count: usize) -> Vec<Uuid> {
    let transaction = library.connection().unchecked_transaction().unwrap();
    let source_id = Uuid::new_v4();
    transaction
        .execute(
            "INSERT INTO sources (id, name, kind) VALUES (?1, 'Generated pack', 'folder')",
            [source_id.to_string()],
        )
        .unwrap();
    let mut ids = Vec::with_capacity(count);
    for index in 0..count {
        let id = Uuid::new_v4();
        transaction
            .execute(
                "INSERT INTO portraits (id, source_id, name, original_folder) VALUES (?1, ?2, ?3, ?4)",
                params![
                    id.to_string(),
                    source_id.to_string(),
                    format!("Generated portrait {index:04}"),
                    format!("generated-{index:04}")
                ],
            )
            .unwrap();
        refresh_search_document(&transaction, id).unwrap();
        ids.push(id);
    }
    transaction.commit().unwrap();
    ids
}
