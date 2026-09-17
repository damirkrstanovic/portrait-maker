mod support;

use std::fs;

use image::{
    ColorType, ImageBuffer, ImageEncoder, Rgba, RgbaImage,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};
use portrait_core::{
    Library,
    catalog::query_catalog,
    duplicate::{consolidate_duplicates, scan_duplicates, scan_import_duplicates},
    import::{JobContext, import_portraits},
    types::{DuplicateConsolidation, DuplicatePolicy, ImportKind, ImportRequest, Page, Query},
};

fn request(path: std::path::PathBuf, source_name: &str, policy: DuplicatePolicy) -> ImportRequest {
    ImportRequest {
        path,
        source_name: source_name.into(),
        kind: ImportKind::Folder,
        resize: false,
        duplicate_policy: policy,
    }
}

#[test]
fn sixteen_bit_pixel_differences_are_not_collapsed() {
    let temp = tempfile::tempdir().unwrap();
    let first = write_sixteen_bit_portrait(temp.path(), "first", 1);
    let second = write_sixteen_bit_portrait(temp.path(), "second", 2);
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    import_portraits(
        &mut library,
        request(first, "first", DuplicatePolicy::Keep),
        &JobContext::default(),
    )
    .unwrap();
    import_portraits(
        &mut library,
        request(second, "second", DuplicatePolicy::Keep),
        &JobContext::default(),
    )
    .unwrap();
    assert!(
        scan_duplicates(&library, &JobContext::default())
            .unwrap()
            .groups
            .is_empty()
    );
}

fn write_sixteen_bit_portrait(
    root: &std::path::Path,
    name: &str,
    first_channel: u16,
) -> std::path::PathBuf {
    let folder = root.join(name);
    fs::create_dir_all(&folder).unwrap();
    for (filename, width, height) in [
        ("Small.png", 185, 242),
        ("Medium.png", 330, 432),
        ("Fulllength.png", 692, 1024),
    ] {
        let image = ImageBuffer::<Rgba<u16>, Vec<u16>>::from_pixel(
            width,
            height,
            Rgba([first_channel, 257, 513, u16::MAX]),
        );
        image.save(folder.join(filename)).unwrap();
    }
    folder
}

#[test]
fn encoded_png_variants_match_but_a_changed_role_does_not() {
    let temp = tempfile::tempdir().unwrap();
    let first = support::write_portrait(temp.path(), "first");
    let second = support::write_portrait(temp.path(), "second");
    // Re-encode the second set with deliberately different PNG compression/filter settings.
    for name in ["Small.png", "Medium.png", "Fulllength.png"] {
        let image = image::open(second.join(name)).unwrap().to_rgba8();
        let file = fs::File::create(second.join(name)).unwrap();
        PngEncoder::new_with_quality(file, CompressionType::Fast, FilterType::NoFilter)
            .write_image(
                image.as_raw(),
                image.width(),
                image.height(),
                ColorType::Rgba8.into(),
            )
            .unwrap();
    }
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    import_portraits(
        &mut library,
        request(first, "first", DuplicatePolicy::Keep),
        &JobContext::default(),
    )
    .unwrap();
    import_portraits(
        &mut library,
        request(second.clone(), "second", DuplicatePolicy::Keep),
        &JobContext::default(),
    )
    .unwrap();
    assert_eq!(
        scan_duplicates(&library, &JobContext::default())
            .unwrap()
            .groups
            .len(),
        1
    );
    let second_id: String = library
        .connection()
        .query_row(
            "SELECT p.id FROM portraits p JOIN sources s ON s.id=p.source_id WHERE s.name='second'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    RgbaImage::from_pixel(185, 242, Rgba([99, 2, 3, 255]))
        .save(
            library
                .root()
                .join("portraits")
                .join(second_id)
                .join("Small.png"),
        )
        .unwrap();
    assert!(
        scan_duplicates(&library, &JobContext::default())
            .unwrap()
            .groups
            .is_empty()
    );
}

#[test]
fn managed_fingerprint_cache_makes_warm_scans_decode_free_and_invalidates_changed_assets() {
    let temp = tempfile::tempdir().unwrap();
    let first = support::write_portrait(temp.path(), "first");
    let second = support::write_portrait(temp.path(), "second");
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    import_portraits(
        &mut library,
        request(first, "first", DuplicatePolicy::Keep),
        &JobContext::default(),
    )
    .unwrap();
    import_portraits(
        &mut library,
        request(second, "second", DuplicatePolicy::Keep),
        &JobContext::default(),
    )
    .unwrap();
    assert_eq!(
        library
            .connection()
            .query_row("SELECT count(*) FROM portrait_fingerprints", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );

    // If the warm scan decoded assets it would repopulate this table. The managed set cache
    // has valid stamps, so this remains empty and the duplicate group still appears.
    library
        .connection()
        .execute("DELETE FROM pixel_fingerprints", [])
        .unwrap();
    assert_eq!(
        scan_duplicates(&library, &JobContext::default())
            .unwrap()
            .groups
            .len(),
        1
    );
    assert_eq!(
        library
            .connection()
            .query_row("SELECT count(*) FROM pixel_fingerprints", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );

    let id: String = library
        .connection()
        .query_row("SELECT id FROM portraits ORDER BY id LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    RgbaImage::from_pixel(185, 242, Rgba([1, 2, 3, 255]))
        .save(library.root().join("portraits").join(id).join("Small.png"))
        .unwrap();
    assert!(
        scan_duplicates(&library, &JobContext::default())
            .unwrap()
            .groups
            .is_empty()
    );
    assert!(
        library
            .connection()
            .query_row("SELECT count(*) FROM pixel_fingerprints", [], |r| r
                .get::<_, i64>(0))
            .unwrap()
            >= 1
    );
}

#[test]
fn skip_attaches_source_and_consolidation_unions_labels_then_trashes() {
    let temp = tempfile::tempdir().unwrap();
    let first = support::write_portrait(temp.path(), "female_elf");
    let second = support::write_portrait(temp.path(), "male_dwarf");
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    import_portraits(
        &mut library,
        request(first, "first", DuplicatePolicy::Keep),
        &JobContext::default(),
    )
    .unwrap();
    let report = import_portraits(
        &mut library,
        request(second.clone(), "second", DuplicatePolicy::Skip),
        &JobContext::default(),
    )
    .unwrap();
    assert_eq!((report.imported, report.skipped), (0, 1));
    assert_eq!(
        library
            .connection()
            .query_row(
                "SELECT count(*) FROM portraits WHERE trashed_at IS NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        library
            .connection()
            .query_row("SELECT count(*) FROM portrait_sources", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    let added_source = uuid::Uuid::parse_str(
        &library
            .connection()
            .query_row("SELECT id FROM sources WHERE name='second'", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        query_catalog(
            &library,
            &Query {
                source_ids: vec![added_source],
                ..Query::default()
            },
            Page {
                offset: 0,
                limit: 10
            },
        )
        .unwrap()
        .total,
        1
    );
    assert_eq!(
        query_catalog(
            &library,
            &Query {
                text: "second".into(),
                ..Query::default()
            },
            Page {
                offset: 0,
                limit: 10
            },
        )
        .unwrap()
        .total,
        1
    );
    let incoming = temp.path().join("incoming");
    support::write_portrait(&incoming, "copy-a");
    support::write_portrait(&incoming, "copy-b");
    let scan = scan_import_duplicates(
        &library,
        &request(incoming, "incoming", DuplicatePolicy::Skip),
        &JobContext::default(),
    )
    .unwrap();
    assert_eq!(scan.matches.len(), 2);
    assert!(
        scan.matches
            .iter()
            .any(|item| item.matching_portrait_id.is_some())
    );
    assert!(
        scan.matches
            .iter()
            .any(|item| item.duplicate_of_in_batch.is_some())
    );
    // Keep-mode makes a review group, then the chosen keeper retains the other source/labels.
    import_portraits(
        &mut library,
        request(second, "third", DuplicatePolicy::Keep),
        &JobContext::default(),
    )
    .unwrap();
    let group = scan_duplicates(&library, &JobContext::default())
        .unwrap()
        .groups
        .pop()
        .unwrap();
    let keeper = group.members[0].portrait.id;
    let remove = group.members[1].portrait.id;
    assert_eq!(
        consolidate_duplicates(
            &mut library,
            &[DuplicateConsolidation {
                keep_id: keeper,
                remove_ids: vec![remove]
            }]
        )
        .unwrap()
        .trashed,
        1
    );
    assert_eq!(
        library
            .connection()
            .query_row(
                "SELECT count(*) FROM portraits WHERE trashed_at IS NOT NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert!(
        library
            .connection()
            .query_row(
                "SELECT count(*) FROM portrait_sources WHERE portrait_id=?1",
                [keeper.to_string()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap()
            >= 2
    );
}
