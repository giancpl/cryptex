use cryptex_core::{
    api::{API_VERSION, ApiError, FileTreePage, HealthResponse, ProjectSummary},
    project::{ProjectError, ProjectService},
};
use std::sync::Mutex;
use tauri::State;

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
    projects: State<'_, Mutex<ProjectService>>,
) -> Result<ProjectSummary, ApiError> {
    projects
        .lock()
        .map_err(|_| internal_error("project service lock is poisoned"))?
        .open(root)
        .map_err(project_error)
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

fn project_error(error: ProjectError) -> ApiError {
    let code = match &error {
        ProjectError::UnknownProject => "PROJECT_NOT_OPEN",
        ProjectError::NotDirectory => "NOT_A_DIRECTORY",
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
