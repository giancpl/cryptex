use cryptex_core::{
    api::{
        API_VERSION, ApiError, FileTreePage, HealthResponse, ProjectSummary, TextDocument,
        WriteResult,
    },
    project::{ProjectError, ProjectService},
    watcher::ProjectWatcher,
};
use std::{collections::HashMap, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Emitter, State};

pub type ProjectWatchers = Mutex<HashMap<String, ProjectWatcher>>;

#[tauri::command]
pub fn health() -> HealthResponse {
    HealthResponse {
        api_version: API_VERSION,
        application: "CrypTex".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

#[tauri::command]
pub fn open_project(
    root: String,
    app: AppHandle,
    projects: State<'_, Mutex<ProjectService>>,
    watchers: State<'_, ProjectWatchers>,
) -> Result<ProjectSummary, ApiError> {
    let summary = projects
        .lock()
        .map_err(|_| internal_error("project service lock is poisoned"))?
        .open(root)
        .map_err(project_error)?;
    let project_id = summary.project_id.clone();
    let emitted_project_id = project_id.clone();
    let watcher = ProjectWatcher::start(
        project_id.clone(),
        PathBuf::from(&summary.canonical_root),
        move |change| {
            let _ = app.emit("project-file-change", change);
        },
    )
    .map_err(|error| ApiError {
        api_version: API_VERSION,
        code: "WATCHER_ERROR".to_owned(),
        message: error.to_string(),
        retryable: true,
    })?;
    watchers
        .lock()
        .map_err(|_| internal_error("watcher service lock is poisoned"))?
        .insert(emitted_project_id, watcher);
    Ok(summary)
}

#[tauri::command(rename_all = "camelCase")]
pub fn list_directory(
    project_id: String,
    relative_path: String,
    projects: State<'_, Mutex<ProjectService>>,
) -> Result<FileTreePage, ApiError> {
    projects
        .lock()
        .map_err(|_| internal_error("project service lock is poisoned"))?
        .list_directory(&project_id, &relative_path)
        .map_err(project_error)
}

#[tauri::command(rename_all = "camelCase")]
pub fn read_text_file(
    project_id: String,
    relative_path: String,
    projects: State<'_, Mutex<ProjectService>>,
) -> Result<TextDocument, ApiError> {
    projects
        .lock()
        .map_err(|_| internal_error("project service lock is poisoned"))?
        .read_text_file(&project_id, &relative_path)
        .map_err(project_error)
}

#[tauri::command(rename_all = "camelCase")]
pub fn write_text_file(
    project_id: String,
    relative_path: String,
    text: String,
    expected_fingerprint: String,
    projects: State<'_, Mutex<ProjectService>>,
    watchers: State<'_, ProjectWatchers>,
) -> Result<WriteResult, ApiError> {
    if let Ok(watchers) = watchers.lock()
        && let Some(watcher) = watchers.get(&project_id)
    {
        watcher.expect_text_write(&relative_path, &text);
    }
    projects
        .lock()
        .map_err(|_| internal_error("project service lock is poisoned"))?
        .write_text_file(&project_id, &relative_path, &text, &expected_fingerprint)
        .map_err(project_error)
}

fn project_error(error: ProjectError) -> ApiError {
    let code = match &error {
        ProjectError::UnknownProject => "PROJECT_NOT_OPEN",
        ProjectError::NotDirectory => "NOT_A_DIRECTORY",
        ProjectError::NotFile => "NOT_A_FILE",
        ProjectError::FileTooLarge => "FILE_TOO_LARGE",
        ProjectError::BinaryFile => "BINARY_FILE",
        ProjectError::InvalidUtf8 => "INVALID_UTF8",
        ProjectError::StaleFingerprint => "STALE_FINGERPRINT",
        ProjectError::InvalidParent => "INVALID_PARENT",
        ProjectError::Path(_) => "UNSAFE_PROJECT_PATH",
        ProjectError::Io(_) => "FILESYSTEM_ERROR",
    };
    ApiError {
        api_version: API_VERSION,
        code: code.to_owned(),
        message: error.to_string(),
        retryable: matches!(error, ProjectError::Io(_)),
    }
}

fn internal_error(message: &str) -> ApiError {
    ApiError {
        api_version: API_VERSION,
        code: "INTERNAL_ERROR".to_owned(),
        message: message.to_owned(),
        retryable: false,
    }
}
