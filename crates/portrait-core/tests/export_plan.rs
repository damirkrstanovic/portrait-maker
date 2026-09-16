mod support;
use portrait_core::{
    Library,
    export::{ExportHistory, plan_export, stable_folder_name},
    types::*,
};
use std::fs;
fn request(target: std::path::PathBuf, mode: ExportMode) -> ExportRequest {
    ExportRequest {
        scope: ExportScope::All,
        target,
        output: ExportOutput::Directory,
        mode,
    }
}
fn fixture() -> (tempfile::TempDir, Library, Vec<uuid::Uuid>) {
    let t = tempfile::tempdir().unwrap();
    let mut l = Library::create(&t.path().join("library")).unwrap();
    let ids = support::seed_catalog(&mut l, 3);
    (t, l, ids)
}
#[test]
fn empty_selection_never_means_export_everything() {
    let (t, l, _) = fixture();
    let mut r = request(t.path().join("out"), ExportMode::Merge);
    r.scope = ExportScope::Selected;
    assert_eq!(
        plan_export(&l, r, &ExportHistory::default())
            .unwrap_err()
            .code(),
        "EMPTY_SELECTION"
    );
}
#[test]
fn freezes_active_ids_stable_names_and_registry_is_one_use() {
    let (t, l, ids) = fixture();
    l.connection()
        .execute(
            "UPDATE portraits SET trashed_at='now' WHERE id=?",
            [ids[0].to_string()],
        )
        .unwrap();
    let p = plan_export(
        &l,
        request(t.path().join("out"), ExportMode::Merge),
        &ExportHistory::default(),
    )
    .unwrap();
    assert_eq!(p.portrait_count, 2);
    assert_eq!(p.actions.len(), 6);
    assert!(p.actions.iter().all(|a| a.kind == ExportActionKind::Add));
    assert!(p.actions.iter().any(|a| {
        a.path
            .to_string_lossy()
            .contains(&stable_folder_name(l.id(), ids[1]))
    }));
    let stored = l.export_plan(p.id).unwrap();
    assert!(!stored.portrait_ids.contains(&ids[0]));
    assert_eq!(
        l.consume_export_plan(p.id, false).unwrap().portrait_ids,
        stored.portrait_ids
    );
    assert_eq!(
        l.consume_export_plan(p.id, false).unwrap_err().code(),
        "EXPORT_PLAN_NOT_FOUND"
    );
    assert!(!t.path().join("out").exists());
}
#[test]
fn merge_preserves_and_replace_removes_only_complete_clean_portrait_folders() {
    let (t, l, _) = fixture();
    let out = t.path().join("collection");
    support::write_portrait(&out, "old");
    support::write_portrait(&out, "notes");
    fs::write(out.join("notes/notes.txt"), "keep").unwrap();
    fs::create_dir(out.join("old/nested")).unwrap();
    support::write_portrait(&out, "remove-me");
    fs::write(out.join("rootnotes.txt"), "keep").unwrap();
    let merge = plan_export(
        &l,
        request(out.clone(), ExportMode::Merge),
        &ExportHistory::default(),
    )
    .unwrap();
    assert!(
        !merge
            .actions
            .iter()
            .any(|a| a.kind == ExportActionKind::Remove)
    );
    assert_eq!(
        plan_export(
            &l,
            request(out.clone(), ExportMode::Replace),
            &ExportHistory::default()
        )
        .unwrap_err()
        .code(),
        "EXPORT_TARGET_NOT_DESIGNATED"
    );
    l.designate_export_target(&out).unwrap();
    let p = plan_export(
        &l,
        request(out.clone(), ExportMode::Replace),
        &ExportHistory::default(),
    )
    .unwrap();
    assert!(p.requires_confirmation);
    let removed: Vec<_> = p
        .actions
        .iter()
        .filter(|a| a.kind == ExportActionKind::Remove)
        .collect();
    assert_eq!(removed.len(), 3);
    assert!(
        removed
            .iter()
            .all(|a| a.path.starts_with(out.join("remove-me")))
    );
    assert!(
        p.actions
            .iter()
            .any(|a| a.path == out.join("notes/notes.txt") && a.kind == ExportActionKind::Preserve)
    );
    assert!(!p.warnings.is_empty());
    assert_eq!(
        l.consume_export_plan(p.id, false).unwrap_err().code(),
        "EXPORT_CONFIRMATION_REQUIRED"
    );
    assert!(l.consume_export_plan(p.id, true).is_ok());
    assert!(out.join("remove-me/Small.png").exists());
}
#[test]
fn rejects_overlap_roots_and_broad_installation_directories() {
    let (t, l, _) = fixture();
    for path in [
        l.root().to_path_buf(),
        l.root().join("child"),
        t.path().to_path_buf(),
        "/".into(),
    ] {
        assert_eq!(
            plan_export(
                &l,
                request(path, ExportMode::Merge),
                &ExportHistory::default()
            )
            .unwrap_err()
            .code(),
            "EXPORT_TARGET_UNSAFE"
        );
    }
    let install = t.path().join("game-install");
    fs::create_dir(&install).unwrap();
    fs::write(install.join("GameAssembly.dll"), "fixture").unwrap();
    assert_eq!(
        l.designate_export_target(&install).unwrap_err().code(),
        "EXPORT_TARGET_UNSAFE"
    );
}
#[cfg(unix)]
#[test]
fn resolves_symlinks_before_overlap_and_rejects_linked_collision() {
    use std::os::unix::fs::symlink;
    let (t, l, ids) = fixture();
    symlink(l.root(), t.path().join("alias")).unwrap();
    assert_eq!(
        plan_export(
            &l,
            request(t.path().join("alias/new"), ExportMode::Merge),
            &ExportHistory::default()
        )
        .unwrap_err()
        .code(),
        "EXPORT_TARGET_UNSAFE"
    );
    let out = t.path().join("out");
    fs::create_dir(&out).unwrap();
    symlink(l.root(), out.join(stable_folder_name(l.id(), ids[0]))).unwrap();
    assert_eq!(
        plan_export(
            &l,
            request(out, ExportMode::Merge),
            &ExportHistory::default()
        )
        .unwrap_err()
        .code(),
        "EXPORT_TARGET_UNSAFE"
    );
}
#[test]
fn unknown_collision_requires_confirmation_and_content_changes_invalidate_plan() {
    let (t, l, ids) = fixture();
    let out = t.path().join("out");
    let set = support::write_portrait(&out, &stable_folder_name(l.id(), ids[0]));
    let p = plan_export(
        &l,
        request(out.clone(), ExportMode::Merge),
        &ExportHistory::default(),
    )
    .unwrap();
    assert!(p.requires_confirmation);
    assert_eq!(
        p.actions
            .iter()
            .filter(|a| a.kind == ExportActionKind::Overwrite)
            .count(),
        3
    );
    let old = fs::read(set.join("Small.png")).unwrap();
    let mut changed = old.clone();
    changed[20] ^= 1;
    fs::write(set.join("Small.png"), changed).unwrap();
    assert_eq!(
        l.validate_export_plan(p.id).unwrap_err().code(),
        "EXPORT_PLAN_STALE"
    );
    assert_eq!(
        l.export_plan(p.id).unwrap_err().code(),
        "EXPORT_PLAN_NOT_FOUND"
    );
}
#[test]
fn catalog_change_invalidates_plan_and_selected_scope_ignores_trash() {
    let (t, l, ids) = fixture();
    for id in &ids {
        l.connection()
            .execute(
                "INSERT INTO selection(portrait_id) VALUES (?)",
                [id.to_string()],
            )
            .unwrap();
    }
    l.connection()
        .execute(
            "UPDATE portraits SET trashed_at='now' WHERE id=?",
            [ids[0].to_string()],
        )
        .unwrap();
    let mut r = request(t.path().join("out"), ExportMode::Merge);
    r.scope = ExportScope::Selected;
    let p = plan_export(&l, r, &ExportHistory::default()).unwrap();
    assert_eq!(p.portrait_count, 2);
    let tx = l.connection().unchecked_transaction().unwrap();
    portrait_core::catalog::bump_catalog_revision(&tx).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        l.validate_export_plan(p.id).unwrap_err().code(),
        "EXPORT_PLAN_STALE"
    );
}
#[test]
fn zip_plan_lists_entries_and_existing_zip_overwrite() {
    let (t, l, _) = fixture();
    let mut r = request(t.path().join("portraits.zip"), ExportMode::Merge);
    r.output = ExportOutput::Zip;
    let p = plan_export(&l, r.clone(), &ExportHistory::default()).unwrap();
    assert_eq!(
        p.actions
            .iter()
            .filter(|a| a.kind == ExportActionKind::Add)
            .count(),
        10
    );
    assert!(!p.requires_confirmation);
    fs::write(&r.target, "old zip").unwrap();
    let p = plan_export(&l, r, &ExportHistory::default()).unwrap();
    assert!(p.requires_confirmation);
}

#[test]
fn provenance_is_bound_to_library_and_destination_ids_and_path_but_every_overwrite_confirms() {
    use portrait_core::export::plan_export_for_destination;
    use std::collections::BTreeMap;
    let (t, l, ids) = fixture();
    let out = t.path().join("out");
    let destination_id = uuid::Uuid::new_v4();
    let folder = stable_folder_name(l.id(), ids[0]);
    let set = support::write_portrait(&out, &folder);
    let mut history = ExportHistory {
        library_id: Some(l.id()),
        destination_id: Some(destination_id),
        destination: Some(out.clone()),
        file_digests: BTreeMap::new(),
    };
    for name in ["Small.png", "Medium.png", "Fulllength.png"] {
        history.file_digests.insert(
            std::path::PathBuf::from(&folder).join(name),
            portrait_core::export::file_digest(&set.join(name)).unwrap(),
        );
    }
    let preview = |history: &ExportHistory, expected_id| {
        plan_export_for_destination(
            &l,
            request(out.clone(), ExportMode::Merge),
            expected_id,
            history,
        )
        .unwrap()
    };
    let owned = preview(&history, destination_id);
    assert!(owned.requires_confirmation);
    assert!(owned.warnings.is_empty());
    assert_eq!(
        owned
            .actions
            .iter()
            .filter(|a| a.kind == ExportActionKind::Overwrite)
            .count(),
        3
    );
    assert_eq!(
        l.export_plan(owned.id).unwrap().destination_id,
        Some(destination_id)
    );
    assert_eq!(
        l.consume_export_plan(owned.id, false).unwrap_err().code(),
        "EXPORT_CONFIRMATION_REQUIRED"
    );
    assert!(l.consume_export_plan(owned.id, true).is_ok());

    let recreated = preview(&history, uuid::Uuid::new_v4());
    assert!(recreated.requires_confirmation);
    assert_eq!(
        recreated.warnings.len(),
        3,
        "a recreated saved destination cannot inherit old ownership"
    );
    history.library_id = Some(uuid::Uuid::new_v4());
    assert_eq!(preview(&history, destination_id).warnings.len(), 3);
    history.library_id = Some(l.id());
    history.destination = Some(t.path().join("different"));
    assert_eq!(preview(&history, destination_id).warnings.len(), 3);
    history.destination = Some(out.clone());
    history.destination_id = None;
    assert_eq!(preview(&history, destination_id).warnings.len(), 3);
    history.destination_id = Some(destination_id);
    let unrouted = plan_export(&l, request(out.clone(), ExportMode::Merge), &history).unwrap();
    assert!(unrouted.requires_confirmation);
    assert_eq!(
        unrouted.warnings.len(),
        3,
        "history cannot supply its own current routing identity"
    );
    assert_eq!(l.export_plan(unrouted.id).unwrap().destination_id, None);
    fs::write(set.join("Small.png"), "changed").unwrap();
    let modified = preview(&history, destination_id);
    assert!(modified.requires_confirmation);
    assert_eq!(modified.warnings.len(), 1);
    assert!(modified.warnings[0].contains("Small.png"));

    let value = serde_json::to_value(&history).unwrap();
    assert_eq!(value["destinationId"], destination_id.to_string());
    let decoded: ExportHistory = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
}
#[cfg(unix)]
#[test]
fn retargeting_alias_or_recreating_destination_retires_preview() {
    use std::os::unix::fs::symlink;
    let (t, l, _) = fixture();
    let out = t.path().join("out");
    let other = t.path().join("other");
    fs::create_dir(&out).unwrap();
    fs::create_dir(&other).unwrap();
    let alias = t.path().join("alias");
    symlink(&out, &alias).unwrap();
    let p = plan_export(
        &l,
        request(alias.clone(), ExportMode::Merge),
        &ExportHistory::default(),
    )
    .unwrap();
    fs::remove_file(&alias).unwrap();
    symlink(&other, &alias).unwrap();
    assert_eq!(
        l.validate_export_plan(p.id).unwrap_err().code(),
        "EXPORT_PLAN_STALE"
    );
    let p = plan_export(
        &l,
        request(out.clone(), ExportMode::Merge),
        &ExportHistory::default(),
    )
    .unwrap();
    fs::rename(&out, t.path().join("old-out")).unwrap();
    fs::create_dir(&out).unwrap();
    assert_eq!(
        l.validate_export_plan(p.id).unwrap_err().code(),
        "EXPORT_PLAN_STALE"
    );
}
#[test]
fn portable_case_collisions_are_rejected_without_ambiguous_actions() {
    let (t, l, ids) = fixture();
    let out = t.path().join("out");
    let folder = stable_folder_name(l.id(), ids[0]);
    let set = support::write_portrait(&out, &folder);
    fs::rename(set.join("Small.png"), set.join("small.png")).unwrap();
    assert_eq!(
        plan_export(
            &l,
            request(out, ExportMode::Merge),
            &ExportHistory::default()
        )
        .unwrap_err()
        .code(),
        "EXPORT_TARGET_UNSAFE"
    );
}
#[test]
fn registry_is_cleared_on_close_and_contract_round_trips() {
    let (t, l, _) = fixture();
    let p = plan_export(
        &l,
        request(t.path().join("out"), ExportMode::Merge),
        &ExportHistory::default(),
    )
    .unwrap();
    let value = serde_json::to_value(&p).unwrap();
    assert_eq!(value["portraitCount"], 3);
    let decoded: ExportPlan = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    let root = l.root().to_path_buf();
    drop(l);
    let reopened = Library::open(&root).unwrap();
    assert_eq!(
        reopened.export_plan(p.id).unwrap_err().code(),
        "EXPORT_PLAN_NOT_FOUND"
    );
}

#[test]
fn warns_for_nonstandard_images_only_within_frozen_scope() {
    let (t, l, ids) = fixture();
    l.connection().execute("INSERT INTO assets(portrait_id,role,relative_path,width,height,file_size) VALUES (?1,'small',?2,1,1,0)",rusqlite::params![ids[0].to_string(),format!("portraits/{}/Small.png",ids[0])]).unwrap();
    let all = plan_export(
        &l,
        request(t.path().join("out"), ExportMode::Merge),
        &ExportHistory::default(),
    )
    .unwrap();
    assert!(all.requires_confirmation);
    assert!(
        all.warnings
            .iter()
            .any(|w| w.contains("1 portraits have nonstandard"))
    );
    l.connection()
        .execute(
            "INSERT INTO selection(portrait_id) VALUES (?)",
            [ids[1].to_string()],
        )
        .unwrap();
    let mut r = request(t.path().join("out"), ExportMode::Merge);
    r.scope = ExportScope::Selected;
    let selected = plan_export(&l, r, &ExportHistory::default()).unwrap();
    assert!(!selected.requires_confirmation);
    assert!(selected.warnings.is_empty());
}
