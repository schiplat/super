use nix::sys::signal::Signal;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::oneshot;
use uuid::Uuid;

use common::{
    BatchProgramRequest, BatchProgramResponse, CreateProgramRequest, HealthResponse, ProgramConfig,
    ProgramInfo, ProgramSummary, StackApplyRequest, UpdateProgramRequest,
};

#[derive(Debug)]
pub enum Command {
    Shutdown {
        reply: oneshot::Sender<()>,
    },
    Reload {
        reply: oneshot::Sender<anyhow::Result<()>>,
    },

    // Generic batch operation commands
    BatchPrograms {
        request: BatchProgramRequest,
        reply: oneshot::Sender<anyhow::Result<BatchProgramResponse>>,
    },

    // Returns a list of UUIDs
    CreateProgram {
        config: CreateProgramRequest,
        reply: oneshot::Sender<anyhow::Result<Vec<Uuid>>>,
    },

    // Update program configuration
    UpdateProgram {
        id: Uuid,
        request: UpdateProgramRequest,
        reply: oneshot::Sender<anyhow::Result<()>>,
    },

    StartProgram {
        id: Uuid,
        reply: oneshot::Sender<anyhow::Result<()>>,
    },
    // Includes force parameter
    StopProgram {
        id: Uuid,
        force: bool,
        reply: oneshot::Sender<anyhow::Result<()>>,
    },
    RestartProgram {
        id: Uuid,
        reply: oneshot::Sender<anyhow::Result<()>>,
    },
    RemoveProgram {
        id: Uuid,
        reply: oneshot::Sender<anyhow::Result<()>>,
    },

    ListPrograms {
        reply: oneshot::Sender<Vec<ProgramSummary>>,
    },
    GetProgram {
        id: Uuid,
        reply: oneshot::Sender<anyhow::Result<ProgramInfo>>,
    },
    GetProgramEvents {
        id: Uuid,
        reply: oneshot::Sender<Vec<common::ProgramEventRecord>>,
    },

    // Group operation commands
    StartGroup {
        group: String,
        reply: oneshot::Sender<anyhow::Result<Vec<Uuid>>>,
    },
    StopGroup {
        group: String,
        force: bool,
        reply: oneshot::Sender<anyhow::Result<Vec<Uuid>>>,
    },
    RestartGroup {
        group: String,
        reply: oneshot::Sender<anyhow::Result<Vec<Uuid>>>,
    },

    ProcessExited {
        id: Uuid,
        code: Option<i32>,
        signal: Option<i32>,
    },
    CheckTimeoutKill {
        id: Uuid,
        target_pid: u32,
    },
    ScheduledRestart {
        id: Uuid,
        retry_count: u32,
    },

    // For HTTP API (GET /health)
    HealthCheck {
        reply: oneshot::Sender<HealthResponse>,
    },

    // Internal health status update (id, is_healthy, optional failure detail)
    InternalHealthUpdate {
        id: Uuid,
        is_healthy: bool,
        failure_detail: Option<String>,
    },

    // Declarative stack deployment
    ApplyStack {
        request: StackApplyRequest,
        reply: oneshot::Sender<anyhow::Result<Vec<String>>>,
    },

    InternalArtifactReady {
        id: Uuid,
        path: PathBuf,
    },
    DumpPrograms {
        reply: oneshot::Sender<Vec<ProgramConfig>>,
    },

    // Internal command to break async recursion
    CheckWaitingQueue,

    // Send signal to process
    SignalProgram {
        id: Uuid,
        signal: Signal,
        reply: oneshot::Sender<anyhow::Result<()>>,
    },

    // Batch resource metrics update (sent by Monitor)
    InternalMetricsUpdate {
        metrics: HashMap<Uuid, (f32, u64)>,
    },

    // Cron tick
    CronTick,

    // Persistence tick
    PersistTick,

    // Generate Prometheus-format metrics
    GenerateMetrics {
        reply: oneshot::Sender<String>,
    },

    GetSystemStats {
        reply: oneshot::Sender<common::SystemStats>,
    },
}
