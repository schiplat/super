use crate::client::ManagerHandle;
use crate::config::ServerConfig;
use axum::response::Response;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{
        DefaultBodyLimit, Extension, FromRequest, Path, Query, Request, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
};

use common::{
    ArtifactConfig, BatchAction, BatchProgramRequest, BatchProgramResponse, CreateProgramRequest,
    DiskPartitionStats, HealthCheck, HealthResponse, LicenseInfo, ProcessStatus, ProgramConfig,
    ProgramHooks, ProgramInfo, ProgramLogFile, ProgramLogsResponse, ProgramSummary, ReloadResponse,
    SignalProgramRequest, StackApplyRequest, SystemStats, UpdateProgramRequest, UserContext,
    UserRole, WsMessage, mask_env_map,
};

use crate::logger::{self, LogSource};

use nix::sys::signal::Signal;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::sync::broadcast;
use uuid::Uuid;

use utoipa::{IntoParams, OpenApi};
use utoipa_swagger_ui::SwaggerUi;

/// Maximum JSON body size for mutating API routes (4 MiB).
const API_BODY_LIMIT: usize = 4 * 1024 * 1024;

// App State
#[derive(Clone)]
pub struct AppState {
    pub manager: ManagerHandle,
    pub log_tx: broadcast::Sender<WsMessage>,
    // Channel to notify main thread of shutdown
    pub shutdown_tx: broadcast::Sender<()>,
    pub config: ServerConfig,
    /// Verified license + loaded plugin versions (None in OSS mode).
    pub license: Option<LicenseInfo>,
}

#[derive(Debug)]
pub struct AppError(pub StatusCode, pub anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let payload = serde_json::json!({
            "status": "error",
            "message": self.1.to_string()
        });
        (self.0, Json(payload)).into_response()
    }
}

fn map_program_mutation_error(default: StatusCode, err: anyhow::Error) -> AppError {
    let msg = err.to_string();
    if msg.contains("already exists") || msg.contains("Duplicate program name") {
        AppError(StatusCode::CONFLICT, err)
    } else {
        AppError(default, err)
    }
}

/// JSON body with field path + line:column in the 400 message.
struct LocatedJson<T>(T);

impl<S, T> FromRequest<S> for LocatedJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req, state).await.map_err(|e| {
            AppError(
                StatusCode::BAD_REQUEST,
                anyhow::anyhow!("request body: {e}"),
            )
        })?;
        let mut de = serde_json::Deserializer::from_slice(&bytes);
        match serde_path_to_error::deserialize(&mut de) {
            Ok(value) => Ok(LocatedJson(value)),
            Err(err) => {
                let path = err.path().to_string();
                let inner = err.inner();
                let loc = format!("line {} column {}", inner.line(), inner.column());
                let msg = if path.is_empty() || path == "." {
                    format!("JSON {loc}: {inner}")
                } else {
                    format!("{path} (JSON {loc}): {inner}")
                };
                Err(AppError(StatusCode::BAD_REQUEST, anyhow::anyhow!("{msg}")))
            }
        }
    }
}

/// Stack body: JSON by default, TOML when `Content-Type` is `application/toml`
/// (or any `…toml` subtype). Parse errors report `path:line:col:` like `super check`.
#[derive(Debug)]
struct StackBody(pub StackApplyRequest);

impl<S> FromRequest<S> for StackBody
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let is_toml = req
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.to_ascii_lowercase().contains("toml"));
        let bytes = Bytes::from_request(req, state).await.map_err(|e| {
            AppError(
                StatusCode::BAD_REQUEST,
                anyhow::anyhow!("request body: {e}"),
            )
        })?;

        if is_toml {
            let content = std::str::from_utf8(&bytes).map_err(|e| {
                AppError(
                    StatusCode::BAD_REQUEST,
                    anyhow::anyhow!("request body: not valid UTF-8: {e}"),
                )
            })?;
            let stack = toml::from_str::<StackApplyRequest>(content).map_err(|e| {
                AppError(
                    StatusCode::BAD_REQUEST,
                    anyhow::anyhow!(common::format_toml_error("request body", content, &e)),
                )
            })?;
            Ok(StackBody(stack))
        } else {
            let mut de = serde_json::Deserializer::from_slice(&bytes);
            match serde_path_to_error::deserialize(&mut de) {
                Ok(value) => Ok(StackBody(value)),
                Err(err) => {
                    let path = err.path().to_string();
                    let inner = err.inner();
                    let loc = format!("line {} column {}", inner.line(), inner.column());
                    let msg = if path.is_empty() || path == "." {
                        format!("JSON {loc}: {inner}")
                    } else {
                        format!("{path} (JSON {loc}): {inner}")
                    };
                    Err(AppError(StatusCode::BAD_REQUEST, anyhow::anyhow!("{msg}")))
                }
            }
        }
    }
}

#[derive(Deserialize, IntoParams)]
struct StopParams {
    force: Option<bool>,
}

#[derive(Deserialize, IntoParams)]
struct LogQueryParams {
    /// Number of lines to return from the end of each log file (default 200, max 5000)
    tail: Option<u32>,
    /// Log stream: stdout, stderr, or omit for both
    source: Option<String>,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_programs,
        create_program,
        get_program_info,
        get_program_logs,
        update_program,
        start_program,
        stop_program,
        restart_program,
        remove_program,
        signal_program,
        batch_programs,

        start_group,
        stop_group,
        restart_group,

        health_check,
        apply_stack,
        export_stack,
        system_reload,
        system_shutdown,
        system_stats,
        system_license,
        metrics_handler,
        query_events,
        event_stats
    ),
    components(
        schemas(
            ProgramSummary,
            ProgramInfo,
            ProgramConfig,
            CreateProgramRequest,
            UpdateProgramRequest,
            ProcessStatus,
            HealthResponse,
            StackApplyRequest,
            SignalProgramRequest,
            HealthCheck,
            ProgramHooks,
            ArtifactConfig,
            BatchProgramRequest,
            BatchProgramResponse,
            BatchAction,
            ProgramLogsResponse,
            ProgramLogFile,
            SystemStats,
            DiskPartitionStats,
            LicenseInfo,
            common::AutorestartPolicy,
            common::EventStats,
        )
    ),
    tags(
        (name = "programs", description = "Process lifecycle management"),
        (name = "groups", description = "Group lifecycle management"),
        (name = "system", description = "System operations"),
        (name = "stack", description = "Declarative stack operations"),
        (name = "events", description = "Persisted event history")
    ),
    info(
        title = "Project Super API",
        version = env!("CARGO_PKG_VERSION"),
        description = "High-performance Process Manager API"
    )
)]
pub struct ApiDoc;

// Router Definition
pub fn make_api_router(
    manager: ManagerHandle,
    log_tx: broadcast::Sender<WsMessage>,
    shutdown_tx: broadcast::Sender<()>,
    config: ServerConfig,
    mount_docs: bool,
    license: Option<LicenseInfo>,
) -> Router {
    let state = AppState {
        manager,
        log_tx,
        shutdown_tx,
        config: config.clone(),
        license,
    };

    // Business APIs under `/api/v1`; `/health` at root; `/ws` + `/metrics` stay root.
    let v1 = Router::new()
        .route("/programs", get(list_programs).post(create_program))
        .route(
            "/programs/{id}",
            get(get_program_info)
                .delete(remove_program)
                .put(update_program),
        )
        .route("/programs/{id}/logs", get(get_program_logs))
        .route("/programs/{id}/events", get(get_program_events))
        .route("/events", get(query_events))
        .route("/events/stats", get(event_stats))
        .route("/programs/{id}/start", post(start_program))
        .route("/programs/{id}/stop", post(stop_program))
        .route("/programs/{id}/restart", post(restart_program))
        .route("/programs/{id}/signal", post(signal_program))
        .route("/programs/batch", post(batch_programs))
        .route("/groups/{name}/start", post(start_group))
        .route("/groups/{name}/stop", post(stop_group))
        .route("/groups/{name}/restart", post(restart_group))
        .route("/stack", put(apply_stack).get(export_stack))
        .route("/system/shutdown", post(system_shutdown))
        .route("/system/reload", post(system_reload))
        .route("/system/stats", get(system_stats))
        .route("/system/license", get(system_license));

    let mut api_router = Router::new()
        .route("/health", get(health_check))
        .nest("/api/v1", v1)
        .route("/ws", get(ws_handler))
        .route("/metrics", get(metrics_handler))
        .layer(DefaultBodyLimit::max(API_BODY_LIMIT))
        .with_state(state);

    #[cfg(feature = "docs")]
    {
        if config.server.enable_docs && mount_docs {
            let openapi = ApiDoc::openapi();
            api_router =
                api_router.merge(SwaggerUi::new("/api/docs").url("/api/v1/openapi.json", openapi));
        }
    }

    api_router
}

/// System Shutdown
#[utoipa::path(
    post,
    path = "/api/v1/system/shutdown",
    tag = "system",
    responses(
        (status = 200, description = "Shutdown initiated"),
        (status = 500, description = "Shutdown failed")
    )
)]
async fn system_shutdown(State(state): State<AppState>) -> Result<StatusCode, AppError> {
    tracing::info!("API received shutdown signal. Initiating graceful shutdown sequence...");
    state
        .manager
        .shutdown()
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let _ = state.shutdown_tx.send(());
    tracing::info!("Graceful shutdown completed. Server exiting.");
    Ok(StatusCode::OK)
}

#[derive(Deserialize, IntoParams)]
struct WsParams {
    id: Option<Uuid>,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsParams>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state.log_tx, params.id))
}

async fn handle_socket(
    mut socket: WebSocket,
    log_tx: broadcast::Sender<WsMessage>,
    filter_id: Option<Uuid>,
) {
    let mut rx = log_tx.subscribe();
    loop {
        tokio::select! {
            // Client frames (heartbeat ping / close). Must be drained or the socket stalls.
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {} // ignore ping/pong/text/binary from client
                    Some(Err(_)) => break,
                }
            }
            // Broadcast log/status. Lagged must not tear down the socket.
            result = rx.recv() => {
                let msg = match result {
                    Ok(msg) => msg,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                let should_send = match &msg {
                    WsMessage::Log { id, .. } => filter_id.is_none_or(|target| target == *id),
                    WsMessage::StatusChange { .. } => true,
                };
                if should_send
                    && let Ok(json) = serde_json::to_string(&msg)
                    && socket.send(Message::Text(json.into())).await.is_err()
                {
                    break;
                }
            }
        }
    }
}

/// List all programs
#[utoipa::path(
    get,
    path = "/api/v1/programs",
    tag = "programs",
    responses(
        (status = 200, description = "List successfully retrieved", body = Vec<ProgramSummary>),
        (status = 500, description = "Internal server error")
    )
)]
async fn list_programs(
    State(state): State<AppState>,
) -> Result<Json<Vec<ProgramSummary>>, AppError> {
    let list = state
        .manager
        .list_programs()
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(list))
}

/// Create a new program
#[utoipa::path(
    post,
    path = "/api/v1/programs",
    tag = "programs",
    request_body = CreateProgramRequest,
    responses(
        (status = 201, description = "Program created", body = Vec<Uuid>),
        (status = 400, description = "Invalid program body (message names the field / JSON line:column)"),
        (status = 409, description = "Program name already exists"),
        (status = 500, description = "Server error")
    )
)]
async fn create_program(
    State(state): State<AppState>,
    LocatedJson(payload): LocatedJson<CreateProgramRequest>,
) -> Result<(StatusCode, Json<Vec<Uuid>>), AppError> {
    let ids = state
        .manager
        .create_program(payload)
        .await
        .map_err(|e| map_program_mutation_error(StatusCode::BAD_REQUEST, e))?;
    if ids.is_empty() {
        return Err(map_program_mutation_error(
            StatusCode::CONFLICT,
            anyhow::anyhow!("No program was created"),
        ));
    }
    Ok((StatusCode::CREATED, Json(ids)))
}

/// Get program details
#[utoipa::path(
    get,
    path = "/api/v1/programs/{id}",
    tag = "programs",
    params(
        ("id" = Uuid, Path, description = "Program ID")
    ),
    responses(
        (status = 200, description = "Program details", body = ProgramInfo),
        (status = 404, description = "Program not found")
    )
)]
async fn get_program_info(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ProgramInfo>, AppError> {
    let mut info = state
        .manager
        .get_program(id)
        .await
        .map_err(|e| AppError(StatusCode::NOT_FOUND, e))?;
    info.config.env = mask_env_map(&info.config.env);
    Ok(Json(info))
}

/// Read historical log lines from disk
#[utoipa::path(
    get,
    path = "/api/v1/programs/{id}/logs",
    tag = "programs",
    params(
        ("id" = Uuid, Path, description = "Program ID"),
        LogQueryParams
    ),
    responses(
        (status = 200, description = "Historical logs", body = ProgramLogsResponse),
        (status = 404, description = "Program not found")
    )
)]
async fn get_program_logs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<LogQueryParams>,
) -> Result<Json<ProgramLogsResponse>, AppError> {
    let info = state
        .manager
        .get_program(id)
        .await
        .map_err(|e| AppError(StatusCode::NOT_FOUND, e))?;

    let tail_lines = params.tail.unwrap_or(200).clamp(1, 5000);
    let log_dir = &state.config.storage.log_dir;
    let stdout_logfile = info.config.stdout_logfile.as_deref();
    let stderr_logfile = info.config.stderr_logfile.as_deref();

    let sources: Vec<LogSource> = match params.source.as_deref() {
        Some("stdout") => vec![LogSource::Stdout],
        Some("stderr") => vec![LogSource::Stderr],
        None => vec![LogSource::Stdout, LogSource::Stderr],
        _ => {
            return Err(AppError(
                StatusCode::BAD_REQUEST,
                anyhow::anyhow!("source must be stdout or stderr"),
            ));
        }
    };

    let mut logs = Vec::new();
    for source in sources {
        if let Some(content) = logger::read_log_lines(
            log_dir,
            id,
            source,
            tail_lines,
            stdout_logfile,
            stderr_logfile,
        )
        .await
        {
            logs.push(ProgramLogFile {
                source: source.as_str().to_string(),
                content,
            });
        }
    }

    Ok(Json(ProgramLogsResponse { id, logs }))
}

/// Read persisted lifecycle/exception event history for a program.
///
/// Optional query filters (all combinable):
/// - `from` / `to`: Unix-seconds inclusive time window
/// - `event_type`: exact event type (e.g. `process_fatal`, `cron_exit`)
/// - `exit_code`: exact exit code
/// - `q`: free-text match on the message
/// - `limit` / `offset`: pagination
/// - `sort_by`: `time` (default) · `event` · `exit_code` · `signal` · `retry_count` · `duration_secs` · `msg`
/// - `order`: `asc` (default) or `desc`
#[utoipa::path(
    get,
    path = "/api/v1/programs/{id}/events",
    tag = "programs",
    params(
        ("id" = Uuid, Path, description = "Program ID"),
        ("from" = Option<u64>, Query, description = "Inclusive lower bound (Unix seconds)"),
        ("to" = Option<u64>, Query, description = "Inclusive upper bound (Unix seconds)"),
        ("event_type" = Option<String>, Query, description = "Exact event type"),
        ("exit_code" = Option<i32>, Query, description = "Exact exit code"),
        ("q" = Option<String>, Query, description = "Free-text match on message"),
        ("limit" = Option<u32>, Query, description = "Max rows"),
        ("offset" = Option<u32>, Query, description = "Pagination offset"),
        ("sort_by" = Option<String>, Query, description = "time|event|exit_code|signal|retry_count|duration_secs|msg"),
        ("order" = Option<String>, Query, description = "asc (default) or desc"),
    ),
    responses(
        (status = 200, description = "Event history", body = Vec<common::ProgramEventRecord>),
        (status = 404, description = "Program not found")
    )
)]
async fn get_program_events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<EventQueryParams>,
) -> Result<Json<Vec<common::ProgramEventRecord>>, AppError> {
    // 404 if the program does not exist
    state
        .manager
        .get_program(id)
        .await
        .map_err(|e| AppError(StatusCode::NOT_FOUND, e))?;
    let query = params.into_event_query(Some(id));
    let events = state
        .manager
        .query_events(query)
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(events))
}

/// Query persisted events across the whole daemon (optionally scoped to a
/// program via `program_id`).
#[utoipa::path(
    get,
    path = "/api/v1/events",
    tag = "events",
    params(
        ("program_id" = Option<Uuid>, Query, description = "Scope to one program"),
        ("from" = Option<u64>, Query, description = "Inclusive lower bound (Unix seconds)"),
        ("to" = Option<u64>, Query, description = "Inclusive upper bound (Unix seconds)"),
        ("event_type" = Option<String>, Query, description = "Exact event type"),
        ("exit_code" = Option<i32>, Query, description = "Exact exit code"),
        ("q" = Option<String>, Query, description = "Free-text match on message"),
        ("limit" = Option<u32>, Query, description = "Max rows"),
        ("offset" = Option<u32>, Query, description = "Pagination offset"),
        ("sort_by" = Option<String>, Query, description = "time|event|exit_code|signal|retry_count|duration_secs|msg"),
        ("order" = Option<String>, Query, description = "asc (default) or desc"),
    ),
    responses(
        (status = 200, description = "Event history", body = Vec<common::ProgramEventRecord>)
    )
)]
async fn query_events(
    State(state): State<AppState>,
    Query(params): Query<EventQueryParams>,
) -> Result<Json<Vec<common::ProgramEventRecord>>, AppError> {
    let program_id = params.program_id;
    let query = params.into_event_query(program_id);
    let events = state
        .manager
        .query_events(query)
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(events))
}

/// Event retention statistics (optional `program_id` scope).
#[utoipa::path(
    get,
    path = "/api/v1/events/stats",
    tag = "events",
    params(
        ("program_id" = Option<Uuid>, Query, description = "Scope to one program")
    ),
    responses(
        (status = 200, description = "Event statistics", body = common::EventStats)
    )
)]
async fn event_stats(
    State(state): State<AppState>,
    Query(params): Query<EventStatsParams>,
) -> Result<Json<common::EventStats>, AppError> {
    let stats = state
        .manager
        .event_stats(params.program_id)
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(stats))
}

/// Query-string parameters shared by the event listing endpoints.
#[derive(Debug, Deserialize)]
struct EventQueryParams {
    program_id: Option<Uuid>,
    from: Option<u64>,
    to: Option<u64>,
    event_type: Option<String>,
    exit_code: Option<i32>,
    q: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    /// `time` (default) · `event` · `exit_code` · `signal` · `retry_count` · `duration_secs` · `msg`
    sort_by: Option<String>,
    /// `asc` (default) or `desc`.
    order: Option<String>,
}

impl EventQueryParams {
    fn into_event_query(self, program_id: Option<Uuid>) -> crate::event_db::EventQuery {
        let sort_desc = self
            .order
            .as_deref()
            .is_some_and(|o| o.eq_ignore_ascii_case("desc"));
        crate::event_db::EventQuery {
            program_id,
            from: self.from,
            to: self.to,
            event_type: self.event_type,
            exit_code: self.exit_code,
            q: self.q,
            limit: self.limit,
            offset: self.offset,
            sort_by: crate::event_db::EventSortBy::parse(self.sort_by.as_deref()),
            sort_desc,
        }
    }
}

/// Query-string parameters for the statistics endpoint.
#[derive(Debug, Deserialize)]
struct EventStatsParams {
    program_id: Option<Uuid>,
}

/// Start a program
#[utoipa::path(
    post,
    path = "/api/v1/programs/{id}/start",
    tag = "programs",
    params(
        ("id" = Uuid, Path, description = "Program ID")
    ),
    responses(
        (status = 200, description = "Program started"),
        (status = 400, description = "Bad request")
    )
)]
async fn start_program(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state
        .manager
        .start_program(id)
        .await
        .map_err(|e| AppError(StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::OK)
}

/// Update program configuration
#[utoipa::path(
    put,
    path = "/api/v1/programs/{id}",
    tag = "programs",
    params(
        ("id" = Uuid, Path, description = "Program ID")
    ),
    request_body = UpdateProgramRequest,
    responses(
        (status = 200, description = "Program updated"),
        (status = 400, description = "Invalid program body (message names the field / JSON line:column)")
    )
)]
async fn update_program(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    LocatedJson(payload): LocatedJson<UpdateProgramRequest>,
) -> Result<StatusCode, AppError> {
    state
        .manager
        .update_program(id, payload)
        .await
        .map_err(|e| map_program_mutation_error(StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::OK)
}

/// Stop a program
#[utoipa::path(
    post,
    path = "/api/v1/programs/{id}/stop",
    tag = "programs",
    params(
        ("id" = Uuid, Path, description = "Program ID"),
        StopParams
    ),
    responses(
        (status = 200, description = "Program stopped"),
        (status = 400, description = "Bad request")
    )
)]
async fn stop_program(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<StopParams>,
) -> Result<StatusCode, AppError> {
    let force = params.force.unwrap_or(false);
    state
        .manager
        .stop_program(id, force)
        .await
        .map_err(|e| AppError(StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::OK)
}

/// Restart a program
#[utoipa::path(
    post,
    path = "/api/v1/programs/{id}/restart",
    tag = "programs",
    params(
        ("id" = Uuid, Path, description = "Program ID")
    ),
    responses(
        (status = 200, description = "Program restarted"),
        (status = 500, description = "Internal error")
    )
)]
async fn restart_program(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state
        .manager
        .restart_program(id)
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(StatusCode::OK)
}

/// Remove a program
#[utoipa::path(
    delete,
    path = "/api/v1/programs/{id}",
    tag = "programs",
    params(
        ("id" = Uuid, Path, description = "Program ID")
    ),
    responses(
        (status = 200, description = "Program removed"),
        (status = 400, description = "Bad request")
    )
)]
async fn remove_program(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state
        .manager
        .remove_program(id)
        .await
        .map_err(|e| AppError(StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::OK)
}

/// Batch operations
#[utoipa::path(
    post,
    path = "/api/v1/programs/batch",
    tag = "programs",
    request_body = BatchProgramRequest,
    responses(
        (status = 200, description = "Batch operation result", body = BatchProgramResponse),
        (status = 500, description = "Internal error")
    )
)]
async fn batch_programs(
    State(state): State<AppState>,
    Json(payload): Json<BatchProgramRequest>,
) -> Result<Json<BatchProgramResponse>, AppError> {
    // Use ManagerHandle wrapper; avoid direct tx access
    let res = state
        .manager
        .batch_programs(payload)
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(res))
}

/// Start a group of programs
#[utoipa::path(
    post,
    path = "/api/v1/groups/{name}/start",
    tag = "groups",
    params(
        ("name" = String, Path, description = "Group Name")
    ),
    responses(
        (status = 200, description = "Group started", body = Vec<Uuid>),
        (status = 404, description = "Group not found")
    )
)]
async fn start_group(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<Uuid>>, AppError> {
    let ids = state
        .manager
        .start_group(name)
        .await
        .map_err(|e| AppError(StatusCode::NOT_FOUND, e))?;
    Ok(Json(ids))
}

/// Stop a group of programs
#[utoipa::path(
    post,
    path = "/api/v1/groups/{name}/stop",
    tag = "groups",
    params(
        ("name" = String, Path, description = "Group Name"),
        StopParams
    ),
    responses(
        (status = 200, description = "Group stopped", body = Vec<Uuid>),
        (status = 404, description = "Group not found")
    )
)]
async fn stop_group(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(params): Query<StopParams>,
) -> Result<Json<Vec<Uuid>>, AppError> {
    let force = params.force.unwrap_or(false);
    let ids = state
        .manager
        .stop_group(name, force)
        .await
        .map_err(|e| AppError(StatusCode::NOT_FOUND, e))?;
    Ok(Json(ids))
}

/// Restart a group of programs
#[utoipa::path(
    post,
    path = "/api/v1/groups/{name}/restart",
    tag = "groups",
    params(
        ("name" = String, Path, description = "Group Name")
    ),
    responses(
        (status = 200, description = "Group restarted", body = Vec<Uuid>),
        (status = 404, description = "Group not found")
    )
)]
async fn restart_group(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<Uuid>>, AppError> {
    let ids = state
        .manager
        .restart_group(name)
        .await
        .map_err(|e| AppError(StatusCode::NOT_FOUND, e))?;
    Ok(Json(ids))
}

/// Reload System Config
#[utoipa::path(
    post,
    path = "/api/v1/system/reload",
    tag = "system",
    responses(
        (status = 200, description = "Configuration reloaded", body = ReloadResponse),
        (status = 500, description = "Reload failed")
    )
)]
async fn system_reload(
    State(state): State<AppState>,
    Query(params): Query<ReloadQuery>,
) -> Result<Json<ReloadResponse>, AppError> {
    let resp = state
        .manager
        .reload(params.wait, params.timeout)
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(resp))
}

#[derive(serde::Deserialize)]
pub struct ReloadQuery {
    /// Wait for affected programs to become Healthy before responding.
    #[serde(default)]
    wait: bool,
    /// Readiness wait timeout in seconds (default: 30).
    #[serde(default = "default_reload_timeout")]
    timeout: u64,
}

fn default_reload_timeout() -> u64 {
    30
}

/// Host-level CPU and memory snapshot
#[utoipa::path(
    get,
    path = "/api/v1/system/stats",
    tag = "system",
    responses(
        (status = 200, description = "System stats", body = SystemStats),
    )
)]
async fn system_stats(State(state): State<AppState>) -> Result<Json<SystemStats>, AppError> {
    let stats = state
        .manager
        .get_system_stats()
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(stats))
}

/// Verified subscription license (requires auth when security plugin is loaded).
#[utoipa::path(
    get,
    path = "/api/v1/system/license",
    tag = "system",
    responses(
        (status = 200, description = "License info", body = LicenseInfo),
        (status = 404, description = "No license configured"),
    )
)]
async fn system_license(State(state): State<AppState>) -> Result<Json<LicenseInfo>, AppError> {
    state
        .license
        .clone()
        .ok_or_else(|| {
            AppError(
                StatusCode::NOT_FOUND,
                anyhow::anyhow!("No license configured"),
            )
        })
        .map(Json)
}

/// System Health Check
#[utoipa::path(
    get,
    path = "/health",
    tag = "system",
    responses(
        (status = 200, description = "System healthy", body = HealthResponse),
        (status = 503, description = "System degraded", body = HealthResponse)
    )
)]
async fn health_check(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let res = state
        .manager
        .health_check()
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    if res.status == "healthy" {
        Ok((StatusCode::OK, Json(res)))
    } else {
        Ok((StatusCode::SERVICE_UNAVAILABLE, Json(res)))
    }
}

/// Apply Stack
#[utoipa::path(
    put,
    path = "/api/v1/stack",
    tag = "stack",
    request_body(
        content = StackApplyRequest,
        content_type = "application/json,application/toml",
        description = "Stack body as JSON (default) or TOML (`Content-Type: application/toml`)"
    ),
    responses(
        (status = 200, description = "Stack applied", body = Vec<String>),
        (status = 400, description = "Invalid stack / program body (message names services[i] and the field)"),
        (status = 500, description = "Internal error")
    )
)]
async fn apply_stack(
    State(state): State<AppState>,
    StackBody(payload): StackBody,
) -> Result<Json<Vec<String>>, AppError> {
    let logs = state
        .manager
        .apply_stack(payload)
        .await
        .map_err(|e| map_program_mutation_error(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(logs))
}

/// Export Stack
#[utoipa::path(
    get,
    path = "/api/v1/stack",
    tag = "stack",
    responses(
        (status = 200, description = "Stack configuration", body = StackApplyRequest),
        (status = 500, description = "Internal error")
    )
)]
async fn export_stack(
    State(state): State<AppState>,
    user: Option<Extension<UserContext>>,
) -> Result<Json<StackApplyRequest>, AppError> {
    let reveal_secrets = user
        .as_ref()
        .is_some_and(|Extension(u)| matches!(u.role, UserRole::Admin));
    let configs = state
        .manager
        .dump_programs()
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let services: Vec<CreateProgramRequest> = configs
        .into_iter()
        .map(|c| {
            #[allow(unused_mut, clippy::needless_update)]
            let mut req = CreateProgramRequest {
                name: Some(c.name),
                command: c.command,
                args: c.args,
                env: if reveal_secrets {
                    c.env
                } else {
                    mask_env_map(&c.env)
                },
                env_file: c.env_file,
                cwd: c.cwd,
                user: c.user,
                group: c.group,
                autostart: c.autostart,
                retry_limit: c.retry_limit,
                autorestart: c.autorestart,
                exitcodes: c.exitcodes,
                startsecs: c.startsecs,
                stopsecs: c.stopsecs,
                priority: c.priority,
                stdout_logfile: c.stdout_logfile.clone(),
                stderr_logfile: c.stderr_logfile.clone(),
                depends_on: c.depends_on,
                health_check: c.health_check,
                hooks: c.hooks,
                artifact: c.artifact,
                cron: c.cron,
                numprocs: 1,
                process_name: None,
                ..Default::default()
            };
            req.resource_limits = c.resource_limits;
            req
        })
        .collect();

    Ok(Json(StackApplyRequest {
        prune: false,
        services,
    }))
}

/// Send signal to program
#[utoipa::path(
    post,
    path = "/api/v1/programs/{id}/signal",
    tag = "programs",
    params(
        ("id" = Uuid, Path, description = "Program ID")
    ),
    request_body = SignalProgramRequest,
    responses(
        (status = 200, description = "Signal sent"),
        (status = 500, description = "Internal error")
    )
)]
async fn signal_program(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<SignalProgramRequest>,
) -> Result<StatusCode, AppError> {
    // Parse signal string
    let sig = match payload.signal.to_lowercase().as_str() {
        "hup" => Signal::SIGHUP,
        "int" => Signal::SIGINT,
        "term" => Signal::SIGTERM,
        "kill" => Signal::SIGKILL,
        "quit" => Signal::SIGQUIT,
        "usr1" => Signal::SIGUSR1,
        "usr2" => Signal::SIGUSR2,
        _ => {
            return Err(AppError(
                StatusCode::BAD_REQUEST,
                anyhow::anyhow!("Unsupported signal type"),
            ));
        }
    };
    state
        .manager
        .signal_program(id, sig)
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(StatusCode::OK)
}

/// Prometheus Metrics
#[utoipa::path(
    get,
    path = "/metrics",
    tag = "system",
    responses(
        (status = 200, description = "Prometheus metrics", body = String)
    )
)]
async fn metrics_handler(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let metrics = state
        .manager
        .generate_metrics()
        .await
        .map_err(|e| AppError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        metrics,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    fn req_with(content_type: &str, body: &str) -> Request {
        Request::builder()
            .method("PUT")
            .header(axum::http::header::CONTENT_TYPE, content_type)
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn stack_body_json_default() {
        let body = r#"{"services":[{"name":"web","command":"/bin/true"}]}"#;
        let StackBody(stack) = StackBody::from_request(req_with("application/json", body), &())
            .await
            .unwrap();
        assert_eq!(stack.services.len(), 1);
        assert_eq!(stack.services[0].name.as_deref(), Some("web"));
    }

    #[tokio::test]
    async fn stack_body_toml_content_type() {
        let body = r#"
[[services]]
name = "web"
command = "/bin/true"
health_check = { type = "tcp", port = 8080 }
"#;
        let StackBody(stack) = StackBody::from_request(req_with("application/toml", body), &())
            .await
            .unwrap();
        let web = &stack.services[0];
        assert_eq!(web.name.as_deref(), Some("web"));
        assert!(matches!(
            web.health_check,
            Some(common::HealthCheck::Tcp { .. })
        ));
    }

    #[tokio::test]
    async fn stack_body_toml_error_carries_location() {
        let body = "[[services]]\nname = \"web\"\ncommand = [1, 2]\n";
        let err = StackBody::from_request(req_with("application/toml", body), &())
            .await
            .unwrap_err();
        let msg = err.1.to_string();
        assert!(msg.starts_with("request body:"), "{msg}");
        assert!(msg.contains("line 3") || msg.contains('3'), "{msg}");
    }

    #[tokio::test]
    async fn stack_body_toml_rejects_unknown_field() {
        let body = "[[services]]\nname = \"web\"\ncommand = \"/bin/true\"\nbogus_field = 1\n";
        let err = StackBody::from_request(req_with("application/toml", body), &())
            .await
            .unwrap_err();
        assert!(err.1.to_string().contains("bogus_field"));
    }

    #[tokio::test]
    async fn stack_body_json_error_keeps_field_path() {
        let body = r#"{"services":[{"name":"web","command":"/bin/true","bogus":1}]}"#;
        let err = StackBody::from_request(req_with("application/json", body), &())
            .await
            .unwrap_err();
        let msg = err.1.to_string();
        assert!(
            msg.contains("services[0]") && msg.contains("bogus"),
            "{msg}"
        );
    }
}
