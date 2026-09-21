use crate::api::{EnginePreferenceState, RootPreferenceState, TrustState};
use cryptex_core::{
    api::{
        API_VERSION, ApiError, BuildLog, BuildOutput, BuildOutputStream, BuildPhase, BuildReason,
        BuildState, Diagnostic, OperationId,
    },
    build::resolve_build_configuration,
    compiler::{BuildRequestError, LATEXMK_EXECUTABLE, LatexmkRequest, LatexmkRequestBuilder},
    diagnostics::{parse_latex_log_file, read_latex_log_file},
    process::{OutputStream, ProcessLimits, ProcessOutcome, ProcessSupervisor},
    project::ProjectService,
    scheduler::{
        BuildScheduler, CancelDisposition, CompletionDisposition, ScheduleDisposition,
        ScheduledBuild,
    },
    toolchain::ToolchainService,
    trust::TrustService,
};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::Mutex,
    thread,
};
use tauri::{AppHandle, Emitter, Manager, State};

struct ExecutableBuild {
    request: LatexmkRequest,
    executable: PathBuf,
}

#[derive(Default)]
struct RuntimeState {
    scheduler: BuildScheduler<ExecutableBuild>,
    last_success: HashMap<String, OperationId>,
    raw_logs: HashMap<String, LogArtifact>,
}

struct LogArtifact {
    operation_id: OperationId,
    path: PathBuf,
}

pub struct BuildRuntime {
    request_builder: LatexmkRequestBuilder,
    state: Mutex<RuntimeState>,
}

impl BuildRuntime {
    pub fn new(cache_root: PathBuf) -> Result<Self, BuildRequestError> {
        Ok(Self {
            request_builder: LatexmkRequestBuilder::new(cache_root)?,
            state: Mutex::new(RuntimeState::default()),
        })
    }
}

#[tauri::command(rename_all = "camelCase")]
pub fn request_build(
    project_id: String,
    reason: BuildReason,
    app: AppHandle,
    runtime: State<'_, BuildRuntime>,
    projects: State<'_, Mutex<ProjectService>>,
    roots: State<'_, RootPreferenceState>,
    engines: State<'_, EnginePreferenceState>,
    trust: State<'_, TrustState>,
    toolchain: State<'_, ToolchainService>,
) -> Result<BuildState, ApiError> {
    let preferred_root = roots
        .lock()
        .map_err(|_| internal("root preference lock is poisoned"))?
        .get(&project_id)
        .map(str::to_owned);
    let preferred_engine = engines
        .lock()
        .map_err(|_| internal("engine preference lock is poisoned"))?
        .get(&project_id);
    let executable = toolchain
        .verified_executable(LATEXMK_EXECUTABLE)
        .map_err(|message| api_error("TOOLCHAIN_NOT_READY", message, true))?;
    let projects = projects
        .lock()
        .map_err(|_| internal("project service lock is poisoned"))?;
    let configuration = resolve_build_configuration(
        &projects,
        &project_id,
        preferred_root.as_deref(),
        preferred_engine,
    )
    .map_err(|error| api_error("BUILD_CONFIGURATION_ERROR", error.to_string(), false))?;
    let trust = trust
        .lock()
        .map_err(|_| internal("trust service lock is poisoned"))?;
    let mut state = runtime
        .state
        .lock()
        .map_err(|_| internal("build runtime lock is poisoned"))?;
    let request = runtime
        .request_builder
        .build(&projects, &trust, configuration.clone())
        .map_err(|error| api_error("BUILD_REQUEST_ERROR", error.to_string(), false))?;
    drop(trust);
    drop(projects);
    let last_success = state.last_success.get(&project_id).cloned();
    let disposition = state
        .scheduler
        .enqueue(
            project_id.clone(),
            reason,
            ExecutableBuild {
                request,
                executable,
            },
        )
        .map_err(|error| api_error("BUILD_SCHEDULER_ERROR", error.to_string(), false))?;

    match disposition {
        ScheduleDisposition::Started(build) => {
            let status = status_for(
                &build,
                BuildPhase::Running,
                last_success,
                None,
                None,
                false,
                false,
                Vec::new(),
                false,
                None,
            );
            drop(state);
            spawn_build(app, build);
            Ok(status)
        }
        ScheduleDisposition::Queued { operation_id, .. } => {
            let status = state_from_parts(
                &project_id,
                operation_id,
                BuildPhase::Queued,
                reason,
                &configuration.root_document,
                configuration.engine,
                last_success,
                None,
                None,
                false,
                false,
                Vec::new(),
                false,
                None,
            );
            drop(state);
            let _ = app.emit("build-state", status.clone());
            Ok(status)
        }
        ScheduleDisposition::Coalesced { into, .. } => {
            let status = state_from_parts(
                &project_id,
                into,
                BuildPhase::Queued,
                BuildReason::Explicit,
                &configuration.root_document,
                configuration.engine,
                last_success,
                None,
                None,
                false,
                false,
                Vec::new(),
                false,
                Some("Save request coalesced into the queued explicit build".to_owned()),
            );
            drop(state);
            Ok(status)
        }
    }
}

#[tauri::command(rename_all = "camelCase")]
pub fn cancel_build(
    project_id: String,
    operation_id: String,
    runtime: State<'_, BuildRuntime>,
) -> Result<bool, ApiError> {
    let disposition = runtime
        .state
        .lock()
        .map_err(|_| internal("build runtime lock is poisoned"))?
        .scheduler
        .cancel(&project_id, &OperationId(operation_id));
    Ok(!matches!(disposition, CancelDisposition::NotFound))
}

#[tauri::command(rename_all = "camelCase")]
pub fn clean_build_artifacts(
    project_id: String,
    runtime: State<'_, BuildRuntime>,
) -> Result<(), ApiError> {
    let mut state = runtime
        .state
        .lock()
        .map_err(|_| internal("build runtime lock is poisoned"))?;
    if state.scheduler.active_operation(&project_id).is_some() {
        return Err(api_error(
            "BUILD_ACTIVE",
            "build artifacts cannot be cleaned while a build is active".to_owned(),
            true,
        ));
    }
    runtime
        .request_builder
        .clean(&project_id)
        .map_err(|error| api_error("BUILD_CLEAN_ERROR", error.to_string(), true))?;
    state.last_success.remove(&project_id);
    state.raw_logs.remove(&project_id);
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub fn read_build_log(
    project_id: String,
    operation_id: String,
    runtime: State<'_, BuildRuntime>,
) -> Result<BuildLog, ApiError> {
    let state = runtime
        .state
        .lock()
        .map_err(|_| internal("build runtime lock is poisoned"))?;
    let artifact = state.raw_logs.get(&project_id).ok_or_else(|| {
        api_error(
            "BUILD_LOG_UNAVAILABLE",
            "no retained raw log is available for this project".to_owned(),
            false,
        )
    })?;
    if artifact.operation_id.0 != operation_id {
        return Err(api_error(
            "BUILD_LOG_UNAVAILABLE",
            "the requested raw log is no longer retained".to_owned(),
            false,
        ));
    }
    let path = runtime
        .request_builder
        .validate_artifact_file(&artifact.path)
        .map_err(|error| api_error("BUILD_LOG_UNAVAILABLE", error.to_string(), false))?;
    let retained_operation = artifact.operation_id.clone();
    drop(state);
    let (text, truncated) = read_latex_log_file(&path)
        .map_err(|error| api_error("BUILD_LOG_READ_ERROR", error.to_string(), true))?;
    Ok(BuildLog {
        api_version: API_VERSION,
        project_id,
        operation_id: retained_operation,
        text,
        truncated,
    })
}

fn spawn_build(app: AppHandle, build: ScheduledBuild<ExecutableBuild>) {
    let running = status_for(
        &build,
        BuildPhase::Running,
        last_success(&app, &build.project_id),
        None,
        None,
        false,
        false,
        Vec::new(),
        false,
        None,
    );
    let _ = app.emit("build-state", running);
    thread::spawn(move || run_build(app, build));
}

fn run_build(app: AppHandle, build: ScheduledBuild<ExecutableBuild>) {
    let project_id = build.project_id.clone();
    let operation_id = build.operation_id.clone();
    let reason = build.reason;
    let configuration = build.request.configuration.clone();
    let artifacts = build.request.artifacts.clone();
    let project_root = build.request.working_directory.clone();
    let supervisor = ProcessSupervisor::new(
        BTreeMap::from([(
            build.request.executable_name.clone(),
            build.executable.clone(),
        )]),
        vec![build.request.working_directory.clone()],
        ProcessLimits::default(),
    );
    let result = supervisor.and_then(|supervisor| {
        let output_app = app.clone();
        let output_project = project_id.clone();
        let output_operation = operation_id.clone();
        supervisor.run(
            &build.request.executable_name,
            &build.request.arguments,
            &build.request.working_directory,
            &build.cancellation,
            move |chunk| {
                let _ = output_app.emit(
                    "build-output",
                    BuildOutput {
                        api_version: API_VERSION,
                        project_id: output_project.clone(),
                        operation_id: output_operation.clone(),
                        stream: match chunk.stream {
                            OutputStream::Stdout => BuildOutputStream::Stdout,
                            OutputStream::Stderr => BuildOutputStream::Stderr,
                        },
                        text: String::from_utf8_lossy(&chunk.bytes).into_owned(),
                    },
                );
            },
        )
    });

    let (mut phase, elapsed_ms, exit_code, mut truncated, mut pdf_available, mut message) =
        match result {
            Ok(result) => (
                match result.outcome {
                    ProcessOutcome::Succeeded => BuildPhase::Succeeded,
                    ProcessOutcome::Failed => BuildPhase::Failed,
                    ProcessOutcome::Cancelled => BuildPhase::Cancelled,
                    ProcessOutcome::TimedOut => BuildPhase::TimedOut,
                },
                Some(result.elapsed.as_millis().min(u64::MAX as u128) as u64),
                result.exit_code,
                result.log_truncated,
                result.outcome == ProcessOutcome::Succeeded && artifacts.pdf.is_file(),
                None,
            ),
            Err(error) => (
                BuildPhase::Failed,
                None,
                None,
                false,
                false,
                Some(error.to_string()),
            ),
        };
    let runtime = app.state::<BuildRuntime>();
    let validated_log = runtime
        .request_builder
        .validate_artifact_file(&artifacts.log)
        .ok();
    let raw_log_available = validated_log.is_some();
    let parsed = if let Some(log) = validated_log {
        parse_latex_log_file(&log, &project_root, &configuration.root_document).unwrap_or_default()
    } else {
        Default::default()
    };
    truncated |= parsed.truncated;
    let diagnostics = parsed.diagnostics;

    if phase == BuildPhase::Succeeded && !pdf_available {
        phase = BuildPhase::Failed;
        pdf_available = false;
        message = Some("latexmk exited successfully but produced no complete PDF".to_owned());
    }

    let mut state = match runtime.state.lock() {
        Ok(state) => state,
        Err(_) => return,
    };
    let completion = state.scheduler.complete(&project_id, &operation_id);
    let CompletionDisposition::Accepted {
        publish_result,
        next,
    } = completion
    else {
        return;
    };
    if publish_result && phase == BuildPhase::Succeeded {
        state
            .last_success
            .insert(project_id.clone(), operation_id.clone());
    }
    if publish_result {
        if raw_log_available {
            state.raw_logs.insert(
                project_id.clone(),
                LogArtifact {
                    operation_id: operation_id.clone(),
                    path: artifacts.log.clone(),
                },
            );
        } else {
            state.raw_logs.remove(&project_id);
        }
    }
    let last_success = state.last_success.get(&project_id).cloned();
    drop(state);

    if publish_result {
        let status = state_from_parts(
            &project_id,
            operation_id,
            phase,
            reason,
            &configuration.root_document,
            configuration.engine,
            last_success,
            elapsed_ms,
            exit_code,
            truncated,
            raw_log_available,
            diagnostics,
            pdf_available,
            message,
        );
        let _ = app.emit("build-state", status);
    }
    if let Some(next) = next {
        spawn_build(app, next);
    }
}

fn last_success(app: &AppHandle, project_id: &str) -> Option<OperationId> {
    app.state::<BuildRuntime>()
        .state
        .lock()
        .ok()
        .and_then(|state| state.last_success.get(project_id).cloned())
}

fn status_for(
    build: &ScheduledBuild<ExecutableBuild>,
    phase: BuildPhase,
    last_success: Option<OperationId>,
    elapsed_ms: Option<u64>,
    exit_code: Option<i32>,
    log_truncated: bool,
    raw_log_available: bool,
    diagnostics: Vec<Diagnostic>,
    pdf_available: bool,
    message: Option<String>,
) -> BuildState {
    state_from_parts(
        &build.project_id,
        build.operation_id.clone(),
        phase,
        build.reason,
        &build.request.configuration.root_document,
        build.request.configuration.engine,
        last_success,
        elapsed_ms,
        exit_code,
        log_truncated,
        raw_log_available,
        diagnostics,
        pdf_available,
        message,
    )
}

#[allow(clippy::too_many_arguments)]
fn state_from_parts(
    project_id: &str,
    operation_id: OperationId,
    phase: BuildPhase,
    reason: BuildReason,
    root_document: &str,
    engine: cryptex_core::api::LatexEngine,
    last_successful_operation_id: Option<OperationId>,
    elapsed_ms: Option<u64>,
    exit_code: Option<i32>,
    log_truncated: bool,
    raw_log_available: bool,
    diagnostics: Vec<Diagnostic>,
    pdf_available: bool,
    message: Option<String>,
) -> BuildState {
    BuildState {
        api_version: API_VERSION,
        project_id: project_id.to_owned(),
        operation_id,
        phase,
        reason,
        root_document: root_document.to_owned(),
        engine,
        elapsed_ms,
        exit_code,
        log_truncated,
        raw_log_available,
        diagnostics,
        pdf_available,
        last_successful_operation_id,
        message,
    }
}

fn internal(message: &str) -> ApiError {
    api_error("INTERNAL_ERROR", message.to_owned(), false)
}

fn api_error(code: &str, message: String, retryable: bool) -> ApiError {
    ApiError {
        api_version: API_VERSION,
        code: code.to_owned(),
        message,
        retryable,
    }
}
