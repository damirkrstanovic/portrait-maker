mod support;

use portrait_core::Library;
use portrait_core::analysis::{PortraitAnalysis, analysis_candidates, save_analysis};
use portrait_core::catalog::query_catalog;
use portrait_core::metadata::{MetadataPatch, edit_metadata};
use portrait_core::selection::{SelectionAction, change_selection};
use portrait_core::types::{Label, Page, Query, SelectionTarget};

fn result(library: &Library, text: &str) -> Vec<portrait_core::types::Portrait> {
    query_catalog(
        library,
        &Query {
            text: text.into(),
            ..Query::default()
        },
        Page {
            offset: 0,
            limit: 20,
        },
    )
    .unwrap()
    .items
}

fn analysis(description: &str, labels: Vec<Label>) -> PortraitAnalysis {
    PortraitAnalysis {
        description: description.into(),
        labels,
    }
}

#[test]
fn analysis_is_searchable_and_exposed_without_replacing_user_description() {
    let temp = tempfile::tempdir().unwrap();
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    let id = support::seed_catalog(&mut library, 1)[0];
    edit_metadata(
        &mut library,
        &[id],
        MetadataPatch {
            description: Some(Some("A note the collector wrote".into())),
            ..MetadataPatch::default()
        },
    )
    .unwrap();

    save_analysis(
        &mut library,
        id,
        &analysis(
            "A weathered elf in silver armor holds a glowing bow.",
            vec![
                Label {
                    category: "race".into(),
                    value: "elf".into(),
                },
                Label {
                    category: "combat".into(),
                    value: "archer".into(),
                },
            ],
        ),
        "gemma-4-26b-a4b",
    )
    .unwrap();

    let portrait = result(&library, "weathered glowing bow").pop().unwrap();
    assert_eq!(portrait.id, id);
    assert_eq!(
        portrait.description.as_deref(),
        Some("A note the collector wrote")
    );
    assert_eq!(
        portrait.model_description.as_deref(),
        Some("A weathered elf in silver armor holds a glowing bow.")
    );
    assert!(portrait.labels.contains(&Label {
        category: "race".into(),
        value: "elf".into(),
    }));
}

#[test]
fn rerun_replaces_only_model_labels_and_respects_user_suppression() {
    let temp = tempfile::tempdir().unwrap();
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    let id = support::seed_catalog(&mut library, 1)[0];
    edit_metadata(
        &mut library,
        &[id],
        MetadataPatch {
            add_labels: vec![Label {
                category: "class".into(),
                value: "ranger".into(),
            }],
            remove_labels: vec![Label {
                category: "race".into(),
                value: "elf".into(),
            }],
            ..MetadataPatch::default()
        },
    )
    .unwrap();
    save_analysis(
        &mut library,
        id,
        &analysis(
            "First pass",
            vec![
                Label {
                    category: "race".into(),
                    value: "elf".into(),
                },
                Label {
                    category: "combat".into(),
                    value: "archer".into(),
                },
            ],
        ),
        "model-a",
    )
    .unwrap();
    save_analysis(
        &mut library,
        id,
        &analysis(
            "Second pass",
            vec![
                Label {
                    category: "magic".into(),
                    value: "arcane".into(),
                },
                Label {
                    category: "class".into(),
                    value: "wizard".into(),
                },
            ],
        ),
        "model-b",
    )
    .unwrap();

    let portrait = result(&library, "second").pop().unwrap();
    assert_eq!(portrait.labels.len(), 2);
    assert!(portrait.labels.contains(&Label {
        category: "class".into(),
        value: "ranger".into()
    }));
    assert!(!portrait.labels.contains(&Label {
        category: "class".into(),
        value: "wizard".into()
    }));
    assert!(portrait.labels.contains(&Label {
        category: "magic".into(),
        value: "arcane".into()
    }));
    assert!(
        !portrait
            .labels
            .iter()
            .any(|label| label.value == "elf" || label.value == "archer")
    );
    let provenance: (String, String, String) = library
        .connection()
        .query_row(
            "SELECT origin, producer, producer_version FROM portrait_labels pl JOIN labels l ON l.id = pl.label_id WHERE pl.portrait_id = ?1 AND l.category = 'magic'",
            [id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(provenance, ("model".into(), "model-b".into(), "2".into()));
}

#[test]
fn candidates_honor_selection_overwrite_and_trash() {
    let temp = tempfile::tempdir().unwrap();
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    let ids = support::seed_catalog(&mut library, 3);
    save_analysis(
        &mut library,
        ids[0],
        &analysis("Already done", Vec::new()),
        "model",
    )
    .unwrap();
    change_selection(
        &mut library,
        SelectionTarget::Ids(vec![ids[1]]),
        SelectionAction::Add,
    )
    .unwrap();
    library
        .connection()
        .execute(
            "UPDATE portraits SET trashed_at = '2026-01-01T00:00:00Z' WHERE id = ?1",
            [ids[2].to_string()],
        )
        .unwrap();

    assert_eq!(
        analysis_candidates(&library, false, false).unwrap(),
        vec![ids[1]]
    );
    assert_eq!(
        analysis_candidates(&library, true, false).unwrap(),
        vec![ids[1]]
    );
    assert_eq!(
        analysis_candidates(&library, false, true).unwrap(),
        vec![ids[0], ids[1]]
    );
}

#[test]
fn freeform_labels_and_long_descriptions_are_saved_and_searchable() {
    let temp = tempfile::tempdir().unwrap();
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    let id = support::seed_catalog(&mut library, 1)[0];
    let description = format!("{} moonlit observatory", "Detailed portrait. ".repeat(350));
    let mut labels = (0..25)
        .map(|n| Label {
            category: "detail".into(),
            value: format!("detail {n}"),
        })
        .collect::<Vec<_>>();
    labels.extend([
        Label {
            category: " Species ".into(),
            value: " Dragon ".into(),
        },
        Label {
            category: "weapon".into(),
            value: "longsword".into(),
        },
        Label {
            category: "".into(),
            value: "".into(),
        },
    ]);
    save_analysis(&mut library, id, &analysis(&description, labels), "model").unwrap();
    let portrait = result(&library, "observatory dragon longsword")
        .pop()
        .unwrap();
    assert_eq!(
        portrait.model_description.as_deref(),
        Some(description.as_str())
    );
    assert_eq!(portrait.labels.len(), 27);
    assert!(portrait.labels.contains(&Label {
        category: "species".into(),
        value: "dragon".into()
    }));
}
