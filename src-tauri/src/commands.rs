use std::path::PathBuf;

use portrait_core::Library;
use portrait_core::metadata::MetadataPatch;
use portrait_core::types::{
    AppError, CatalogFacets, CatalogPage, Destination, DiscoveryReport, DuplicateConsolidation,
    DuplicateConsolidationReport, DuplicateScanReport, Game, ImportDuplicateReport, ImportReport,
    ImportRequest, Job, Page, Query, Role, SelectionAction, SelectionTarget,
};
use tauri::State;

use crate::state::{DesktopState, LibraryInfo};

#[tauri::command]
pub(crate) async fn create_library(
    path: String,
    state: State<'_, DesktopState>,
) -> Result<LibraryInfo, AppError> {
    let path = PathBuf::from(path);
    let library = tauri::async_runtime::spawn_blocking(move || Library::create(&path))
        .await
        .map_err(worker_error)?
        .map_err(DesktopState::app_error)?;
    state.activate(library)
}

#[tauri::command]
pub(crate) async fn open_library(
    path: String,
    state: State<'_, DesktopState>,
) -> Result<LibraryInfo, AppError> {
    let path = PathBuf::from(path);
    let library = tauri::async_runtime::spawn_blocking(move || Library::open(&path))
        .await
        .map_err(worker_error)?
        .map_err(DesktopState::app_error)?;
    state.activate(library)
}

#[tauri::command]
pub(crate) fn close_library(state: State<'_, DesktopState>) -> Result<(), AppError> {
    state.close()
}

#[tauri::command]
pub(crate) fn get_last_library_path(
    state: State<'_, DesktopState>,
) -> Result<Option<String>, AppError> {
    Ok(state
        .last_library_path()?
        .map(|path| path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub(crate) fn start_import(
    request: ImportRequest,
    state: State<'_, DesktopState>,
) -> Result<Job, AppError> {
    state.start_import(request)
}

#[tauri::command]
pub(crate) fn get_job(id: String, state: State<'_, DesktopState>) -> Result<Option<Job>, AppError> {
    let id = uuid::Uuid::parse_str(&id).map_err(|_| AppError {
        code: "JOB_NOT_FOUND".into(),
        message: "The import job is no longer available.".into(),
        recoverable: true,
    })?;
    state.job(id)
}

#[tauri::command]
pub(crate) fn cancel_job(id: String, state: State<'_, DesktopState>) -> Result<(), AppError> {
    let id = uuid::Uuid::parse_str(&id).map_err(|_| AppError {
        code: "JOB_NOT_FOUND".into(),
        message: "The import job is no longer available.".into(),
        recoverable: true,
    })?;
    state.cancel_job(id)
}

#[tauri::command]
pub(crate) fn get_import_report(
    id: String,
    state: State<'_, DesktopState>,
) -> Result<Option<ImportReport>, AppError> {
    let id = uuid::Uuid::parse_str(&id).map_err(|_| AppError {
        code: "JOB_NOT_FOUND".into(),
        message: "The import job is no longer available.".into(),
        recoverable: true,
    })?;
    state.import_report(id)
}

#[tauri::command]
pub(crate) fn start_duplicate_scan(state: State<'_, DesktopState>) -> Result<Job, AppError> {
    state.start_duplicate_scan()
}

#[tauri::command]
pub(crate) fn get_duplicate_scan_report(
    id: String,
    state: State<'_, DesktopState>,
) -> Result<Option<DuplicateScanReport>, AppError> {
    state.duplicate_scan_report(uuid::Uuid::parse_str(&id).map_err(worker_error)?)
}

#[tauri::command]
pub(crate) fn start_import_duplicate_scan(
    request: ImportRequest,
    state: State<'_, DesktopState>,
) -> Result<Job, AppError> {
    state.start_import_duplicate_scan(request)
}

#[tauri::command]
pub(crate) fn get_import_duplicate_report(
    id: String,
    state: State<'_, DesktopState>,
) -> Result<Option<ImportDuplicateReport>, AppError> {
    state.import_duplicate_report(uuid::Uuid::parse_str(&id).map_err(worker_error)?)
}

#[tauri::command]
pub(crate) async fn consolidate_duplicates(
    groups: Vec<DuplicateConsolidation>,
    state: State<'_, DesktopState>,
) -> Result<DuplicateConsolidationReport, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.consolidate_duplicates(&groups))
        .await
        .map_err(worker_error)?
}

#[tauri::command]
pub(crate) async fn query_catalog(
    query: Query,
    page: Page,
    state: State<'_, DesktopState>,
) -> Result<CatalogPage, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.query_catalog(&query, page))
        .await
        .map_err(worker_error)?
}

#[tauri::command]
pub(crate) async fn get_catalog_facets(
    state: State<'_, DesktopState>,
) -> Result<CatalogFacets, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.catalog_facets())
        .await
        .map_err(worker_error)?
}

#[tauri::command]
pub(crate) fn get_display_role(state: State<'_, DesktopState>) -> Result<Role, AppError> {
    state.display_role()
}

#[tauri::command]
pub(crate) fn set_display_role(role: Role, state: State<'_, DesktopState>) -> Result<(), AppError> {
    state.remember_display_role(role)
}

#[tauri::command]
pub(crate) async fn edit_metadata(
    ids: Vec<uuid::Uuid>,
    patch: MetadataPatch,
    state: State<'_, DesktopState>,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.edit_metadata(&ids, patch))
        .await
        .map_err(worker_error)?
}

#[tauri::command]
pub(crate) async fn rename_source(
    id: uuid::Uuid,
    name: String,
    state: State<'_, DesktopState>,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.rename_source(id, &name))
        .await
        .map_err(worker_error)?
}

#[tauri::command]
pub(crate) async fn change_selection(
    target: SelectionTarget,
    action: SelectionAction,
    state: State<'_, DesktopState>,
) -> Result<u64, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.change_selection(target, action))
        .await
        .map_err(worker_error)?
}

#[tauri::command]
pub(crate) async fn trash_portraits(
    ids: Vec<uuid::Uuid>,
    state: State<'_, DesktopState>,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.trash_portraits(&ids))
        .await
        .map_err(worker_error)?
}

#[tauri::command]
pub(crate) async fn restore_portraits(
    ids: Vec<uuid::Uuid>,
    state: State<'_, DesktopState>,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.restore_portraits(&ids))
        .await
        .map_err(worker_error)?
}

#[tauri::command]
pub(crate) fn start_purge(
    ids: Vec<uuid::Uuid>,
    state: State<'_, DesktopState>,
) -> Result<Job, AppError> {
    state.start_purge(ids)
}

#[tauri::command]
pub(crate) async fn discover_destinations(
    state: State<'_, DesktopState>,
) -> Result<DiscoveryReport, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.discover_destinations())
        .await
        .map_err(worker_error)?
}

#[tauri::command]
pub(crate) fn get_saved_destinations(
    state: State<'_, DesktopState>,
) -> Result<Vec<Destination>, AppError> {
    state.saved_destinations()
}

#[tauri::command]
pub(crate) fn save_destination(
    destination: Destination,
    state: State<'_, DesktopState>,
) -> Result<(), AppError> {
    state.save_destination(destination)
}

#[tauri::command]
pub(crate) fn validate_destination(
    path: String,
    game: Game,
    state: State<'_, DesktopState>,
) -> Result<Destination, AppError> {
    state.validate_destination(PathBuf::from(path), game)
}

#[tauri::command]
pub(crate) fn resolve_prefix(
    path: String,
    game: Game,
    state: State<'_, DesktopState>,
) -> Result<Vec<Destination>, AppError> {
    state.resolve_prefix(PathBuf::from(path), game)
}

fn worker_error(error: impl std::fmt::Display) -> AppError {
    AppError {
        code: "WORKER_ERROR".into(),
        message: format!("The library operation could not finish: {error}"),
        recoverable: true,
    }
}

#[tauri::command]
pub(crate) async fn plan_export(
    request: portrait_core::types::ExportRequest,
    state: State<'_, DesktopState>,
) -> Result<portrait_core::types::ExportPlan, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.plan_export(request))
        .await
        .map_err(worker_error)?
}
#[tauri::command]
pub(crate) async fn designate_export_target(
    path: String,
    state: State<'_, DesktopState>,
) -> Result<PathBuf, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.designate_export_target(PathBuf::from(path)))
        .await
        .map_err(worker_error)?
}
#[tauri::command]
pub(crate) async fn validate_export_plan(
    id: uuid::Uuid,
    state: State<'_, DesktopState>,
) -> Result<portrait_core::types::ExportPlan, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.validate_export_plan(id))
        .await
        .map_err(worker_error)?
}
#[tauri::command]
pub(crate) fn discard_export_plan(
    id: uuid::Uuid,
    state: State<'_, DesktopState>,
) -> Result<(), AppError> {
    state.discard_export_plan(id)
}

#[tauri::command]
pub(crate) async fn apply_export(
    id: uuid::Uuid,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<Job, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.apply_export(id, confirmed))
        .await
        .map_err(worker_error)?
}
#[tauri::command]
pub(crate) fn get_export_report(
    id: uuid::Uuid,
    state: State<'_, DesktopState>,
) -> Result<Option<portrait_core::types::ExportReport>, AppError> {
    state.export_report(id)
}

#[tauri::command]
pub(crate) async fn start_backup(
    path: String,
    confirmed_path: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<Job, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.start_backup(path.into(), confirmed_path.map(Into::into))
    })
    .await
    .map_err(worker_error)?
}
#[tauri::command]
pub(crate) async fn start_restore(
    archive: String,
    path: String,
    state: State<'_, DesktopState>,
) -> Result<Job, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.start_restore(archive.into(), path.into()))
        .await
        .map_err(worker_error)?
}
#[tauri::command]
pub(crate) fn get_restore_result(
    id: String,
    state: State<'_, DesktopState>,
) -> Result<Option<LibraryInfo>, AppError> {
    let id = uuid::Uuid::parse_str(&id).map_err(worker_error)?;
    state.restored_library(id)
}
