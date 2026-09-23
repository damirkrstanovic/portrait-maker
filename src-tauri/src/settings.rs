use std::fs;
use std::path::{Path, PathBuf};

use portrait_core::types::AppError;
use portrait_core::types::{Destination, Role};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    last_library: Option<PathBuf>,
    display_role: Option<Role>,
    #[serde(default)]
    destinations: Vec<Destination>,
    #[serde(default)]
    analysis: crate::state::AnalysisSettings,
}

#[derive(Clone)]
pub(crate) struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub(crate) fn analysis_log_path(&self) -> PathBuf {
        self.path
            .parent()
            .expect("settings has a parent")
            .join("logs/analysis-errors.jsonl")
    }

    pub(crate) fn new(config_dir: PathBuf) -> Self {
        Self {
            path: config_dir.join("settings.json"),
        }
    }

    pub(crate) fn last_library(&self) -> Result<Option<PathBuf>, AppError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&self.path).map_err(settings_error)?;
        let settings: Settings = serde_json::from_slice(&bytes).map_err(settings_error)?;
        Ok(settings.last_library)
    }

    pub(crate) fn remember_library(&self, root: &Path) -> Result<(), AppError> {
        let mut settings = self.read()?;
        settings.last_library = Some(root.to_path_buf());
        self.write(&settings)
    }

    pub(crate) fn display_role(&self) -> Result<Role, AppError> {
        Ok(self.read()?.display_role.unwrap_or(Role::Large))
    }

    pub(crate) fn remember_display_role(&self, role: Role) -> Result<(), AppError> {
        let mut settings = self.read()?;
        settings.display_role = Some(role);
        self.write(&settings)
    }

    pub(crate) fn destinations(&self) -> Result<Vec<Destination>, AppError> {
        Ok(self.read()?.destinations)
    }

    pub(crate) fn remember_destination(&self, destination: Destination) -> Result<(), AppError> {
        let mut settings = self.read()?;
        settings
            .destinations
            .retain(|saved| !(saved.game == destination.game && saved.path == destination.path));
        settings.destinations.push(destination);
        self.write(&settings)
    }

    pub(crate) fn analysis(&self) -> Result<crate::state::AnalysisSettings, AppError> {
        Ok(self.read()?.analysis)
    }

    pub(crate) fn remember_analysis(
        &self,
        config: crate::state::AnalysisSettings,
    ) -> Result<(), AppError> {
        let mut settings = self.read()?;
        settings.analysis = config;
        self.write(&settings)
    }

    fn read(&self) -> Result<Settings, AppError> {
        if !self.path.exists() {
            return Ok(Settings::default());
        }
        serde_json::from_slice(&fs::read(&self.path).map_err(settings_error)?)
            .map_err(settings_error)
    }

    fn write(&self, settings: &Settings) -> Result<(), AppError> {
        let parent = self.path.parent().ok_or_else(|| AppError {
            code: "SETTINGS_ERROR".into(),
            message: "The app settings location is invalid.".into(),
            recoverable: true,
        })?;
        fs::create_dir_all(parent).map_err(settings_error)?;
        let bytes = serde_json::to_vec_pretty(settings).map_err(settings_error)?;
        fs::write(&self.path, bytes).map_err(settings_error)
    }
}

fn settings_error(error: impl std::fmt::Display) -> AppError {
    AppError {
        code: "SETTINGS_ERROR".into(),
        message: format!("The app could not save or read its settings: {error}"),
        recoverable: true,
    }
}
