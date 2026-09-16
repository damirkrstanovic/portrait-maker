mod support;

use portrait_core::Library;
use portrait_core::catalog::query_catalog;
use portrait_core::selection::{SelectionAction, SelectionTarget, change_selection};
use portrait_core::types::{Page, Query};

#[test]
fn select_matching_is_not_limited_to_one_page() {
    let temp = tempfile::tempdir().unwrap();
    let mut library = Library::create(&temp.path().join("library")).unwrap();
    support::seed_catalog(&mut library, 1_500);

    let count = change_selection(
        &mut library,
        SelectionTarget::Matching(Query::default()),
        SelectionAction::Add,
    )
    .unwrap();

    assert_eq!(count, 1_500);
    assert_eq!(
        query_catalog(
            &library,
            &Query::default(),
            Page {
                offset: 0,
                limit: 100,
            },
        )
        .unwrap()
        .items
        .len(),
        100
    );
}

#[test]
fn remove_matching_uses_the_query_snapshot_and_clear_persists_after_restart() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let mut library = Library::create(&root).unwrap();
    let ids = support::seed_catalog(&mut library, 3);

    assert_eq!(
        change_selection(
            &mut library,
            SelectionTarget::Ids(ids.clone()),
            SelectionAction::Add
        )
        .unwrap(),
        3
    );
    assert_eq!(
        change_selection(
            &mut library,
            SelectionTarget::Matching(Query {
                text: "0001".into(),
                ..Query::default()
            }),
            SelectionAction::Remove,
        )
        .unwrap(),
        2
    );
    drop(library);

    let mut reopened = Library::open(&root).unwrap();
    assert_eq!(
        query_catalog(
            &reopened,
            &Query {
                selected_only: true,
                ..Query::default()
            },
            Page {
                offset: 0,
                limit: 10
            },
        )
        .unwrap()
        .total,
        2
    );
    assert_eq!(
        change_selection(
            &mut reopened,
            SelectionTarget::Ids(Vec::new()),
            SelectionAction::Clear,
        )
        .unwrap(),
        0
    );
}
