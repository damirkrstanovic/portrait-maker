use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::Engine;
use portrait_core::{
    analysis::{PortraitAnalysis, analysis_candidates, save_analysis},
    import::JobContext,
    types::{AppError, Job, JobState, Role},
};
use reqwest::{
    blocking::Client,
    header::{AUTHORIZATION, HeaderValue},
    redirect::Policy,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{DesktopState, LibraryJob, library_not_open, state_error};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSettings {
    pub endpoint: String,
    pub model: String,
    pub api_key_path: String,
}
impl Default for AnalysisSettings {
    fn default() -> Self {
        Self {
            endpoint: "http://lizard10.local:8080/v1/chat/completions".into(),
            model: "gemma-4-26b-a4b".into(),
            api_key_path: ".apikey".into(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisRequest {
    pub endpoint: String,
    pub model: String,
    pub api_key_path: String,
    pub selected_only: bool,
    pub overwrite: bool,
}

const PROMPT: &str = r#"Describe this fictional fantasy portrait for a Pathfinder portrait library. Return a JSON object shaped like {"description":"...","labels":[{"category":"race","value":"elf"}]}. Describe visible appearance, clothing, equipment, pose and background in useful detail for searching. Do not infer real-world ethnicity, identity or personal traits. Treat ancestry/class as fictional visual archetypes, not established character facts. Gender labels describe only depicted presentation, not gender identity. Omit uncertain labels rather than guessing.
Labels are freeform category/value strings. Prefer useful RPG categories such as gender, race, class, combat, weapon, armor and magic; add other categories when helpful, such as hair, clothing, pose or background. Use descriptive values, including specific weapon types and class archetypes, without limiting them to a fixed vocabulary. Prefer lowercase for consistent filtering. Do not use filenames as evidence. No Markdown or explanatory text outside JSON."#;

fn error(code: &str, message: &str) -> AppError {
    AppError {
        code: code.into(),
        message: message.into(),
        recoverable: true,
    }
}
fn validated(request: &AnalysisRequest) -> Result<AnalysisSettings, AppError> {
    let url = reqwest::Url::parse(request.endpoint.trim())
        .map_err(|_| error("ANALYSIS_SETTINGS", "Enter a valid HTTP analysis endpoint."))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(error(
            "ANALYSIS_SETTINGS",
            "Use an HTTP endpoint without embedded credentials, query parameters or fragments.",
        ));
    }
    let model = request.model.trim();
    if model.is_empty()
        || model.len() > 200
        || model.chars().any(char::is_control)
        || request.api_key_path.trim().is_empty()
    {
        return Err(error(
            "ANALYSIS_SETTINGS",
            "Enter a model name and a backend API-key file path.",
        ));
    }
    Ok(AnalysisSettings {
        endpoint: url.to_string(),
        model: model.into(),
        api_key_path: request.api_key_path.trim().into(),
    })
}
fn token(path: &str) -> Result<String, AppError> {
    let configured = PathBuf::from(path);
    #[cfg(debug_assertions)]
    let configured = if path == ".apikey" && !configured.exists() {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".apikey")
    } else {
        configured
    };
    let file = fs::File::open(configured).map_err(|_| {
        error(
            "ANALYSIS_KEY",
            "The backend could not read the API-key file. Check its path and permissions.",
        )
    })?;
    let mut bytes = Vec::new();
    file.take(8193).read_to_end(&mut bytes).map_err(|_| {
        error(
            "ANALYSIS_KEY",
            "The backend could not read the API-key file.",
        )
    })?;
    if bytes.len() > 8192 {
        return Err(error("ANALYSIS_KEY", "The API-key file is too large."));
    }
    let value = String::from_utf8(bytes).map_err(|_| {
        error(
            "ANALYSIS_KEY",
            "The API-key file must contain a UTF-8 token.",
        )
    })?;
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(error(
            "ANALYSIS_KEY",
            "The API-key file must contain one nonempty token.",
        ));
    }
    Ok(value.into())
}
struct AnalysisResponse {
    analysis: PortraitAnalysis,
    status: u16,
    body: String,
}
struct AnalysisFailure {
    code: &'static str,
    stage: &'static str,
    detail: String,
    status: Option<u16>,
    body: Option<String>,
}
impl AnalysisFailure {
    fn new(code: &'static str, stage: &'static str, detail: impl ToString) -> Self {
        Self {
            code,
            stage,
            detail: detail.to_string(),
            status: None,
            body: None,
        }
    }
    fn response(mut self, status: u16, body: &str) -> Self {
        self.status = Some(status);
        self.body = Some(body.into());
        self
    }
}

// Redact before serializing the log record, including escaped tokens in nested JSON strings.
fn redact(text: &str, secret: &str) -> String {
    let mut result = if secret.is_empty() {
        text.into()
    } else {
        text.replace(secret, "[REDACTED]")
    };
    if !secret.is_empty() {
        let escaped = serde_json::to_string(secret).expect("string serialization");
        result = result.replace(&escaped[1..escaped.len() - 1], "[REDACTED]");
    }
    if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&result) {
        fn visit(value: &mut serde_json::Value, secret: &str) {
            match value {
                serde_json::Value::String(text) => *text = redact(text, secret),
                serde_json::Value::Array(items) => items.iter_mut().for_each(|v| visit(v, secret)),
                serde_json::Value::Object(items) => {
                    *items = std::mem::take(items)
                        .into_iter()
                        .map(|(k, mut v)| {
                            visit(&mut v, secret);
                            (redact(&k, secret), v)
                        })
                        .collect();
                }
                _ => {}
            }
        }
        let original = value.clone();
        visit(&mut value, secret);
        if value != original {
            result = value.to_string();
        }
    }
    // A server may echo request data. Never retain image data URIs in diagnostic logs.
    while let Some(start) = result.find("data:image/") {
        let end = result[start..]
            .find(|c: char| c.is_whitespace() || matches!(c, '\"' | '\\'))
            .map_or(result.len(), |n| start + n);
        result.replace_range(start..end, "[IMAGE REDACTED]");
    }
    result
}

fn log_failure(
    path: &std::path::Path,
    job: Uuid,
    portrait: Uuid,
    model: &str,
    secret: &str,
    failure: AnalysisFailure,
) -> AppError {
    let detail = redact(&failure.detail, secret);
    let record = serde_json::json!({
        "timestampUnixMs": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        "jobId": job, "portraitId": portrait, "model": redact(model, secret),
        "stage": failure.stage, "error": detail, "httpStatus": failure.status,
        "response": failure.body.as_deref().map(|body| redact(body, secret)),
    });
    let write = (|| -> std::io::Result<()> {
        fs::create_dir_all(path.parent().expect("log has parent"))?;
        let mut options = fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        let mut bytes = serde_json::to_vec(&record)?;
        bytes.push(b'\n');
        file.write_all(&bytes)
    })();
    let location = redact(&path.display().to_string(), secret);
    let message = match write {
        Ok(()) => format!("{}: {detail}. Details: {location}", failure.stage),
        Err(err) => format!(
            "{}: {detail}. Could not write failure log {location}: {}",
            failure.stage,
            redact(&err.to_string(), secret)
        ),
    };
    error(failure.code, &message)
}

struct Running(Arc<AtomicBool>);
impl Drop for Running {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl DesktopState {
    pub fn analysis_settings(&self) -> Result<AnalysisSettings, AppError> {
        self.settings.analysis()
    }

    pub fn start_analysis(&self, request: AnalysisRequest) -> Result<Job, AppError> {
        let config = validated(&request)?;
        let (identity, ids) = {
            let guard = self.library.lock().map_err(state_error)?;
            let lib = guard
                .as_ref()
                .ok_or_else(|| library_not_open("analyzing portraits"))?;
            (
                (lib.id(), lib.root().to_path_buf()),
                analysis_candidates(lib, request.selected_only, request.overwrite)
                    .map_err(Self::app_error)?,
            )
        };
        if self
            .analysis_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(error(
                "ANALYSIS_RUNNING",
                "A portrait analysis job is already running.",
            ));
        }
        let running = Running(Arc::clone(&self.analysis_running));
        self.settings.remember_analysis(config.clone())?;
        let no_candidates = ids.is_empty();
        let id = Uuid::new_v4();
        let context = JobContext::default();
        let job = Job {
            id,
            state: JobState::Running,
            completed: 0,
            total: Some(ids.len() as u64),
            message: "Preparing portrait analysis".into(),
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
        let log_path = self.settings.analysis_log_path();
        std::thread::spawn(move || {
            let _running = running;
            let mut succeeded = 0;
            let mut failed = 0;
            let mut last_failure: Option<String> = None;
            let result = (|| -> Result<(), AppError> {
                if ids.is_empty() {
                    return Ok(());
                }
                let secret = token(&config.api_key_path)?;
                let mut authorization = HeaderValue::from_str(&format!("Bearer {secret}"))
                    .map_err(|_| {
                        error(
                            "ANALYSIS_KEY",
                            "The API-key file contains an invalid token.",
                        )
                    })?;
                authorization.set_sensitive(true);
                let client = Client::builder()
                    .timeout(Duration::from_secs(120))
                    .connect_timeout(Duration::from_secs(15))
                    .redirect(Policy::none())
                    .no_proxy()
                    .build()
                    .map_err(|_| {
                        error(
                            "ANALYSIS_TRANSPORT",
                            "Could not initialize the analysis connection.",
                        )
                    })?;
                for portrait_id in ids {
                    if context.is_cancelled() {
                        return Err(error("CANCELLED", "Analysis cancelled."));
                    }
                    state.analysis_progress(id, succeeded, failed, last_failure.as_deref());
                    let path = {
                        let guard = state.library.lock().map_err(state_error)?;
                        let lib = guard
                            .as_ref()
                            .filter(|lib| lib.id() == identity.0 && lib.root() == identity.1)
                            .ok_or_else(|| {
                                error(
                                    "ANALYSIS_LIBRARY_CHANGED",
                                    "Analysis stopped because the open library changed.",
                                )
                            })?;
                        portrait_core::thumbnails::asset_path(lib, portrait_id, Role::Large)
                    };
                    let path = match path {
                        Ok(path) => path,
                        Err(err) => {
                            failed += 1;
                            last_failure = Some(
                                log_failure(
                                    &log_path,
                                    id,
                                    portrait_id,
                                    &config.model,
                                    &secret,
                                    AnalysisFailure::new("ANALYSIS_IMAGE", "image_path", err),
                                )
                                .message,
                            );
                            continue;
                        }
                    };
                    // The SQLite/library mutex is deliberately released for file IO, encoding, and HTTP.
                    let response =
                        analyze(&client, &authorization, &secret, &config, &path, &context);
                    if context.is_cancelled() {
                        return Err(error("CANCELLED", "Analysis cancelled."));
                    }
                    let response = match response {
                        Ok(value) => value,
                        Err(failure) => {
                            let recoverable =
                                matches!(failure.code, "ANALYSIS_IMAGE" | "ANALYSIS_RESPONSE");
                            let failure = log_failure(
                                &log_path,
                                id,
                                portrait_id,
                                &config.model,
                                &secret,
                                failure,
                            );
                            failed += 1;
                            if !recoverable {
                                return Err(failure);
                            }
                            last_failure = Some(failure.message);
                            continue;
                        }
                    };
                    let mut guard = state.library.lock().map_err(state_error)?;
                    let lib = guard
                        .as_mut()
                        .filter(|lib| lib.id() == identity.0 && lib.root() == identity.1)
                        .ok_or_else(|| {
                            error(
                                "ANALYSIS_LIBRARY_CHANGED",
                                "Analysis stopped because the open library changed.",
                            )
                        })?;
                    if context.is_cancelled() {
                        return Err(error("CANCELLED", "Analysis cancelled."));
                    }
                    let save = save_analysis(lib, portrait_id, &response.analysis, &config.model);
                    drop(guard);
                    match save {
                        Ok(()) => succeeded += 1,
                        Err(err) => {
                            let recoverable = matches!(
                                err,
                                portrait_core::CoreError::InvalidModelAnalysis(_)
                                    | portrait_core::CoreError::PortraitNotFound
                            );
                            let failure = log_failure(
                                &log_path,
                                id,
                                portrait_id,
                                &config.model,
                                &secret,
                                AnalysisFailure::new("ANALYSIS_SAVE", "save", err)
                                    .response(response.status, &response.body),
                            );
                            failed += 1;
                            if !recoverable {
                                return Err(failure);
                            }
                            last_failure = Some(failure.message);
                        }
                    }
                    state.analysis_progress(id, succeeded, failed, last_failure.as_deref());
                }
                Ok(())
            })();
            if let Ok(mut jobs) = state.jobs.lock()
                && let Some(entry) = jobs.get_mut(&id)
            {
                entry.job.completed = succeeded + failed;
                let summary = format!(
                    "{succeeded} analyzed, {failed} failed{}",
                    last_failure
                        .as_ref()
                        .map(|reason| format!("; last failure: {reason}"))
                        .unwrap_or_default()
                );
                match result {
                    Ok(()) => {
                        entry.job.state = JobState::Done;
                        entry.job.message = if no_candidates {
                            "No portraits to analyze. Select active portraits or enable reanalysis for existing descriptions.".into()
                        } else {
                            format!("Analysis finished: {summary}.")
                        };
                    }
                    Err(failure) => {
                        entry.job.state = if failure.code == "CANCELLED" {
                            JobState::Cancelled
                        } else {
                            JobState::Failed
                        };
                        entry.job.message = format!("{} {summary}.", failure.message);
                    }
                }
            }
        });
        Ok(job)
    }
    fn analysis_progress(&self, id: Uuid, succeeded: u64, failed: u64, last_failure: Option<&str>) {
        if let Ok(mut jobs) = self.jobs.lock()
            && let Some(entry) = jobs.get_mut(&id)
        {
            entry.job.completed = succeeded + failed;
            entry.job.message = format!(
                "Analyzing portraits: {succeeded} analyzed, {failed} failed{}",
                last_failure
                    .map(|reason| format!("; last failure: {reason}"))
                    .unwrap_or_default()
            );
        }
    }
}

fn analyze(
    client: &Client,
    authorization: &HeaderValue,
    secret: &str,
    config: &AnalysisSettings,
    path: &std::path::Path,
    context: &JobContext,
) -> Result<AnalysisResponse, AnalysisFailure> {
    let bytes = fs::read(path).map_err(|err| {
        AnalysisFailure::new(
            "ANALYSIS_IMAGE",
            "image_read",
            format!("{}: {err}", path.display()),
        )
    })?;
    let image = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    );
    if context.is_cancelled() {
        return Err(AnalysisFailure::new(
            "CANCELLED",
            "cancel",
            "Analysis cancelled",
        ));
    }
    let mut response = client.post(&config.endpoint).header(AUTHORIZATION, authorization.clone()).json(&serde_json::json!({
        "model": config.model, "temperature": 0.1,
        "response_format": {"type":"json_object"}, "chat_template_kwargs":{"enable_thinking":false},
        "messages":[{"role":"user","content":[{"type":"text","text":PROMPT},{"type":"image_url","image_url":{"url":image}}]}]
    })).send().map_err(|err| AnalysisFailure::new("ANALYSIS_TRANSPORT", "http_request", format!("{err:#}")))?;
    let status = response.status();
    let mut bytes = Vec::new();
    if let Err(err) = response.read_to_end(&mut bytes) {
        return Err(AnalysisFailure::new("ANALYSIS_TRANSPORT", "http_read", err)
            .response(status.as_u16(), &String::from_utf8_lossy(&bytes)));
    }
    let body = String::from_utf8_lossy(&bytes).into_owned();
    if !status.is_success() {
        return Err(AnalysisFailure::new(
            if matches!(status.as_u16(), 401 | 403) {
                "ANALYSIS_AUTH"
            } else {
                "ANALYSIS_TRANSPORT"
            },
            "http_status",
            format!("Analysis server returned HTTP {status}"),
        )
        .response(status.as_u16(), &body));
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|err| {
        AnalysisFailure::new("ANALYSIS_RESPONSE", "response_json", err)
            .response(status.as_u16(), &body)
    })?;
    let content = value
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            AnalysisFailure::new(
                "ANALYSIS_RESPONSE",
                "response_content",
                "Missing string at choices[0].message.content",
            )
            .response(status.as_u16(), &body)
        })?;
    if content.contains(secret) {
        return Err(AnalysisFailure::new(
            "ANALYSIS_RESPONSE",
            "response_content",
            "Model response contains the API token",
        )
        .response(status.as_u16(), &body));
    }
    let content = content.trim();
    let content = content
        .strip_prefix("```json")
        .or_else(|| content.strip_prefix("```"))
        .and_then(|s| s.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(content);
    let analysis = serde_json::from_str(content).map_err(|err| {
        AnalysisFailure::new("ANALYSIS_RESPONSE", "metadata_json", err)
            .response(status.as_u16(), &body)
    })?;
    Ok(AnalysisResponse {
        analysis,
        status: status.as_u16(),
        body,
    })
}
