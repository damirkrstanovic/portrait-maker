mod support;

use portrait_core::Library;
use portrait_core::catalog::query_catalog;
use portrait_core::metadata::{MetadataPatch, apply_inferred_labels, edit_metadata, rename_source};
use portrait_core::types::{Label, Page, Query};
use rusqlite::params;

fn page(library: &Library, query: Query) -> Vec<portrait_core::types::Portrait> {
    query_catalog(
        library,
        &query,
        Page {
            offset: 0,
            limit: 20,
        },
    )
    .unwrap()
    .items
}

#[test]
fn edits_metadata_reindexes_search_and_keeps_user_labels_over_inference() {
    let temp = tempfile::tempdir().unwrap();
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    let ids = support::seed_catalog(&mut library, 1);
    let id = ids[0];

    edit_metadata(
        &mut library,
        &[id],
        MetadataPatch {
            name: Some("  Scout of Kenabres  ".into()),
            description: Some(Some("  Watches the wardstone.  ".into())),
            add_labels: vec![Label {
                category: "role".into(),
                value: "  Watcher  ".into(),
            }],
            remove_labels: Vec::new(),
        },
    )
    .unwrap();
    apply_inferred_labels(
        &mut library,
        id,
        &[Label {
            category: "role".into(),
            value: "watcher".into(),
        }],
        "path-vocabulary",
        "1",
    )
    .unwrap();

    let result = page(
        &library,
        Query {
            text: "wardstone watcher".into(),
            ..Query::default()
        },
    );
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "Scout of Kenabres");
    assert_eq!(
        result[0].description.as_deref(),
        Some("Watches the wardstone.")
    );
    assert_eq!(
        result[0].labels,
        vec![Label {
            category: "role".into(),
            value: "watcher".into()
        }]
    );
    let origin: String = library
        .connection()
        .query_row(
            "SELECT origin FROM portrait_labels WHERE portrait_id = ?1",
            [id.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(origin, "user");
}

#[test]
fn removing_an_inferred_label_suppresses_a_repeat_inference_and_source_rename_persists() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let mut library = Library::create(&root).unwrap();
    let id = support::seed_catalog(&mut library, 1)[0];
    let source_id: String = library
        .connection()
        .query_row(
            "SELECT source_id FROM portraits WHERE id = ?1",
            [id.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    let source_id = uuid::Uuid::parse_str(&source_id).unwrap();
    apply_inferred_labels(
        &mut library,
        id,
        &[Label {
            category: "race".into(),
            value: "elf".into(),
        }],
        "path-vocabulary",
        "1",
    )
    .unwrap();

    edit_metadata(
        &mut library,
        &[id],
        MetadataPatch {
            name: None,
            description: None,
            add_labels: Vec::new(),
            remove_labels: vec![Label {
                category: "race".into(),
                value: "elf".into(),
            }],
        },
    )
    .unwrap();
    apply_inferred_labels(
        &mut library,
        id,
        &[Label {
            category: "race".into(),
            value: "elf".into(),
        }],
        "path-vocabulary",
        "1",
    )
    .unwrap();
    apply_inferred_labels(
        &mut library,
        id,
        &[Label {
            category: "race".into(),
            value: "elf".into(),
        }],
        "path-vocabulary",
        "2",
    )
    .unwrap();
    apply_inferred_labels(
        &mut library,
        id,
        &[Label {
            category: "race".into(),
            value: "elf".into(),
        }],
        "model-classifier",
        "2026.09",
    )
    .unwrap();
    rename_source(&mut library, source_id, "  WotR companions  ").unwrap();
    drop(library);

    let reopened = Library::open(&root).unwrap();
    assert!(
        page(
            &reopened,
            Query {
                text: "elf".into(),
                ..Query::default()
            }
        )
        .is_empty()
    );
    assert_eq!(
        page(
            &reopened,
            Query {
                text: "companions".into(),
                ..Query::default()
            }
        )[0]
        .source_name,
        "WotR companions"
    );
    assert_eq!(reopened.connection().query_row(
        "SELECT count(*) FROM suppressed_inferred_labels WHERE portrait_id = ?1 AND category = 'race' AND normalized_value = 'elf'",
        params![id.to_string()], |row| row.get::<_, i64>(0),
    ).unwrap(), 1);
    assert_eq!(reopened.connection().query_row(
        "SELECT count(*) FROM user_label_suppressions WHERE portrait_id = ?1 AND category = 'race' AND normalized_value = 'elf'",
        params![id.to_string()], |row| row.get::<_, i64>(0),
    ).unwrap(), 1);
}

#[test]
fn rejects_blank_metadata_and_multi_portrait_renames_without_partial_changes() {
    let temp = tempfile::tempdir().unwrap();
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    let ids = support::seed_catalog(&mut library, 2);

    assert_eq!(
        edit_metadata(
            &mut library,
            &ids,
            MetadataPatch {
                name: Some("Both portraits".into()),
                ..MetadataPatch::default()
            },
        )
        .unwrap_err()
        .code(),
        "METADATA_NAME_REQUIRES_SINGLE_PORTRAIT"
    );
    assert_eq!(
        edit_metadata(
            &mut library,
            &[ids[0]],
            MetadataPatch {
                name: Some("   ".into()),
                add_labels: vec![Label {
                    category: "role".into(),
                    value: "   ".into(),
                }],
                ..MetadataPatch::default()
            },
        )
        .unwrap_err()
        .code(),
        "METADATA_NAME_INVALID"
    );
    assert_eq!(
        page(&library, Query::default())[0].name,
        "Generated portrait 0000"
    );
}
