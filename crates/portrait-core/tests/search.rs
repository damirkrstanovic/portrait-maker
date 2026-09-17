mod support;

use portrait_core::catalog::{
    bump_catalog_revision, catalog_facets, compile_fts, query_catalog, rebuild_search_index,
    refresh_search_document, refresh_source_documents,
};
use portrait_core::import::{JobContext, import_portraits};
use portrait_core::types::{ImportKind, ImportRequest, Label, Page, Query};
use portrait_core::{CoreError, Library};
use rusqlite::{Transaction, params};
use uuid::Uuid;

fn add_source(transaction: &Transaction<'_>, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    transaction
        .execute(
            "INSERT INTO sources (id, name, kind) VALUES (?1, ?2, 'folder')",
            params![id.to_string(), name],
        )
        .unwrap();
    id
}

fn add_portrait(
    transaction: &Transaction<'_>,
    source_id: Uuid,
    name: &str,
    folder: &str,
    description: Option<&str>,
    trashed: bool,
) -> Uuid {
    let id = Uuid::new_v4();
    transaction
        .execute(
            "INSERT INTO portraits (id, source_id, name, original_folder, description, trashed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, CASE WHEN ?6 THEN '2026-09-16T12:00:00Z' END)",
            params![
                id.to_string(),
                source_id.to_string(),
                name,
                folder,
                description,
                trashed
            ],
        )
        .unwrap();
    id
}

fn add_label(transaction: &Transaction<'_>, portrait_id: Uuid, category: &str, value: &str) {
    transaction
        .execute(
            "INSERT OR IGNORE INTO labels (category, normalized_value, display_value) VALUES (?1, ?2, ?2)",
            params![category, value],
        )
        .unwrap();
    transaction
        .execute(
            "INSERT INTO portrait_labels (portrait_id, label_id, origin) \
             SELECT ?1, id, 'user' FROM labels WHERE category = ?2 AND normalized_value = ?3",
            params![portrait_id.to_string(), category, value],
        )
        .unwrap();
}

fn query(text: &str) -> Query {
    Query {
        text: text.into(),
        ..Query::default()
    }
}

fn first_page() -> Page {
    Page {
        offset: 0,
        limit: 50,
    }
}

#[test]
fn user_text_is_not_raw_match_syntax() {
    assert_eq!(
        compile_fts(" elf   archer ").as_deref(),
        Some("\"elf\" AND \"archer\"")
    );
    assert_eq!(compile_fts("\" OR * :").as_deref(), Some("\"OR\""));
    assert_eq!(compile_fts("   "), None);
}

#[test]
fn free_text_uses_unicode_diacritics_descriptions_and_literal_punctuation() {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::create(&temp.path().join("library")).unwrap();
    let transaction = library.connection().unchecked_transaction().unwrap();
    let source = add_source(&transaction, "Heroes: Deluxe");
    let id = add_portrait(
        &transaction,
        source,
        "Éowyn, Shield-Maiden",
        "women/rohan",
        Some("Fearless archer & scout"),
        false,
    );
    add_label(&transaction, id, "class", "ranger");
    refresh_search_document(&transaction, id).unwrap();
    bump_catalog_revision(&transaction).unwrap();
    transaction.commit().unwrap();

    for text in [
        "eowyn",
        "e\u{301}owyn",
        "shield maiden",
        "fearless scout",
        "heroes deluxe",
        "ranger",
    ] {
        let result = query_catalog(&library, &query(text), first_page()).unwrap();
        assert_eq!(
            result.items.iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![id]
        );
    }
    assert_eq!(
        query_catalog(&library, &query("\" OR * :"), first_page())
            .unwrap()
            .total,
        0
    );
}

#[test]
fn free_text_keeps_noncomposable_marks_and_unicode_scripts_in_word_tokens() {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::create(&temp.path().join("library")).unwrap();
    let transaction = library.connection().unchecked_transaction().unwrap();
    let source = add_source(&transaction, "Unicode Pack");
    let id = add_portrait(
        &transaction,
        source,
        "q\u{301}werty 東京 Дракон",
        "unicode",
        None,
        false,
    );
    refresh_search_document(&transaction, id).unwrap();
    transaction.commit().unwrap();

    assert_eq!(
        compile_fts("q\u{301}werty / 東京 — дракон").as_deref(),
        Some("\"q\u{301}werty\" AND \"東京\" AND \"дракон\"")
    );
    for text in ["q\u{301}werty", "東京", "дракон"] {
        let result = query_catalog(&library, &query(text), first_page()).unwrap();
        assert_eq!(
            result.items.iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![id]
        );
    }
}

#[test]
fn direct_filters_or_within_groups_and_and_across_groups_using_bound_input() {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::create(&temp.path().join("library")).unwrap();
    let transaction = library.connection().unchecked_transaction().unwrap();
    let source_a = add_source(&transaction, "A");
    let source_b = add_source(&transaction, "B");
    let elf_archer = add_portrait(&transaction, source_a, "Elf Archer", "elf", None, false);
    add_label(&transaction, elf_archer, "race", "elf");
    add_label(&transaction, elf_archer, "class", "ranger");
    let human_mage = add_portrait(&transaction, source_a, "Human Mage", "human", None, false);
    add_label(&transaction, human_mage, "race", "human");
    add_label(&transaction, human_mage, "class", "mage");
    let dwarf_archer = add_portrait(&transaction, source_b, "Dwarf Archer", "dwarf", None, false);
    add_label(&transaction, dwarf_archer, "race", "dwarf");
    add_label(&transaction, dwarf_archer, "class", "ranger");
    for id in [elf_archer, human_mage, dwarf_archer] {
        refresh_search_document(&transaction, id).unwrap();
    }
    transaction.commit().unwrap();

    let filtered = Query {
        text: "archer".into(),
        source_ids: vec![source_a],
        labels: vec![
            Label {
                category: "race".into(),
                value: "elf".into(),
            },
            Label {
                category: "race".into(),
                value: "human".into(),
            },
            Label {
                category: "class".into(),
                value: "ranger".into(),
            },
        ],
        selected_only: false,
        trash: false,
    };
    let result = query_catalog(&library, &filtered, first_page()).unwrap();
    assert_eq!(
        result.items.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![elf_archer]
    );

    let hostile = Query {
        labels: vec![Label {
            category: "race' OR 1=1 --".into(),
            value: "elf') OR 1=1 --".into(),
        }],
        ..Query::default()
    };
    assert_eq!(
        query_catalog(&library, &hostile, first_page())
            .unwrap()
            .total,
        0
    );
}

#[test]
fn label_facets_display_the_human_value_but_round_trip_the_normalized_query_key() {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::create(&temp.path().join("library")).unwrap();
    let transaction = library.connection().unchecked_transaction().unwrap();
    let source = add_source(&transaction, "Pack");
    let id = add_portrait(&transaction, source, "Elf Ranger", "elf", None, false);
    transaction
        .execute(
            "INSERT INTO labels (category, normalized_value, display_value) VALUES (?1, ?2, ?3)",
            params!["race", "elf", "Elf"],
        )
        .unwrap();
    transaction
        .execute(
            "INSERT INTO portrait_labels (portrait_id, label_id, origin) SELECT ?1, id, 'user' FROM labels WHERE category = ?2 AND normalized_value = ?3",
            params![id.to_string(), "race", "elf"],
        )
        .unwrap();
    refresh_search_document(&transaction, id).unwrap();
    transaction.commit().unwrap();

    let facet = catalog_facets(&library)
        .unwrap()
        .labels
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(
        (
            facet.category.as_str(),
            facet.value.as_str(),
            facet.display_value.as_str()
        ),
        ("race", "elf", "Elf")
    );
    let result = query_catalog(
        &library,
        &Query {
            labels: vec![Label {
                category: facet.category,
                value: facet.value,
            }],
            ..Query::default()
        },
        first_page(),
    )
    .unwrap();
    assert_eq!(
        result
            .items
            .iter()
            .map(|portrait| portrait.id)
            .collect::<Vec<_>>(),
        vec![id]
    );
}

#[test]
fn selected_and_trash_views_are_explicit_and_isolated() {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::create(&temp.path().join("library")).unwrap();
    let transaction = library.connection().unchecked_transaction().unwrap();
    let source = add_source(&transaction, "Pack");
    let active = add_portrait(&transaction, source, "Active", "active", None, false);
    let trashed = add_portrait(&transaction, source, "Trashed", "trashed", None, true);
    transaction
        .execute(
            "INSERT INTO selection (portrait_id) VALUES (?1)",
            [active.to_string()],
        )
        .unwrap();
    refresh_search_document(&transaction, active).unwrap();
    refresh_search_document(&transaction, trashed).unwrap();
    transaction.commit().unwrap();

    let active_page = query_catalog(&library, &Query::default(), first_page()).unwrap();
    assert_eq!(
        active_page
            .items
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![active]
    );
    assert!(active_page.items[0].selected);

    let selected = Query {
        selected_only: true,
        ..Query::default()
    };
    assert_eq!(
        query_catalog(&library, &selected, first_page())
            .unwrap()
            .total,
        1
    );

    let trash = Query {
        trash: true,
        ..Query::default()
    };
    let trash_page = query_catalog(&library, &trash, first_page()).unwrap();
    assert_eq!(
        trash_page
            .items
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        vec![trashed]
    );
    assert!(!trash_page.items[0].selected);
}

#[test]
fn source_refresh_and_full_rebuild_restore_search_documents() {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::create(&temp.path().join("library")).unwrap();
    let transaction = library.connection().unchecked_transaction().unwrap();
    let source = add_source(&transaction, "Old Pack");
    let id = add_portrait(&transaction, source, "Hero", "hero", None, false);
    refresh_search_document(&transaction, id).unwrap();
    transaction.commit().unwrap();
    assert_eq!(
        query_catalog(&library, &query("old"), first_page())
            .unwrap()
            .total,
        1
    );

    let transaction = library.connection().unchecked_transaction().unwrap();
    transaction
        .execute(
            "UPDATE sources SET name = ?1 WHERE id = ?2",
            params!["New Pack", source.to_string()],
        )
        .unwrap();
    assert_eq!(refresh_source_documents(&transaction, source).unwrap(), 1);
    bump_catalog_revision(&transaction).unwrap();
    transaction.commit().unwrap();
    assert_eq!(
        query_catalog(&library, &query("old"), first_page())
            .unwrap()
            .total,
        0
    );
    assert_eq!(
        query_catalog(&library, &query("new"), first_page())
            .unwrap()
            .items[0]
            .id,
        id
    );

    let transaction = library.connection().unchecked_transaction().unwrap();
    transaction
        .execute(
            "INSERT INTO search_index(search_index) VALUES('delete-all')",
            [],
        )
        .unwrap();
    transaction
        .execute("DELETE FROM search_documents", [])
        .unwrap();
    transaction.commit().unwrap();
    assert_eq!(
        query_catalog(&library, &query("hero"), first_page())
            .unwrap()
            .total,
        0
    );

    rebuild_search_index(&library).unwrap();
    assert_eq!(
        query_catalog(&library, &query("hero"), first_page())
            .unwrap()
            .items[0]
            .id,
        id
    );
}

#[test]
fn pages_are_deterministic_and_revision_changes_with_catalog_mutations() {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::create(&temp.path().join("library")).unwrap();
    let transaction = library.connection().unchecked_transaction().unwrap();
    let source = add_source(&transaction, "Pack");
    for _ in 0..5 {
        let id = add_portrait(&transaction, source, "Same Name", "same", None, false);
        refresh_search_document(&transaction, id).unwrap();
    }
    bump_catalog_revision(&transaction).unwrap();
    transaction.commit().unwrap();

    let first = query_catalog(
        &library,
        &Query::default(),
        Page {
            offset: 0,
            limit: 2,
        },
    )
    .unwrap();
    let second = query_catalog(
        &library,
        &Query::default(),
        Page {
            offset: 2,
            limit: 2,
        },
    )
    .unwrap();
    assert_eq!(first.total, 5);
    assert_eq!(second.total, 5);
    assert_eq!(first.revision, second.revision);
    assert!(first.items[0].id.to_string() < first.items[1].id.to_string());
    assert!(
        first
            .items
            .iter()
            .all(|item| !second.items.iter().any(|other| other.id == item.id))
    );

    let transaction = library.connection().unchecked_transaction().unwrap();
    bump_catalog_revision(&transaction).unwrap();
    transaction.commit().unwrap();
    let changed = query_catalog(&library, &Query::default(), first_page()).unwrap();
    assert_eq!(changed.revision, first.revision + 1);
}

#[test]
fn import_updates_the_search_index_in_the_same_commit() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    support::write_portrait(&input, "female_elf_archer");
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    import_portraits(
        &mut library,
        ImportRequest {
            path: input,
            source_name: "Curated Pack".into(),
            kind: ImportKind::Folder,
            resize: false,
            duplicate_policy: Default::default(),
        },
        &JobContext::default(),
    )
    .unwrap();

    let result = query_catalog(&library, &query("curated elf archer"), first_page()).unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].name, "female_elf_archer");
    assert!(result.items[0].labels.contains(&Label {
        category: "race".into(),
        value: "elf".into()
    }));
    assert!(result.revision > 0);
}

#[test]
fn invalid_page_sizes_are_rejected_by_the_core_api() {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::create(&temp.path().join("library")).unwrap();
    for limit in [0, 201] {
        let error =
            query_catalog(&library, &Query::default(), Page { offset: 0, limit }).unwrap_err();
        assert!(matches!(error, CoreError::InvalidPagination));
    }
}
