mod support;
use portrait_core::{
    Library,
    backup::{backup_library, backup_library_confirmed, restore_library},
    import::{JobContext, import_portraits},
    types::{ImportKind, ImportRequest},
};
use std::{fs, path::Path};

fn fixture(root: &Path) -> Library {
    let input = root.join("input");
    support::write_portrait_with_dimensions(&input, "elf", (4, 5), (5, 6), (6, 7));
    support::write_portrait_with_dimensions(&input, "dwarf", (4, 5), (5, 6), (6, 7));
    let mut lib = Library::create(&root.join("library")).unwrap();
    import_portraits(
        &mut lib,
        ImportRequest {
            path: input,
            source_name: "Original pack".into(),
            kind: ImportKind::Folder,
            resize: false,
            duplicate_policy: Default::default(),
        },
        &JobContext::default(),
    )
    .unwrap();
    lib.connection().execute_batch("UPDATE sources SET name='Edited pack'; UPDATE portraits SET name='Edited name', description='Personal description'; INSERT INTO selection(portrait_id) SELECT id FROM portraits ORDER BY id LIMIT 1; UPDATE portraits SET trashed_at='2026-09-16T00:00:00Z' WHERE id NOT IN (SELECT portrait_id FROM selection); INSERT INTO labels(category,normalized_value,display_value) VALUES('mood','calm','Calm'); INSERT INTO portrait_labels(portrait_id,label_id,origin) SELECT id,(SELECT max(id) FROM labels),'user' FROM portraits; INSERT INTO user_label_suppressions(portrait_id,category,normalized_value) SELECT id,'race','elf' FROM portraits; INSERT INTO operation_state VALUES('receipt','export_receipt','{}','now');").unwrap();
    lib
}
fn rows(lib: &Library, table: &str) -> Vec<String> {
    let mut stmt = lib
        .connection()
        .prepare(&format!("SELECT * FROM {table} ORDER BY 1,2"))
        .unwrap();
    let n = stmt.column_count();
    stmt.query_map([], |row| {
        Ok((0..n)
            .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
            .collect::<Vec<_>>()
            .join("|"))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}
#[test]
fn complete_roundtrip_and_focused_safety_checks() {
    let temp = tempfile::tempdir().unwrap();
    let lib = fixture(temp.path());
    let zip = temp.path().join("backup.zip");
    backup_library(&lib, &zip, &JobContext::default()).unwrap();
    let restored =
        restore_library(&zip, &temp.path().join("restored"), &JobContext::default()).unwrap();
    assert_eq!(lib.id(), restored.id());
    for table in [
        "sources",
        "portraits",
        "assets",
        "labels",
        "portrait_labels",
        "user_label_suppressions",
        "selection",
    ] {
        assert_eq!(rows(&lib, table), rows(&restored, table), "{table}");
    }
    let mut stmt = lib
        .connection()
        .prepare("SELECT relative_path FROM assets")
        .unwrap();
    for path in stmt.query_map([], |r| r.get::<_, String>(0)).unwrap() {
        let p = path.unwrap();
        assert_eq!(
            fs::read(lib.root().join(&p)).unwrap(),
            fs::read(restored.root().join(p)).unwrap()
        );
    }
    assert_eq!(
        restored
            .connection()
            .query_row(
                "SELECT count(*) FROM operation_state WHERE kind='export_receipt'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert!(restore_library(&zip, restored.root(), &JobContext::default()).is_err());
    let original = fs::read(&zip).unwrap();
    assert!(backup_library(&lib, &zip, &JobContext::default()).is_err());
    assert_eq!(fs::read(&zip).unwrap(), original);
    let cancelled = JobContext::default();
    cancelled.cancel();
    assert!(backup_library_confirmed(&lib, &zip, &cancelled, true).is_err());
    assert_eq!(fs::read(&zip).unwrap(), original);
    assert!(restore_library(&zip, &temp.path().join("cancelled"), &cancelled).is_err());
    assert!(!temp.path().join("cancelled").exists());
    backup_library_confirmed(&lib, &zip, &JobContext::default(), true).unwrap();
    lib.connection()
        .execute(
            "INSERT INTO operation_state VALUES('orphan','export_delta','{}','now')",
            [],
        )
        .unwrap();
    assert!(
        backup_library(
            &lib,
            &temp.path().join("blocked.zip"),
            &JobContext::default()
        )
        .is_err()
    );
}
#[test]
fn malformed_backup_never_promotes() {
    use std::io::{Read, Write};
    let temp = tempfile::tempdir().unwrap();
    let lib = fixture(temp.path());
    let good = temp.path().join("good.zip");
    backup_library(&lib, &good, &JobContext::default()).unwrap();
    for mode in ["missing", "corrupt", "traversal", "newer", "external"] {
        let bad = temp.path().join(format!("{mode}.zip"));
        let mut source = zip::ZipArchive::new(fs::File::open(&good).unwrap()).unwrap();
        let mut out = zip::ZipWriter::new(fs::File::create(&bad).unwrap());
        let mut changed = false;
        for i in 0..source.len() {
            let mut file = source.by_index(i).unwrap();
            let name = file.name().to_string();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            if name.starts_with("portraits/") && !changed {
                changed = true;
                if mode == "missing" {
                    continue;
                }
                if mode == "corrupt" {
                    bytes = b"bad PNG".to_vec();
                }
            }
            if name == "library.json" && mode == "newer" {
                let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                v["formatVersion"] = 999.into();
                bytes = serde_json::to_vec(&v).unwrap();
            }
            if name == "library.sqlite3" && mode == "external" {
                let db = temp.path().join("tampered.sqlite3");
                fs::write(&db, &bytes).unwrap();
                let conn = rusqlite::Connection::open(&db).unwrap();
                conn.execute(
                    "INSERT INTO operation_state VALUES('orphan','export_delta','{}','now')",
                    [],
                )
                .unwrap();
                drop(conn);
                bytes = fs::read(db).unwrap();
            }
            out.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            out.write_all(&bytes).unwrap();
        }
        if mode == "traversal" {
            out.start_file("../escape", zip::write::SimpleFileOptions::default())
                .unwrap();
            out.write_all(b"escape").unwrap();
        }
        out.finish().unwrap();
        let target = temp.path().join(format!("restore-{mode}"));
        assert!(
            restore_library(&bad, &target, &JobContext::default()).is_err(),
            "{mode}"
        );
        assert!(!target.exists(), "{mode}");
    }
}
