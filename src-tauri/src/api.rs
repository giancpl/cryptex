use cryptex_core::{
    api::{
        API_VERSION, ApiError, BuildConfiguration, BuildPermission, FileTreePage, HealthResponse,
        LatexEngine, ProjectSummary, ProjectTrustState, RecoveryInventory, RecoverySnapshot,
        RootDocumentCandidates, TextDocument, ToolchainReadiness, WriteResult,
    },
    build::{BuildResolutionError, resolve_build_configuration as resolve_configuration},
    catalog::{CommandCatalog, CommandContext},
    catalog_search::{CatalogSearchError, CatalogSearchHit, CatalogSearchQuery},
    index::{ProjectIndex, ScannerLimits, project::ProjectIndexer},
    project::{ProjectError, ProjectService},
    recovery::{RecoveryError, RecoveryService},
    settings::{EnginePreferences, RootPreferences, SettingsError},
    toolchain::ToolchainService,
    trust::{TrustError, TrustService},
    watcher::ProjectWatcher,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter, State};

pub type ProjectWatchers = Mutex<HashMap<String, ProjectWatcher>>;
pub type ProjectIndexes = Arc<Mutex<HashMap<String, ProjectIndexer>>>;
pub type RootPreferenceState = Mutex<RootPreferences>;
pub type EnginePreferenceState = Mutex<EnginePreferences>;
pub type TrustState = Mutex<TrustService>;

#[tauri::command]
pub fn health() -> HealthResponse {
    HealthResponse {
        api_version: API_VERSION,
        application: "CrypTex".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

#[tauri::command]
pub fn toolchain_readiness(toolchain: State<'_, ToolchainService>) -> ToolchainReadiness {
    toolchain.readiness()
}

#[tauri::command]
pub async fn open_project(
    root: String,
    app: AppHandle,
    projects: State<'_, Mutex<ProjectService>>,
    watchers: State<'_, ProjectWatchers>,
    indexes: State<'_, ProjectIndexes>,
) -> Result<ProjectSummary, ApiError> {
    let summary = projects
        .lock()
        .map_err(|_| internal_error("project service lock is poisoned"))?
        .open(root)
        .map_err(project_error)?;
    let project_id = summary.project_id.clone();
    let index_project_id = project_id.clone();
    let index_root = PathBuf::from(&summary.canonical_root);
    let indexer = tauri::async_runtime::spawn_blocking(move || {
        ProjectIndexer::open(index_project_id, index_root, ScannerLimits::default())
    })
    .await
    .map_err(|error| internal_error(&format!("project index worker failed: {error}")))?
    .map_err(index_error)?;
    indexes
        .lock()
        .map_err(|_| internal_error("project index lock is poisoned"))?
        .insert(project_id.clone(), indexer);
    let emitted_project_id = project_id.clone();
    let watcher_indexes = indexes.inner().clone();
    let watcher = ProjectWatcher::start(
        project_id.clone(),
        PathBuf::from(&summary.canonical_root),
        move |change| {
            if let Ok(mut indexes) = watcher_indexes.lock()
                && let Some(indexer) = indexes.get_mut(&change.project_id)
            {
                let _ = indexer.apply_change(&change);
            }
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
pub fn project_index(
    project_id: String,
    indexes: State<'_, ProjectIndexes>,
) -> Result<ProjectIndex, ApiError> {
    indexes
        .lock()
        .map_err(|_| internal_error("project index lock is poisoned"))?
        .get(&project_id)
        .map(ProjectIndexer::snapshot)
        .ok_or_else(|| ApiError {
            api_version: API_VERSION,
            code: "PROJECT_INDEX_UNAVAILABLE".to_owned(),
            message: "the project index is not available".to_owned(),
            retryable: true,
        })
}

#[tauri::command(rename_all = "camelCase")]
pub fn search_command_catalog(
    project_id: String,
    query: String,
    context: Option<CommandContext>,
    limit: u16,
    indexes: State<'_, ProjectIndexes>,
    catalog: State<'_, CommandCatalog>,
) -> Result<Vec<CatalogSearchHit>, ApiError> {
    let indexes = indexes
        .lock()
        .map_err(|_| internal_error("project index lock is poisoned"))?;
    let index = indexes.get(&project_id).ok_or_else(|| ApiError {
        api_version: API_VERSION,
        code: "PROJECT_INDEX_UNAVAILABLE".to_owned(),
        message: "the project index is not available".to_owned(),
        retryable: true,
    })?;
    let request = CatalogSearchQuery::from_project_index(query, context, limit, index);
    catalog.search(&request).map_err(catalog_search_error)
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

#[tauri::command(rename_all = "camelCase")]
pub fn detect_root_documents(
    project_id: String,
    projects: State<'_, Mutex<ProjectService>>,
    preferences: State<'_, RootPreferenceState>,
) -> Result<RootDocumentCandidates, ApiError> {
    let preferred = preferences
        .lock()
        .map_err(|_| internal_error("root preference lock is poisoned"))?
        .get(&project_id)
        .map(str::to_owned);
    projects
        .lock()
        .map_err(|_| internal_error("project service lock is poisoned"))?
        .detect_root_documents(&project_id, preferred.as_deref())
        .map_err(project_error)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_root_document(
    project_id: String,
    relative_path: String,
    projects: State<'_, Mutex<ProjectService>>,
    preferences: State<'_, RootPreferenceState>,
) -> Result<RootDocumentCandidates, ApiError> {
    let projects = projects
        .lock()
        .map_err(|_| internal_error("project service lock is poisoned"))?;
    let detected = projects
        .detect_root_documents(&project_id, None)
        .map_err(project_error)?;
    if !detected
        .candidates
        .iter()
        .any(|candidate| candidate.relative_path == relative_path)
    {
        return Err(ApiError {
            api_version: API_VERSION,
            code: "INVALID_ROOT_DOCUMENT".to_owned(),
            message: "root document must be one of the detected candidates".to_owned(),
            retryable: false,
        });
    }
    preferences
        .lock()
        .map_err(|_| internal_error("root preference lock is poisoned"))?
        .set(&project_id, &relative_path)
        .map_err(settings_error)?;
    projects
        .detect_root_documents(&project_id, Some(&relative_path))
        .map_err(project_error)
}

#[tauri::command(rename_all = "camelCase")]
pub fn resolve_build_configuration(
    project_id: String,
    projects: State<'_, Mutex<ProjectService>>,
    roots: State<'_, RootPreferenceState>,
    engines: State<'_, EnginePreferenceState>,
) -> Result<BuildConfiguration, ApiError> {
    let preferred_root = roots
        .lock()
        .map_err(|_| internal_error("root preference lock is poisoned"))?
        .get(&project_id)
        .map(str::to_owned);
    let preferred_engine = engines
        .lock()
        .map_err(|_| internal_error("engine preference lock is poisoned"))?
        .get(&project_id);
    let projects = projects
        .lock()
        .map_err(|_| internal_error("project service lock is poisoned"))?;
    resolve_configuration(
        &projects,
        &project_id,
        preferred_root.as_deref(),
        preferred_engine,
    )
    .map_err(build_resolution_error)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_project_engine(
    project_id: String,
    engine: Option<LatexEngine>,
    projects: State<'_, Mutex<ProjectService>>,
    engines: State<'_, EnginePreferenceState>,
) -> Result<(), ApiError> {
    projects
        .lock()
        .map_err(|_| internal_error("project service lock is poisoned"))?
        .require_open(&project_id)
        .map_err(project_error)?;
    engines
        .lock()
        .map_err(|_| internal_error("engine preference lock is poisoned"))?
        .set(&project_id, engine)
        .map_err(settings_error)
}

#[tauri::command(rename_all = "camelCase")]
pub fn project_trust(
    project_id: String,
    projects: State<'_, Mutex<ProjectService>>,
    trust: State<'_, TrustState>,
) -> Result<ProjectTrustState, ApiError> {
    projects
        .lock()
        .map_err(|_| internal_error("project service lock is poisoned"))?
        .require_open(&project_id)
        .map_err(project_error)?;
    trust
        .lock()
        .map_err(|_| internal_error("trust service lock is poisoned"))?
        .state(&project_id)
        .map_err(trust_error)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_project_permission(
    project_id: String,
    permission: BuildPermission,
    allowed: bool,
    projects: State<'_, Mutex<ProjectService>>,
    trust: State<'_, TrustState>,
) -> Result<ProjectTrustState, ApiError> {
    projects
        .lock()
        .map_err(|_| internal_error("project service lock is poisoned"))?
        .require_open(&project_id)
        .map_err(project_error)?;
    trust
        .lock()
        .map_err(|_| internal_error("trust service lock is poisoned"))?
        .set(&project_id, permission, allowed)
        .map_err(trust_error)
}

#[tauri::command(rename_all = "camelCase")]
pub fn revoke_project_trust(
    project_id: String,
    projects: State<'_, Mutex<ProjectService>>,
    trust: State<'_, TrustState>,
) -> Result<ProjectTrustState, ApiError> {
    projects
        .lock()
        .map_err(|_| internal_error("project service lock is poisoned"))?
        .require_open(&project_id)
        .map_err(project_error)?;
    trust
        .lock()
        .map_err(|_| internal_error("trust service lock is poisoned"))?
        .revoke(&project_id)
        .map_err(trust_error)
}

#[tauri::command(rename_all = "camelCase")]
pub fn store_recovery_snapshot(
    project_id: String,
    relative_path: String,
    text: String,
    base_fingerprint: String,
    revision: u64,
    recovery: State<'_, RecoveryService>,
) -> Result<RecoverySnapshot, ApiError> {
    recovery
        .store(
            &project_id,
            &relative_path,
            &text,
            &base_fingerprint,
            revision,
        )
        .map_err(recovery_error)
}

#[tauri::command(rename_all = "camelCase")]
pub fn list_recovery_snapshots(
    project_id: String,
    recovery: State<'_, RecoveryService>,
) -> Result<RecoveryInventory, ApiError> {
    recovery.list(&project_id).map_err(recovery_error)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_recovery_snapshot(
    project_id: String,
    relative_path: String,
    recovery: State<'_, RecoveryService>,
) -> Result<(), ApiError> {
    recovery
        .delete(&project_id, &relative_path)
        .map_err(recovery_error)
}

fn build_resolution_error(error: BuildResolutionError) -> ApiError {
    let code = match &error {
        BuildResolutionError::NoRootDocument => "NO_ROOT_DOCUMENT",
        BuildResolutionError::AmbiguousRootDocument => "AMBIGUOUS_ROOT_DOCUMENT",
        BuildResolutionError::UnsupportedEngine(_) => "UNSUPPORTED_TEX_ENGINE",
        BuildResolutionError::Project(_) => "BUILD_PROJECT_ERROR",
    };
    ApiError {
        api_version: API_VERSION,
        code: code.to_owned(),
        message: error.to_string(),
        retryable: false,
    }
}

fn recovery_error(error: RecoveryError) -> ApiError {
    let retryable = matches!(&error, RecoveryError::Io(_));
    ApiError {
        api_version: API_VERSION,
        code: "RECOVERY_ERROR".to_owned(),
        message: error.to_string(),
        retryable,
    }
}
fn trust_error(error: TrustError) -> ApiError {
    let retryable = matches!(&error, TrustError::Io(_));
    ApiError {
        api_version: API_VERSION,
        code: "TRUST_ERROR".to_owned(),
        message: error.to_string(),
        retryable,
    }
}
fn settings_error(error: SettingsError) -> ApiError {
    let retryable = matches!(&error, SettingsError::Io(_));
    ApiError {
        api_version: API_VERSION,
        code: "SETTINGS_ERROR".to_owned(),
        message: error.to_string(),
        retryable,
    }
}

fn catalog_search_error(error: CatalogSearchError) -> ApiError {
    ApiError {
        api_version: API_VERSION,
        code: "INVALID_CATALOG_SEARCH".to_owned(),
        message: error.to_string(),
        retryable: false,
    }
}

fn index_error(error: cryptex_core::index::project::ProjectIndexError) -> ApiError {
    ApiError {
        api_version: API_VERSION,
        code: "PROJECT_INDEX_ERROR".to_owned(),
        message: error.to_string(),
        retryable: true,
    }
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
