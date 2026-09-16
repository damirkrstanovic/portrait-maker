fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "start_backup",
        "start_restore",
        "get_restore_result",
        "create_library",
        "open_library",
        "close_library",
        "get_last_library_path",
        "discover_destinations",
        "get_saved_destinations",
        "save_destination",
        "validate_destination",
        "resolve_prefix",
        "plan_export",
        "apply_export",
        "get_export_report",
        "designate_export_target",
        "validate_export_plan",
        "discard_export_plan",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("failed to build Tauri command permissions");
}
