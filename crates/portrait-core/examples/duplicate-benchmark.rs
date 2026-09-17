//! Small, reproducible timing probe; uses only generated portraits in a temporary library.
use image::{Rgba, RgbaImage};
use portrait_core::{
    Library,
    duplicate::{scan_duplicates, scan_import_duplicates},
    import::{JobContext, import_portraits},
    types::{DuplicatePolicy, ImportKind, ImportRequest},
};
use std::time::Instant;

fn main() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("input");
    let count = 12;
    for index in 0..count {
        let folder = source.join(format!("portrait-{index}"));
        std::fs::create_dir_all(&folder).unwrap();
        for (name, width, height) in [
            ("Small.png", 185, 242),
            ("Medium.png", 330, 432),
            ("Fulllength.png", 692, 1024),
        ] {
            RgbaImage::from_fn(width, height, |x, y| {
                Rgba([(x % 255) as u8, (y % 255) as u8, index as u8, 255])
            })
            .save(folder.join(name))
            .unwrap();
        }
    }
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    let started = Instant::now();
    let imported = import_portraits(
        &mut library,
        ImportRequest {
            path: source.clone(),
            source_name: "Timing fixture".into(),
            kind: ImportKind::Folder,
            resize: false,
            duplicate_policy: DuplicatePolicy::Keep,
        },
        &JobContext::default(),
    )
    .unwrap();
    println!(
        "import {} portraits: {:.3}s",
        imported.imported,
        started.elapsed().as_secs_f64()
    );
    for label in ["post-import scan", "cold scan", "warm scan"] {
        if label == "cold scan" {
            // This fixture is disposable. Simulate an existing library with no fingerprints.
            for table in ["portrait_fingerprints", "pixel_fingerprints"] {
                let exists: bool = library
                    .connection()
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                        [table],
                        |row| row.get(0),
                    )
                    .unwrap();
                if exists {
                    library
                        .connection()
                        .execute(&format!("DELETE FROM {table}"), [])
                        .unwrap();
                }
            }
        }
        let started = Instant::now();
        let report = scan_duplicates(&library, &JobContext::default()).unwrap();
        assert!(report.issues.is_empty());
        println!(
            "{label}: {:.3}s ({} groups)",
            started.elapsed().as_secs_f64(),
            report.groups.len()
        );
    }
    let request = ImportRequest {
        path: source,
        source_name: "Repeated pack".into(),
        kind: ImportKind::Folder,
        resize: false,
        duplicate_policy: DuplicatePolicy::Skip,
    };
    let started = Instant::now();
    let review = scan_import_duplicates(&library, &request, &JobContext::default()).unwrap();
    assert_eq!(review.matches.len(), count);
    println!(
        "import duplicate review: {:.3}s",
        started.elapsed().as_secs_f64()
    );
    let started = Instant::now();
    let skipped = import_portraits(&mut library, request, &JobContext::default()).unwrap();
    assert_eq!(skipped.skipped, count as u64);
    println!("skip after review: {:.3}s", started.elapsed().as_secs_f64());
}
