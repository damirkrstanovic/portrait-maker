//! Measures catalog queries and first-view thumbnails on a disposable, read-only snapshot.
use portrait_core::{
    Library,
    catalog::{catalog_facets, query_catalog},
    thumbnails::thumbnail,
    types::{Page, Query, Role},
};
use std::{path::PathBuf, time::Instant};
fn main() {
    // Read the source through SQLite's snapshot API; never open it as a writable Library.
    let source = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .expect("usage: browse-probe LIBRARY"),
    )
    .canonicalize()
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path();
    let db = rusqlite::Connection::open_with_flags(
        source.join("library.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    db.backup(rusqlite::MAIN_DB, path.join("library.sqlite3"), None)
        .unwrap();
    std::fs::copy(source.join("library.json"), path.join("library.json")).unwrap();
    let mut statement=db.prepare("SELECT relative_path FROM assets WHERE portrait_id IN (SELECT id FROM portraits WHERE trashed_at IS NULL ORDER BY name,id LIMIT 24)").unwrap();
    for relative in statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
    {
        let relative = PathBuf::from(relative.unwrap());
        assert!(
            relative
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
        );
        let original = source.join(&relative).canonicalize().unwrap();
        assert!(original.starts_with(&source));
        let destination = path.join(relative);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(original, destination).unwrap();
    }
    let lib = Library::open(&path).unwrap();
    let t = Instant::now();
    let page = query_catalog(
        &lib,
        &Query::default(),
        Page {
            offset: 0,
            limit: 200,
        },
    )
    .unwrap();
    println!("catalog {} total / 200 page: {:?}", page.total, t.elapsed());
    let t = Instant::now();
    let facets = catalog_facets(&lib).unwrap();
    println!("facets {} sources: {:?}", facets.sources.len(), t.elapsed());
    for role in [Role::Small, Role::Medium, Role::Large] {
        for pass in ["cold", "warm"] {
            let t = Instant::now();
            for p in page.items.iter().take(24) {
                thumbnail(&lib, p.id, role, 360).unwrap();
            }
            println!("24 {role:?} {pass} thumbnails: {:?}", t.elapsed());
        }
    }
}
