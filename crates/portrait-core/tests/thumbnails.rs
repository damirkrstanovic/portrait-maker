use std::fs;

use image::{Rgba, RgbaImage};
use portrait_core::thumbnails::thumbnail;
use portrait_core::types::Role;
use portrait_core::{CoreError, Library};
use rusqlite::{Transaction, params};
use uuid::Uuid;

fn insert_portrait(library: &Library, id: Uuid, color: [u8; 4]) {
    let source = Uuid::new_v4();
    let portrait_dir = library.root().join("portraits").join(id.to_string());
    fs::create_dir(&portrait_dir).unwrap();
    let image_path = portrait_dir.join("Small.png");
    RgbaImage::from_pixel(640, 320, Rgba(color))
        .save(&image_path)
        .unwrap();
    let transaction = library.connection().unchecked_transaction().unwrap();
    transaction
        .execute(
            "INSERT INTO sources (id, name, kind) VALUES (?1, 'Test set', 'folder')",
            [source.to_string()],
        )
        .unwrap();
    transaction
        .execute(
            "INSERT INTO portraits (id, source_id, name, original_folder) VALUES (?1, ?2, 'Test portrait', 'test')",
            params![id.to_string(), source.to_string()],
        )
        .unwrap();
    insert_asset(&transaction, id, "portraits", 1);
    transaction.commit().unwrap();
}

fn insert_asset(transaction: &Transaction<'_>, id: Uuid, directory: &str, size: i64) {
    transaction
        .execute(
            "INSERT INTO assets (portrait_id, role, relative_path, width, height, file_size) \
             VALUES (?1, 'small', ?2, 640, 320, ?3)",
            params![id.to_string(), format!("{directory}/{id}/Small.png"), size],
        )
        .unwrap();
}

#[test]
fn rejects_unknown_portraits_and_edges_outside_the_asset_contract() {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::create(&temp.path().join("library")).unwrap();

    assert!(matches!(
        thumbnail(&library, Uuid::new_v4(), Role::Small, 240),
        Err(CoreError::PortraitNotFound)
    ));
    let id = Uuid::new_v4();
    insert_portrait(&library, id, [20, 30, 40, 255]);
    assert!(matches!(
        thumbnail(&library, id, Role::Small, 0),
        Err(CoreError::InvalidThumbnailEdge)
    ));
    assert!(matches!(
        thumbnail(&library, id, Role::Small, 1025),
        Err(CoreError::InvalidThumbnailEdge)
    ));
}

#[test]
fn generates_a_png_bounded_by_requested_edge_without_changing_the_original() {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::create(&temp.path().join("library")).unwrap();
    let id = Uuid::new_v4();
    insert_portrait(&library, id, [20, 30, 40, 255]);
    let original = library
        .root()
        .join("portraits")
        .join(id.to_string())
        .join("Small.png");
    let original_bytes = fs::read(&original).unwrap();

    let cached = thumbnail(&library, id, Role::Small, 200).unwrap();
    let dimensions = image::image_dimensions(&cached).unwrap();

    assert_eq!(dimensions, (200, 100));
    assert!(cached.starts_with(library.root().join("cache")));
    assert_eq!(fs::read(original).unwrap(), original_bytes);
}

#[test]
fn does_not_upscale_small_portraits_for_a_larger_grid_edge() {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::create(&temp.path().join("library")).unwrap();
    let id = Uuid::new_v4();
    insert_portrait(&library, id, [20, 30, 40, 255]);
    let original = library
        .root()
        .join("portraits")
        .join(id.to_string())
        .join("Small.png");
    RgbaImage::from_pixel(100, 50, Rgba([20, 30, 40, 255]))
        .save(&original)
        .unwrap();

    let cached = thumbnail(&library, id, Role::Small, 360).unwrap();

    assert_eq!(image::image_dimensions(cached).unwrap(), (100, 50));
}

#[test]
fn regenerated_cache_uses_changed_asset_metadata_and_library_roots_do_not_cross() {
    let temp = tempfile::tempdir().unwrap();
    let first = Library::create(&temp.path().join("first")).unwrap();
    let second = Library::create(&temp.path().join("second")).unwrap();
    let id = Uuid::new_v4();
    insert_portrait(&first, id, [20, 30, 40, 255]);
    insert_portrait(&second, id, [80, 90, 100, 255]);

    let initial = thumbnail(&first, id, Role::Small, 160).unwrap();
    first
        .connection()
        .execute(
            "UPDATE assets SET file_size = 2 WHERE portrait_id = ?1 AND role = 'small'",
            [id.to_string()],
        )
        .unwrap();
    let regenerated = thumbnail(&first, id, Role::Small, 160).unwrap();
    let switched = thumbnail(&second, id, Role::Small, 160).unwrap();

    assert_ne!(initial, regenerated);
    assert!(regenerated.is_file());
    assert!(switched.starts_with(second.root()));
    assert!(!switched.starts_with(first.root()));
}
