use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use portrait_core::catalog::{catalog_facets, query_catalog};
use portrait_core::discovery::{
    DiscoveryEnvironment, discover_report, resolve_prefix, validate_destination,
};
use portrait_core::duplicate::{consolidate_duplicates, scan_duplicates, scan_import_duplicates};
use portrait_core::import::{JobContext, import_portraits};
use portrait_core::metadata::{MetadataPatch, edit_metadata, rename_source};
use portrait_core::selection::change_selection;
use portrait_core::trash::{purge_portraits, restore_portraits, trash_portraits};
use portrait_core::types::{
    AppError, CatalogFacets, CatalogPage, Destination, DiscoveryReport, DuplicateConsolidation,
    DuplicateConsolidationReport, DuplicateScanReport, Game, ImportDuplicateReport, ImportReport,
    ImportRequest, Job, JobState, Page, Query, Role, SelectionAction, SelectionTarget,
};
use portrait_core::{CoreError, Library};
use serde::Serialize;
use uuid::Uuid;

use crate::settings::SettingsStore;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryInfo {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Clone)]
pub struct DesktopState {
    library: Arc<Mutex<Option<Library>>>,
    jobs: Arc<Mutex<HashMap<Uuid, LibraryJob>>>,
    settings: SettingsStore,
    exports: crate::export_store::ExportStore,
}

struct LibraryJob {
    job: Job,
    context: JobContext,
    report: Option<ImportReport>,
    duplicate_report: Option<DuplicateScanReport>,
    import_duplicate_report: Option<ImportDuplicateReport>,
    export_report: Option<portrait_core::types::ExportReport>,
    restored_library: Option<LibraryInfo>,
}

impl DesktopState {
    #[must_use]
    pub fn new(config_dir: PathBuf) -> Self {
        Self {
            library: Arc::new(Mutex::new(None)),
            jobs: Arc::new(Mutex::new(HashMap::new())),
            exports: crate::export_store::ExportStore::new(&config_dir),
            settings: SettingsStore::new(config_dir),
        }
    }

    pub fn activate(&self, library: Library) -> Result<LibraryInfo, AppError> {
        self.exports.flush(&library)?;
        self.settings.remember_library(library.root())?;
        let info = LibraryInfo {
            id: library.id().to_string(),
            name: library.name().to_owned(),
            path: library.root().to_string_lossy().into_owned(),
        };
        *self.library.lock().map_err(state_error)? = Some(library);
        Ok(info)
    }

    pub fn close(&self) -> Result<(), AppError> {
        *self.library.lock().map_err(state_error)? = None;
        Ok(())
    }

    pub fn last_library_path(&self) -> Result<Option<PathBuf>, AppError> {
        self.settings.last_library()
    }

    pub fn start_backup(
        &self,
        path: PathBuf,
        confirmed_path: Option<PathBuf>,
    ) -> Result<Job, AppError> {
        if path.try_exists().map_err(|e| Self::app_error(e.into()))?
            && confirmed_path.as_ref() != Some(&path)
        {
            return Err(Self::app_error(CoreError::BackupConfirmationRequired));
        }
        let identity = self
            .library
            .lock()
            .map_err(state_error)?
            .as_ref()
            .map(|lib| (lib.id(), lib.root().to_path_buf()))
            .ok_or_else(|| library_not_open("backing up"))?;
        let overwrite = confirmed_path.as_ref() == Some(&path);
        self.start_portable_job(
            move |state, context| {
                let guard = state.library.lock().map_err(state_error)?;
                let lib = guard
                    .as_ref()
                    .filter(|lib| lib.id() == identity.0 && lib.root() == identity.1)
                    .ok_or_else(|| library_not_open("backing up the original library"))?;
                portrait_core::backup::backup_library_confirmed(lib, &path, context, overwrite)
                    .map_err(Self::app_error)?;
                Ok(None)
            },
            "Preparing portable backup",
        )
    }

    pub fn start_restore(&self, archive: PathBuf, path: PathBuf) -> Result<Job, AppError> {
        if self.library.lock().map_err(state_error)?.is_some() {
            return Err(state_error(
                "Close the current library before restoring a backup.",
            ));
        }
        self.start_portable_job(
            move |state, context| {
                let mut guard = state.library.lock().map_err(state_error)?;
                if guard.is_some() {
                    return Err(state_error(
                        "Close the current library before restoring a backup.",
                    ));
                }
                let library = portrait_core::backup::restore_library(&archive, &path, context)
                    .map_err(Self::app_error)?;
                let info = LibraryInfo {
                    id: library.id().to_string(),
                    name: library.name().into(),
                    path: library.root().to_string_lossy().into_owned(),
                };
                // Settings remain machine-local; restoring never imports destinations or export history.
                let _ = state.settings.remember_library(library.root());
                *guard = Some(library);
                Ok(Some(info))
            },
            "Preparing library restore",
        )
    }

    pub fn restored_library(&self, id: Uuid) -> Result<Option<LibraryInfo>, AppError> {
        Ok(self
            .jobs
            .lock()
            .map_err(state_error)?
            .get(&id)
            .and_then(|entry| entry.restored_library.clone()))
    }

    fn start_portable_job(
        &self,
        work: impl FnOnce(&Self, &JobContext) -> Result<Option<LibraryInfo>, AppError> + Send + 'static,
        message: &str,
    ) -> Result<Job, AppError> {
        let id = Uuid::new_v4();
        let jobs_for_progress = Arc::clone(&self.jobs);
        let context = JobContext::with_status_progress(move |completed, total, message| {
            if let Ok(mut jobs) = jobs_for_progress.lock()
                && let Some(entry) = jobs.get_mut(&id)
            {
                entry.job.completed = completed;
                entry.job.total = total;
                entry.job.message = message.into();
            }
        });
        let job = Job {
            id,
            state: JobState::Running,
            completed: 0,
            total: None,
            message: message.into(),
        };
        self.jobs.lock().map_err(state_error)?.insert(
            id,
            LibraryJob {
                job: job.clone(),
                context: context.clone(),
                report: None,
                duplicate_report: None,
                import_duplicate_report: None,
                export_report: None,
                restored_library: None,
            },
        );
        let state = self.clone();
        std::thread::spawn(move || {
            let result = work(&state, &context);
            if let Ok(mut jobs) = state.jobs.lock()
                && let Some(entry) = jobs.get_mut(&id)
            {
                match result {
                    Ok(info) => {
                        entry.job.state = JobState::Done;
                        entry.job.message = if info.is_some() {
                            "Library restored and verified"
                        } else {
                            "Portable backup saved"
                        }
                        .into();
                        entry.restored_library = info;
                    }
                    Err(error) => {
                        entry.job.state = if error.code == "CANCELLED" {
                            JobState::Cancelled
                        } else {
                            JobState::Failed
                        };
                        entry.job.message = error.message;
                    }
                }
            }
        });
        Ok(job)
    }

    pub fn start_import(&self, request: ImportRequest) -> Result<Job, AppError> {
        if self.library.lock().map_err(state_error)?.is_none() {
            return Err(AppError {
                code: "LIBRARY_NOT_OPEN".into(),
                message: "Open a portrait library before importing.".into(),
                recoverable: true,
            });
        }
        let id = Uuid::new_v4();
        let jobs_for_progress = Arc::clone(&self.jobs);
        let progress_id = id;
        let context = JobContext::with_progress(move |completed, total| {
            if let Ok(mut jobs) = jobs_for_progress.lock()
                && let Some(entry) = jobs.get_mut(&progress_id)
                && entry.job.state == JobState::Running
            {
                entry.job.completed = completed;
                entry.job.total = total;
                entry.job.message = "Preparing import".into();
            }
        });
        let job = Job {
            id,
            state: JobState::Running,
            completed: 0,
            total: None,
            message: "Starting import".into(),
        };
        self.jobs.lock().map_err(state_error)?.insert(
            id,
            LibraryJob {
                job: job.clone(),
                context: context.clone(),
                report: None,
                duplicate_report: None,
                import_duplicate_report: None,
                export_report: None,
                restored_library: None,
            },
        );
        let library = Arc::clone(&self.library);
        let jobs = Arc::clone(&self.jobs);
        std::thread::spawn(move || {
            let result = match library.lock() {
                Ok(mut guard) => match guard.as_mut() {
                    Some(library) => import_portraits(library, request, &context),
                    None => Err(CoreError::LibraryNotFound),
                },
                Err(error) => Err(CoreError::Migration(error.to_string())),
            };
            if let Ok(mut jobs) = jobs.lock()
                && let Some(entry) = jobs.get_mut(&id)
            {
                match result {
                    Ok(report) => {
                        entry.job.completed =
                            entry.job.completed.max(report.imported + report.skipped);
                        entry.job.state = if report.cancelled {
                            JobState::Cancelled
                        } else {
                            JobState::Done
                        };
                        entry.job.message = if report.cancelled {
                            "Import cancelled".into()
                        } else {
                            format!(
                                "Imported {} portrait{}",
                                report.imported,
                                if report.imported == 1 { "" } else { "s" }
                            )
                        };
                        entry.report = Some(report);
                    }
                    Err(CoreError::Cancelled) => {
                        entry.job.state = JobState::Cancelled;
                        entry.job.message = "Import cancelled".into();
                    }
                    Err(error) => {
                        entry.job.state = JobState::Failed;
                        entry.job.message = error.to_string();
                    }
                }
            }
        });
        Ok(job)
    }

    pub fn job(&self, id: Uuid) -> Result<Option<Job>, AppError> {
        Ok(self
            .jobs
            .lock()
            .map_err(state_error)?
            .get(&id)
            .map(|entry| entry.job.clone()))
    }

    pub fn cancel_job(&self, id: Uuid) -> Result<(), AppError> {
        let mut jobs = self.jobs.lock().map_err(state_error)?;
        let entry = jobs.get_mut(&id).ok_or_else(|| AppError {
            code: "JOB_NOT_FOUND".into(),
            message: "The requested job is no longer available.".into(),
            recoverable: true,
        })?;
        entry.context.cancel();
        Ok(())
    }

    pub fn import_report(&self, id: Uuid) -> Result<Option<ImportReport>, AppError> {
        Ok(self
            .jobs
            .lock()
            .map_err(state_error)?
            .get(&id)
            .and_then(|entry| entry.report.clone()))
    }

    pub fn start_duplicate_scan(&self) -> Result<Job, AppError> {
        self.start_duplicate_job("Checking library duplicates", |library, context| {
            scan_duplicates(library, context)
        })
    }

    pub fn start_import_duplicate_scan(&self, request: ImportRequest) -> Result<Job, AppError> {
        if self.library.lock().map_err(state_error)?.is_none() {
            return Err(library_not_open("checking import duplicates"));
        }
        let id = Uuid::new_v4();
        let jobs_for_progress = Arc::clone(&self.jobs);
        let context = JobContext::with_status_progress(move |completed, total, message| {
            if let Ok(mut jobs) = jobs_for_progress.lock()
                && let Some(entry) = jobs.get_mut(&id)
                && entry.job.state == JobState::Running
            {
                entry.job.completed = completed;
                entry.job.total = total;
                entry.job.message = message.into();
            }
        });
        let job = Job {
            id,
            state: JobState::Running,
            completed: 0,
            total: None,
            message: "Checking import duplicates".into(),
        };
        self.jobs.lock().map_err(state_error)?.insert(
            id,
            LibraryJob {
                job: job.clone(),
                context: context.clone(),
                report: None,
                duplicate_report: None,
                import_duplicate_report: None,
                export_report: None,
                restored_library: None,
            },
        );
        let library = Arc::clone(&self.library);
        let jobs = Arc::clone(&self.jobs);
        std::thread::spawn(move || {
            let result = match library.lock() {
                Ok(guard) => guard
                    .as_ref()
                    .ok_or(CoreError::LibraryNotFound)
                    .and_then(|library| scan_import_duplicates(library, &request, &context)),
                Err(error) => Err(CoreError::Migration(error.to_string())),
            };
            if let Ok(mut jobs) = jobs.lock()
                && let Some(entry) = jobs.get_mut(&id)
            {
                match result {
                    Ok(report) => {
                        entry.job.state = JobState::Done;
                        entry.job.message = format!(
                            "Found {} import duplicate{}",
                            report.matches.len(),
                            if report.matches.len() == 1 { "" } else { "s" }
                        );
                        entry.import_duplicate_report = Some(report);
                    }
                    Err(CoreError::Cancelled) => {
                        entry.job.state = JobState::Cancelled;
                        entry.job.message = "Duplicate scan cancelled".into();
                    }
                    Err(error) => {
                        entry.job.state = JobState::Failed;
                        entry.job.message = error.to_string();
                    }
                }
            }
        });
        Ok(job)
    }

    fn start_duplicate_job(
        &self,
        message: &str,
        work: impl FnOnce(&Library, &JobContext) -> portrait_core::Result<DuplicateScanReport>
        + Send
        + 'static,
    ) -> Result<Job, AppError> {
        if self.library.lock().map_err(state_error)?.is_none() {
            return Err(library_not_open("checking duplicates"));
        }
        let id = Uuid::new_v4();
        let jobs_for_progress = Arc::clone(&self.jobs);
        let context = JobContext::with_status_progress(move |completed, total, phase| {
            if let Ok(mut jobs) = jobs_for_progress.lock()
                && let Some(entry) = jobs.get_mut(&id)
                && entry.job.state == JobState::Running
            {
                entry.job.completed = completed;
                entry.job.total = total;
                entry.job.message = phase.into();
            }
        });
        let job = Job {
            id,
            state: JobState::Running,
            completed: 0,
            total: None,
            message: message.into(),
        };
        self.jobs.lock().map_err(state_error)?.insert(
            id,
            LibraryJob {
                job: job.clone(),
                context: context.clone(),
                report: None,
                duplicate_report: None,
                import_duplicate_report: None,
                export_report: None,
                restored_library: None,
            },
        );
        let library = Arc::clone(&self.library);
        let jobs = Arc::clone(&self.jobs);
        std::thread::spawn(move || {
            let result = match library.lock() {
                Ok(guard) => guard
                    .as_ref()
                    .ok_or(CoreError::LibraryNotFound)
                    .and_then(|library| work(library, &context)),
                Err(error) => Err(CoreError::Migration(error.to_string())),
            };
            if let Ok(mut jobs) = jobs.lock()
                && let Some(entry) = jobs.get_mut(&id)
            {
                match result {
                    Ok(report) => {
                        entry.job.state = JobState::Done;
                        entry.job.message = format!(
                            "Found {} duplicate group{}",
                            report.groups.len(),
                            if report.groups.len() == 1 { "" } else { "s" }
                        );
                        entry.duplicate_report = Some(report);
                    }
                    Err(CoreError::Cancelled) => {
                        entry.job.state = JobState::Cancelled;
                        entry.job.message = "Duplicate scan cancelled".into();
                    }
                    Err(error) => {
                        entry.job.state = JobState::Failed;
                        entry.job.message = error.to_string();
                    }
                }
            }
        });
        Ok(job)
    }

    pub fn duplicate_scan_report(&self, id: Uuid) -> Result<Option<DuplicateScanReport>, AppError> {
        Ok(self
            .jobs
            .lock()
            .map_err(state_error)?
            .get(&id)
            .and_then(|entry| entry.duplicate_report.clone()))
    }
    pub fn import_duplicate_report(
        &self,
        id: Uuid,
    ) -> Result<Option<ImportDuplicateReport>, AppError> {
        Ok(self
            .jobs
            .lock()
            .map_err(state_error)?
            .get(&id)
            .and_then(|entry| entry.import_duplicate_report.clone()))
    }
    pub fn consolidate_duplicates(
        &self,
        groups: &[DuplicateConsolidation],
    ) -> Result<DuplicateConsolidationReport, AppError> {
        let mut guard = self.library.lock().map_err(state_error)?;
        let library = guard
            .as_mut()
            .ok_or_else(|| library_not_open("consolidating duplicates"))?;
        consolidate_duplicates(library, groups).map_err(Self::app_error)
    }

    pub fn query_catalog(&self, query: &Query, page: Page) -> Result<CatalogPage, AppError> {
        let guard = self.library.lock().map_err(state_error)?;
        let library = guard.as_ref().ok_or_else(|| AppError {
            code: "LIBRARY_NOT_OPEN".into(),
            message: "Open a portrait library before browsing portraits.".into(),
            recoverable: true,
        })?;
        query_catalog(library, query, page).map_err(Self::app_error)
    }

    pub fn catalog_facets(&self) -> Result<CatalogFacets, AppError> {
        let guard = self.library.lock().map_err(state_error)?;
        let library = guard.as_ref().ok_or_else(|| AppError {
            code: "LIBRARY_NOT_OPEN".into(),
            message: "Open a portrait library before browsing portraits.".into(),
            recoverable: true,
        })?;
        catalog_facets(library).map_err(Self::app_error)
    }

    pub fn display_role(&self) -> Result<Role, AppError> {
        self.settings.display_role()
    }

    pub fn remember_display_role(&self, role: Role) -> Result<(), AppError> {
        self.settings.remember_display_role(role)
    }

    pub fn plan_export(
        &self,
        request: portrait_core::types::ExportRequest,
    ) -> Result<portrait_core::types::ExportPlan, AppError> {
        let guard = self.library.lock().map_err(state_error)?;
        let library = guard
            .as_ref()
            .ok_or_else(|| library_not_open("planning an export"))?;
        self.exports.flush(library)?;
        let target = portrait_core::export::resolve_export_target(library, &request)
            .map_err(Self::app_error)?;
        let saved = if request.output == portrait_core::types::ExportOutput::Directory {
            self.settings
                .destinations()?
                .into_iter()
                .find(|d| {
                    let mut candidate = request.clone();
                    candidate.target = d.path.clone();
                    portrait_core::export::resolve_export_target(library, &candidate)
                        .ok()
                        .as_ref()
                        == Some(&target)
                })
                .map(|d| d.id)
        } else {
            None
        };
        let (id, history) = self.exports.route(&target, library.id(), saved)?;
        portrait_core::export::plan_export_for_destination(library, request, id, &history)
            .map_err(Self::app_error)
    }

    pub fn export_report(
        &self,
        id: Uuid,
    ) -> Result<Option<portrait_core::types::ExportReport>, AppError> {
        Ok(self
            .jobs
            .lock()
            .map_err(state_error)?
            .get(&id)
            .and_then(|entry| entry.export_report.clone()))
    }

    pub fn apply_export(&self, plan_id: Uuid, confirmed: bool) -> Result<Job, AppError> {
        let expected_library = {
            let guard = self.library.lock().map_err(state_error)?;
            let library = guard
                .as_ref()
                .ok_or_else(|| library_not_open("exporting portraits"))?;
            let plan = library
                .validate_export_plan(plan_id)
                .map_err(Self::app_error)?;
            if plan.requires_confirmation && !confirmed {
                return Err(Self::app_error(CoreError::ExportConfirmationRequired));
            }
            library.id()
        };
        let id = Uuid::new_v4();
        let progress_jobs = Arc::clone(&self.jobs);
        let context = JobContext::with_status_progress(move |completed, total, phase| {
            if let Ok(mut jobs) = progress_jobs.lock()
                && let Some(entry) = jobs.get_mut(&id)
            {
                entry.job.completed = completed;
                entry.job.total = total;
                entry.job.message = phase.into();
            }
        });
        let job = Job {
            id,
            state: JobState::Running,
            completed: 0,
            total: None,
            message: "Preparing export".into(),
        };
        self.jobs.lock().map_err(state_error)?.insert(
            id,
            LibraryJob {
                job: job.clone(),
                context: context.clone(),
                report: None,
                duplicate_report: None,
                import_duplicate_report: None,
                export_report: None,
                restored_library: None,
            },
        );
        let library = Arc::clone(&self.library);
        let jobs = Arc::clone(&self.jobs);
        let exports = self.exports.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<portrait_core::types::ExportReport, AppError> {
                let guard = library.lock().map_err(state_error)?;
                let library = guard
                    .as_ref()
                    .filter(|l| l.id() == expected_library)
                    .ok_or_else(|| library_not_open("applying the reviewed export"))?;
                let plan = library.export_plan(plan_id).map_err(Self::app_error)?;
                let mut report =
                    portrait_core::export::apply_export(library, &plan, confirmed, &context)
                        .map_err(Self::app_error)?;
                if let Err(e) = exports.flush(library) {
                    report.issues.push(portrait_core::types::Issue{path:plan.plan.target.to_string_lossy().into_owned(),code:e.code,message:format!("Export committed. {}. History will retry when this library is reopened.",e.message),severity:portrait_core::types::IssueSeverity::Warning});
                }
                Ok(report)
            })();
            if let Ok(mut jobs) = jobs.lock()
                && let Some(entry) = jobs.get_mut(&id)
            {
                match result {
                    Ok(report) => {
                        entry.job.state = JobState::Done;
                        entry.job.completed = report.added + report.overwritten + report.removed;
                        entry.job.total = Some(entry.job.completed);
                        entry.job.message = format!(
                            "Export complete: {} files added, {} overwritten, {} removed, {} preserved",
                            report.added, report.overwritten, report.removed, report.preserved
                        );
                        entry.export_report = Some(report);
                    }
                    Err(e) => {
                        entry.job.state = if e.code == "CANCELLED" {
                            JobState::Cancelled
                        } else {
                            JobState::Failed
                        };
                        entry.job.message = e.message;
                    }
                }
            }
        });
        Ok(job)
    }

    pub fn designate_export_target(&self, path: PathBuf) -> Result<PathBuf, AppError> {
        let guard = self.library.lock().map_err(state_error)?;
        let library = guard
            .as_ref()
            .ok_or_else(|| library_not_open("designating an export collection"))?;
        library
            .designate_export_target(&path)
            .map_err(Self::app_error)
    }

    pub fn validate_export_plan(
        &self,
        id: Uuid,
    ) -> Result<portrait_core::types::ExportPlan, AppError> {
        let guard = self.library.lock().map_err(state_error)?;
        let library = guard
            .as_ref()
            .ok_or_else(|| library_not_open("validating an export preview"))?;
        library.validate_export_plan(id).map_err(Self::app_error)
    }

    pub fn discard_export_plan(&self, id: Uuid) -> Result<(), AppError> {
        let guard = self.library.lock().map_err(state_error)?;
        let library = guard
            .as_ref()
            .ok_or_else(|| library_not_open("discarding an export preview"))?;
        library.discard_export_plan(id);
        Ok(())
    }

    pub fn discover_destinations(&self) -> Result<DiscoveryReport, AppError> {
        discover_report(&DiscoveryEnvironment::production()).map_err(Self::app_error)
    }

    pub fn saved_destinations(&self) -> Result<Vec<Destination>, AppError> {
        self.settings
            .destinations()?
            .into_iter()
            .map(|saved| {
                let mut validated =
                    validate_destination(&saved.path, saved.game).map_err(Self::app_error)?;
                validated.id = saved.id;
                validated.name = saved.name;
                Ok(validated)
            })
            .collect()
    }

    pub fn save_destination(&self, destination: Destination) -> Result<(), AppError> {
        self.settings.remember_destination(destination)
    }

    pub fn validate_destination(&self, path: PathBuf, game: Game) -> Result<Destination, AppError> {
        validate_destination(&path, game).map_err(Self::app_error)
    }

    pub fn resolve_prefix(&self, path: PathBuf, game: Game) -> Result<Vec<Destination>, AppError> {
        resolve_prefix(&path, game).map_err(Self::app_error)
    }

    pub fn edit_metadata(&self, ids: &[Uuid], patch: MetadataPatch) -> Result<(), AppError> {
        let mut guard = self.library.lock().map_err(state_error)?;
        let library = guard
            .as_mut()
            .ok_or_else(|| library_not_open("editing portraits"))?;
        edit_metadata(library, ids, patch).map_err(Self::app_error)
    }

    pub fn rename_source(&self, id: Uuid, name: &str) -> Result<(), AppError> {
        let mut guard = self.library.lock().map_err(state_error)?;
        let library = guard
            .as_mut()
            .ok_or_else(|| library_not_open("editing sources"))?;
        rename_source(library, id, name).map_err(Self::app_error)
    }

    pub fn change_selection(
        &self,
        target: SelectionTarget,
        action: SelectionAction,
    ) -> Result<u64, AppError> {
        let mut guard = self.library.lock().map_err(state_error)?;
        let library = guard
            .as_mut()
            .ok_or_else(|| library_not_open("changing selection"))?;
        change_selection(library, target, action).map_err(Self::app_error)
    }

    pub fn trash_portraits(&self, ids: &[Uuid]) -> Result<(), AppError> {
        let mut guard = self.library.lock().map_err(state_error)?;
        let library = guard
            .as_mut()
            .ok_or_else(|| library_not_open("moving portraits to trash"))?;
        trash_portraits(library, ids).map_err(Self::app_error)
    }

    pub fn restore_portraits(&self, ids: &[Uuid]) -> Result<(), AppError> {
        let mut guard = self.library.lock().map_err(state_error)?;
        let library = guard
            .as_mut()
            .ok_or_else(|| library_not_open("restoring portraits"))?;
        restore_portraits(library, ids).map_err(Self::app_error)
    }

    pub fn start_purge(&self, ids: Vec<Uuid>) -> Result<Job, AppError> {
        if self.library.lock().map_err(state_error)?.is_none() {
            return Err(library_not_open("permanently deleting portraits"));
        }
        let id = Uuid::new_v4();
        let jobs_for_progress = Arc::clone(&self.jobs);
        let progress_id = id;
        let context = JobContext::with_progress(move |completed, total| {
            if let Ok(mut jobs) = jobs_for_progress.lock()
                && let Some(entry) = jobs.get_mut(&progress_id)
                && entry.job.state == JobState::Running
            {
                entry.job.completed = completed;
                entry.job.total = total;
                entry.job.message = format!(
                    "Permanently deleting {completed} portrait{}",
                    if completed == 1 { "" } else { "s" }
                );
            }
        });
        let job = Job {
            id,
            state: JobState::Running,
            completed: 0,
            total: None,
            message: "Preparing permanent deletion".into(),
        };
        self.jobs.lock().map_err(state_error)?.insert(
            id,
            LibraryJob {
                job: job.clone(),
                context: context.clone(),
                report: None,
                duplicate_report: None,
                import_duplicate_report: None,
                export_report: None,
                restored_library: None,
            },
        );
        let library = Arc::clone(&self.library);
        let jobs = Arc::clone(&self.jobs);
        std::thread::spawn(move || {
            let result = match library.lock() {
                Ok(mut guard) => match guard.as_mut() {
                    Some(library) => purge_portraits(library, &ids, &context),
                    None => Err(CoreError::LibraryNotFound),
                },
                Err(error) => Err(CoreError::Recovery(error.to_string())),
            };
            if let Ok(mut jobs) = jobs.lock()
                && let Some(entry) = jobs.get_mut(&id)
            {
                match result {
                    Ok(deleted) => {
                        entry.job.completed = deleted;
                        entry.job.total = Some(deleted);
                        entry.job.state = JobState::Done;
                        entry.job.message = format!(
                            "Permanently deleted {} portrait{}",
                            entry.job.completed,
                            if entry.job.completed == 1 { "" } else { "s" }
                        );
                    }
                    Err(CoreError::Cancelled) => {
                        entry.job.state = JobState::Cancelled;
                        entry.job.message = "Permanent deletion cancelled".into();
                    }
                    Err(error) => {
                        entry.job.state = JobState::Failed;
                        entry.job.message = error.to_string();
                    }
                }
            }
        });
        Ok(job)
    }

    pub fn asset_path(
        &self,
        id: Uuid,
        role: Role,
        thumbnail_edge: Option<u32>,
    ) -> Result<PathBuf, AppError> {
        // Only the SQLite lookup and managed-path validation need the library
        // lock. Decoding and encoding PNGs can take seconds for a large grid;
        // holding this mutex through that work serialised every protocol image
        // request and blocked catalog commands behind it.
        let resolved = {
            let guard = self.library.lock().map_err(state_error)?;
            let library = guard.as_ref().ok_or_else(|| AppError {
                code: "LIBRARY_NOT_OPEN".into(),
                message: "Open a portrait library before browsing portraits.".into(),
                recoverable: true,
            })?;
            match thumbnail_edge {
                Some(_) => portrait_core::thumbnails::thumbnail_source(library, id, role)
                    .map(ResolvedAsset::Thumbnail),
                None => portrait_core::thumbnails::asset_path(library, id, role)
                    .map(ResolvedAsset::Original),
            }
        }
        .map_err(Self::app_error)?;

        match (resolved, thumbnail_edge) {
            (ResolvedAsset::Thumbnail(source), Some(edge)) => {
                portrait_core::thumbnails::thumbnail_from_source(&source, id, role, edge)
            }
            (ResolvedAsset::Original(path), None) => Ok(path),
            _ => unreachable!("thumbnail edge and resolved asset always agree"),
        }
        .map_err(Self::app_error)
    }

    #[must_use]
    pub fn app_error(error: CoreError) -> AppError {
        AppError {
            code: error.code().into(),
            message: error.to_string(),
            recoverable: error.recoverable(),
        }
    }
}

enum ResolvedAsset {
    Thumbnail(portrait_core::thumbnails::ThumbnailSource),
    Original(PathBuf),
}

fn library_not_open(activity: &str) -> AppError {
    AppError {
        code: "LIBRARY_NOT_OPEN".into(),
        message: format!("Open a portrait library before {activity}."),
        recoverable: true,
    }
}

fn state_error(error: impl std::fmt::Display) -> AppError {
    AppError {
        code: "APP_STATE_ERROR".into(),
        message: format!("The open library state is unavailable: {error}"),
        recoverable: true,
    }
}
