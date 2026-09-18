mod assets;
mod commands;
mod export_store;
mod settings;
pub mod state;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            app.manage(state::DesktopState::new(config_dir));
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("portrait", |context, request, responder| {
            // URI protocol callbacks run on the webview path. Thumbnail cache
            // misses decode and encode PNGs, so keep that CPU and disk work off
            // the UI thread.
            let state = context
                .app_handle()
                .state::<state::DesktopState>()
                .inner()
                .clone();
            let path = request.uri().path().to_owned();
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(assets::serve(&state, &path));
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_analysis_settings,
            commands::start_analysis,
            commands::start_backup,
            commands::start_restore,
            commands::get_restore_result,
            commands::create_library,
            commands::open_library,
            commands::close_library,
            commands::get_last_library_path,
            commands::start_import,
            commands::get_job,
            commands::cancel_job,
            commands::get_import_report,
            commands::start_duplicate_scan,
            commands::get_duplicate_scan_report,
            commands::start_import_duplicate_scan,
            commands::get_import_duplicate_report,
            commands::consolidate_duplicates,
            commands::query_catalog,
            commands::get_catalog_facets,
            commands::get_display_role,
            commands::set_display_role,
            commands::edit_metadata,
            commands::rename_source,
            commands::change_selection,
            commands::trash_portraits,
            commands::restore_portraits,
            commands::start_purge,
            commands::discover_destinations,
            commands::get_saved_destinations,
            commands::save_destination,
            commands::validate_destination,
            commands::resolve_prefix,
            commands::plan_export,
            commands::apply_export,
            commands::get_export_report,
            commands::designate_export_target,
            commands::validate_export_plan,
            commands::discard_export_plan,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Pathfinder Portrait Manager");
}
