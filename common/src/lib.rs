use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
// use serde_json::Value;
use utoipa::ToSchema;

pub mod auth;
pub mod config;
pub mod daemon;
pub mod license;
pub mod paths;
pub mod plugin_abi;
pub mod plugin_async;
pub mod plugin_error;
pub mod plugin_http_abi;
pub mod plugin_ui_abi;
pub mod program_validate;
pub mod resources;
pub mod security;

pub use daemon::{
    DEFAULT_PIDFILE_REL, PidfileStatus, claim_pidfile, inspect_pidfile, pid_is_alive,
    pidfile_parent_unwritable, release_pidfile, resolve_daemonize, resolve_pidfile_path,
    should_write_pidfile, under_systemd,
};
pub use paths::{resolve_super_root, resolve_super_root_for_config};
pub use security::{
    DEFAULT_SOCKET_MODE, FetchUrlPolicy, MAX_LICENSE_B64_LEN, MAX_LICENSE_JSON_LEN,
    is_loopback_bind_host, mask_env_map, mask_secret_value, parse_socket_mode,
    resolve_confined_log_path, resolve_plugin_library, sanitize_ui_asset_path,
    validate_license_grant_ids, validate_outbound_url,
};

pub use auth::{
    AuthRecord, AuthStatusResponse, AuthTokenInfo, CreateTokenRequest, CreateTokenResponse,
    UserContext, UserRole,
};
pub use license::{
    EmbeddedVerifyingKeyInfo, LICENSE_TRIAL_URL, LICENSE_UPGRADE_URL, LicenseClaims,
    LicenseExpiryStatus, LicenseInfo, embedded_keyring_summary, embedded_verifying_key_ids,
    embedded_verifying_keys, format_keyring_summary, kid_for_public_bytes,
    license_degradation_refusal_message, license_expiry_status, license_help_footer,
    licensed_deployment_intent, resolve_license_strict, scan_plugin_stems,
    should_refuse_license_degradation, verify_license, verify_license_for_superd,
};
pub use plugin_abi::{PLUGIN_API_VERSION, PLUGIN_SYMBOL, SuperPluginV1, read_plugin_version};
pub use plugin_error::{set_last_plugin_error, take_last_plugin_error};
pub use plugin_http_abi::{HTTP_PLUGIN_API_VERSION, HTTP_PLUGIN_SYMBOL, SuperPluginHttpV1};
pub use plugin_ui_abi::{SuperPluginUiV1, UI_PLUGIN_API_VERSION, UI_PLUGIN_SYMBOL};
pub use program_validate::{
    format_serde_json_error, validate_artifact_config, validate_create_program_request,
    validate_program_log_paths, validate_update_program_request, with_program_location,
};
pub use resources::ResourceLimits;

// Helpers
fn default_true() -> bool {
    true
}
fn default_retry_limit() -> u32 {
    3
}
fn default_localhost() -> String {
    "127.0.0.1".to_string()
}
fn default_one() -> u32 {
    1
}
fn default_startsecs() -> u32 {
    10
}
fn default_exitcodes() -> Vec<i32> {
    vec![0]
}
fn default_priority() -> i32 {
    999
}

/// Default seconds between health probes.
pub const DEFAULT_HEALTH_INTERVAL_SECS: u64 = 5;
/// Default per-probe timeout for TCP health checks.
pub const DEFAULT_TCP_HEALTH_TIMEOUT_SECS: u64 = 3;
/// Default per-probe timeout for HTTP health checks.
pub const DEFAULT_HTTP_HEALTH_TIMEOUT_SECS: u64 = 5;
/// Default per-probe timeout for exec health checks.
pub const DEFAULT_EXEC_HEALTH_TIMEOUT_SECS: u64 = 7;
/// Default grace period before the first probe after process start.
pub const DEFAULT_HEALTH_START_PERIOD_SECS: u64 = 1;
/// Default consecutive failures before the daemon auto-restarts the program.
pub const DEFAULT_HEALTH_MAX_FAILURES: u32 = 3;
/// Upper bound for health probe intervals (24h).
pub const MAX_HEALTH_INTERVAL_SECS: u64 = 86400;
/// Upper bound for a single probe timeout (1h).
pub const MAX_HEALTH_TIMEOUT_SECS: u64 = 3600;
/// Upper bound for the health failure threshold.
pub const MAX_HEALTH_MAX_FAILURES: u32 = 1000;

fn default_health_interval_secs() -> u64 {
    DEFAULT_HEALTH_INTERVAL_SECS
}
fn default_tcp_health_timeout_secs() -> u64 {
    DEFAULT_TCP_HEALTH_TIMEOUT_SECS
}
fn default_http_health_timeout_secs() -> u64 {
    DEFAULT_HTTP_HEALTH_TIMEOUT_SECS
}
fn default_exec_health_timeout_secs() -> u64 {
    DEFAULT_EXEC_HEALTH_TIMEOUT_SECS
}
fn default_health_start_period_secs() -> u64 {
    DEFAULT_HEALTH_START_PERIOD_SECS
}
fn default_health_max_failures() -> u32 {
    DEFAULT_HEALTH_MAX_FAILURES
}

/// Default number of overlapping cron runs allowed for a scheduled task.
pub const DEFAULT_MAX_CONCURRENT: u32 = 1;
/// Default cap on queued cron firings when `max_concurrent` is reached.
pub const DEFAULT_MAX_QUEUED: u32 = 100;
/// Hard upper bound for `max_concurrent`.
pub const MAX_CONCURRENT_CAP: u32 = 64;
/// Hard upper bound for `max_queued`.
pub const MAX_QUEUED_CAP: u32 = 10000;

/// Auto-restart policy (Supervisor-compatible semantics)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum AutorestartPolicy {
    /// Restart only when exit code is not in `exitcodes` (default)
    #[default]
    Unexpected,
    /// Always restart on any exit
    True,
    /// Never auto-restart
    False,
}

impl AutorestartPolicy {
    pub fn should_restart(&self, code: Option<i32>, exitcodes: &[i32]) -> bool {
        match self {
            Self::False => false,
            Self::True => true,
            Self::Unexpected => !ProgramConfig::is_expected_exit(code, exitcodes),
        }
    }
}

/// What to do when a cron tick fires while the previous run is still active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum CronOverlap {
    /// Skip the tick — never run concurrently with the previous run (default).
    #[default]
    Skip,
    /// Queue the run and start it as soon as the previous run exits.
    Queue,
    /// Terminate the running instance, then start the new run.
    Kill,
}

/// What to do with cron slots that were missed while the daemon was down or lagged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum CronCatchup {
    /// Never backfill missed slots (default).
    #[default]
    Skip,
    /// Run once for the most recent missed slot, immediately after recovery.
    Latest,
    /// Backfill every missed slot, capped to avoid a flood.
    All,
}

/// Process lifecycle status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "PascalCase")]
pub enum ProcessStatus {
    // Physical states (OS process)
    /// Stopped, no PID
    Stopped,
    /// Starting (pre-start hook or spawn)
    Starting,
    /// Running (has PID), health check not yet passed
    Running,
    /// Crash backoff (waiting for retry timer)
    Backoff,
    /// Start failed (retries exhausted or config error)
    Fatal,
    /// Stopping (signal sent)
    Stopping,

    // Logical states (Superd manager layer)
    /// Waiting for dependencies
    Waiting,
    /// Running and healthy, ready to serve
    Healthy,
}

/// Core program config (persisted in snapshot.json)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ProgramConfig {
    // Identity
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,

    // Dynamically loaded env file path (e.g. /etc/secrets/.env)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_file: Option<String>,

    pub cwd: Option<String>,
    pub user: Option<String>,

    // Behavior
    #[serde(default = "default_true")]
    pub autostart: bool,
    #[serde(default = "default_retry_limit")]
    pub retry_limit: u32,

    /// Auto-restart policy on process exit (Supervisor `autorestart`)
    #[serde(default)]
    pub autorestart: AutorestartPolicy,

    /// Exit codes considered successful when `autorestart = unexpected` (default: [0])
    #[serde(default = "default_exitcodes")]
    pub exitcodes: Vec<i32>,

    /// Seconds a process must run before exit counts as stable (Supervisor `startsecs`)
    #[serde(default = "default_startsecs")]
    pub startsecs: u32,

    /// Seconds to wait for SIGTERM before SIGKILL. Falls back to `[server].shutdown_timeout`.
    /// Supervisor config alias: `stopwaitsecs`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "stopwaitsecs"
    )]
    pub stopsecs: Option<u32>,

    /// Startup order when multiple programs autostart (Supervisor `priority`; lower = earlier).
    #[serde(default = "default_priority")]
    pub priority: i32,

    /// Custom stdout log file path (Supervisor `stdout_logfile`). Default: `{log_dir}/{uuid}.out`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout_logfile: Option<String>,

    /// Custom stderr log file path (Supervisor `stderr_logfile`). Default: `{log_dir}/{uuid}.err`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr_logfile: Option<String>,

    // Orchestration and grouping
    pub group: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub health_check: Option<HealthCheck>,
    #[serde(default)]
    pub hooks: ProgramHooks,

    // OTA upgrade
    /// Online upgrade config; changes trigger an upgrade transaction
    pub artifact: Option<ArtifactConfig>,

    // Advanced features
    /// Cron expression (e.g. "0 0 * * * *"). Scheduled tasks do not autostart on daemon boot.
    pub cron: Option<String>,

    /// Cron overlap policy when the previous run is still executing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_overlap: Option<CronOverlap>,

    /// Policy for cron slots missed while the daemon was down.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catchup: Option<CronCatchup>,

    /// Max random delay (seconds) added before each cron trigger to spread load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jitter_sec: Option<u64>,

    /// Max overlapping cron runs allowed at once (default 1). Scheduled tasks
    /// may share the same cron expression; `max_concurrent` lets up to N runs
    /// of the same task be in flight simultaneously.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<u32>,

    /// Cap on queued cron firings when `max_concurrent` is reached and
    /// `on_overlap = queue`/`kill`. New firings beyond the cap are dropped and
    /// recorded as `queue_full`. `0` means the default (100).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_queued: Option<u32>,

    /// Internal: last successful cron spawn (epoch secs). Used to detect slots
    /// missed while the daemon was down so the catchup policy can backfill them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cron_last_run: Option<u64>,

    /// Linux cgroup resource limits (requires isolation plugin on Linux).
    #[serde(default)]
    pub resource_limits: Option<ResourceLimits>,

    // Metadata
    pub created_at: u64,
    pub updated_at: u64,

    // [WAL] Upgrade transaction state
    // Core field for transactional upgrades; acts as a write-ahead log.
    // 1. None: stable state.
    // 2. Some(path): upgrade verification period; path is backup of previous version.
    //
    // If Manager restarts and this field is set, the last upgrade did not finish.
    // On process crash, Manager uses this path to roll back immediately.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_path: Option<String>,
}

impl ProgramConfig {
    pub fn is_expected_exit(code: Option<i32>, exitcodes: &[i32]) -> bool {
        match code {
            Some(c) => exitcodes.contains(&c),
            None => false,
        }
    }

    pub fn should_autorestart(&self, code: Option<i32>) -> bool {
        self.autorestart.should_restart(code, &self.exitcodes)
    }

    /// Effective `max_concurrent` (1 default, 0 treated as default, capped at 64).
    pub fn max_concurrent_eff(&self) -> u32 {
        let v = self.max_concurrent.unwrap_or(DEFAULT_MAX_CONCURRENT);
        if v == 0 {
            DEFAULT_MAX_CONCURRENT
        } else {
            v.min(MAX_CONCURRENT_CAP)
        }
    }

    /// Effective `max_queued` (100 default, 0 treated as default, capped at 10000).
    pub fn max_queued_eff(&self) -> u32 {
        let v = self.max_queued.unwrap_or(DEFAULT_MAX_QUEUED);
        if v == 0 {
            DEFAULT_MAX_QUEUED
        } else {
            v.min(MAX_QUEUED_CAP)
        }
    }
}

/// Health check configuration.
///
/// Every probe type shares the same tuning knobs (`interval_secs`,
/// `timeout_secs`, `start_period_secs`, `max_failures`). A value of `0` for
/// interval/timeout/start_period falls back to the default; `max_failures = 0`
/// disables auto-restart on health failure (mark unhealthy only).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HealthCheck {
    Tcp {
        #[serde(default = "default_localhost")]
        host: String,
        port: u16,
        #[serde(default = "default_health_interval_secs")]
        interval_secs: u64,
        #[serde(default = "default_tcp_health_timeout_secs")]
        timeout_secs: u64,
        #[serde(default = "default_health_start_period_secs")]
        start_period_secs: u64,
        #[serde(default = "default_health_max_failures")]
        max_failures: u32,
    },
    Http {
        url: String,
        method: Option<String>,
        #[serde(default = "default_health_interval_secs")]
        interval_secs: u64,
        #[serde(default = "default_http_health_timeout_secs")]
        timeout_secs: u64,
        #[serde(default = "default_health_start_period_secs")]
        start_period_secs: u64,
        #[serde(default = "default_health_max_failures")]
        max_failures: u32,
    },
    Exec {
        command: String,
        #[serde(default = "default_health_interval_secs")]
        interval_secs: u64,
        #[serde(default = "default_exec_health_timeout_secs")]
        timeout_secs: u64,
        #[serde(default = "default_health_start_period_secs")]
        start_period_secs: u64,
        #[serde(default = "default_health_max_failures")]
        max_failures: u32,
    },

    Disabled,
}

impl HealthCheck {
    /// Seconds between probes (0 = default; unused for `Disabled`). Upper
    /// bounds are enforced by config validation (`validate_health_tuning`).
    pub fn interval_secs(&self) -> u64 {
        match self {
            HealthCheck::Tcp { interval_secs, .. }
            | HealthCheck::Http { interval_secs, .. }
            | HealthCheck::Exec { interval_secs, .. } => {
                if *interval_secs == 0 {
                    DEFAULT_HEALTH_INTERVAL_SECS
                } else {
                    *interval_secs
                }
            }
            HealthCheck::Disabled => 0,
        }
    }

    /// Max seconds a single probe may take (0 = default; unused for `Disabled`).
    pub fn timeout_secs(&self) -> u64 {
        match self {
            HealthCheck::Tcp { timeout_secs, .. } => {
                if *timeout_secs == 0 {
                    DEFAULT_TCP_HEALTH_TIMEOUT_SECS
                } else {
                    *timeout_secs
                }
            }
            HealthCheck::Http { timeout_secs, .. } => {
                if *timeout_secs == 0 {
                    DEFAULT_HTTP_HEALTH_TIMEOUT_SECS
                } else {
                    *timeout_secs
                }
            }
            HealthCheck::Exec { timeout_secs, .. } => {
                if *timeout_secs == 0 {
                    DEFAULT_EXEC_HEALTH_TIMEOUT_SECS
                } else {
                    *timeout_secs
                }
            }
            HealthCheck::Disabled => 0,
        }
    }

    /// Grace period (seconds) after process start before the first probe
    /// (0 = default; unused for `Disabled`).
    pub fn start_period_secs(&self) -> u64 {
        match self {
            HealthCheck::Tcp {
                start_period_secs, ..
            }
            | HealthCheck::Http {
                start_period_secs, ..
            }
            | HealthCheck::Exec {
                start_period_secs, ..
            } => {
                if *start_period_secs == 0 {
                    DEFAULT_HEALTH_START_PERIOD_SECS
                } else {
                    *start_period_secs
                }
            }
            HealthCheck::Disabled => 0,
        }
    }

    /// Consecutive failures before the daemon auto-restarts the program.
    /// `0` disables auto-restart (the process is only marked unhealthy).
    pub fn max_failures(&self) -> u32 {
        match self {
            HealthCheck::Tcp { max_failures, .. }
            | HealthCheck::Http { max_failures, .. }
            | HealthCheck::Exec { max_failures, .. } => *max_failures,
            HealthCheck::Disabled => 0,
        }
    }

    /// True when this check can actually fail (i.e. it is not `Disabled`).
    pub fn is_enabled(&self) -> bool {
        !matches!(self, HealthCheck::Disabled)
    }
}

/// Lifecycle hooks
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct ProgramHooks {
    pub pre_start: Option<String>,
    pub post_start: Option<String>,
    pub pre_stop: Option<String>,
    pub post_stop: Option<String>,
}

/// OTA artifact configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ArtifactConfig {
    pub source: String,
    pub checksum: String,
    pub extract: bool,
    pub destination: String,
    pub restart_policy: String,
}

/// API request: create program
#[derive(Debug, Deserialize, Serialize, Default, Clone, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateProgramRequest {
    pub name: Option<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub env_file: Option<String>,

    pub cwd: Option<String>,
    pub user: Option<String>,

    #[serde(default = "default_true")]
    pub autostart: bool,
    #[serde(default = "default_retry_limit")]
    pub retry_limit: u32,

    #[serde(default)]
    pub autorestart: AutorestartPolicy,
    #[serde(default = "default_exitcodes")]
    pub exitcodes: Vec<i32>,
    #[serde(default = "default_startsecs")]
    pub startsecs: u32,
    pub stopsecs: Option<u32>,
    #[serde(default = "default_priority")]
    pub priority: i32,
    pub stdout_logfile: Option<String>,
    pub stderr_logfile: Option<String>,

    pub group: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub health_check: Option<HealthCheck>,
    #[serde(default)]
    pub hooks: ProgramHooks,
    pub artifact: Option<ArtifactConfig>,

    #[serde(default = "default_one")]
    pub numprocs: u32,
    pub process_name: Option<String>,

    pub cron: Option<String>,

    /// Cron overlap policy when the previous run is still executing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_overlap: Option<CronOverlap>,

    /// Policy for cron slots missed while the daemon was down.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catchup: Option<CronCatchup>,

    /// Max random delay (seconds) added before each cron trigger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jitter_sec: Option<u64>,

    /// Max overlapping cron runs allowed at once (default 1; 0 means default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<u32>,

    /// Cap on queued cron firings when at `max_concurrent` (default 100; 0 means default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_queued: Option<u32>,

    /// Linux cgroup resource limits (requires isolation plugin on Linux).
    #[serde(default)]
    pub resource_limits: Option<ResourceLimits>,
}

/// API request: update program (partial)
#[derive(Debug, Deserialize, Serialize, Default, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateProgramRequest {
    pub name: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub env_file: Option<String>,
    pub cwd: Option<String>,
    pub user: Option<String>,
    pub autostart: Option<bool>,
    pub retry_limit: Option<u32>,
    pub autorestart: Option<AutorestartPolicy>,
    pub exitcodes: Option<Vec<i32>>,
    pub startsecs: Option<u32>,
    pub stopsecs: Option<u32>,
    pub priority: Option<i32>,
    pub stdout_logfile: Option<String>,
    pub stderr_logfile: Option<String>,
    pub group: Option<String>,
    pub depends_on: Option<Vec<String>>,
    pub health_check: Option<HealthCheck>,
    pub hooks: Option<ProgramHooks>,
    pub artifact: Option<ArtifactConfig>,

    pub cron: Option<String>,

    /// Cron overlap policy when the previous run is still executing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_overlap: Option<CronOverlap>,

    /// Policy for cron slots missed while the daemon was down.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catchup: Option<CronCatchup>,

    /// Max random delay (seconds) added before each cron trigger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jitter_sec: Option<u64>,

    /// Max overlapping cron runs allowed at once (default 1; 0 means default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<u32>,

    /// Cap on queued cron firings when at `max_concurrent` (default 100; 0 means default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_queued: Option<u32>,

    /// Linux cgroup resource limits (requires isolation plugin on Linux).
    #[serde(default)]
    pub resource_limits: Option<ResourceLimits>,
}

/// API response: list summary
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ProgramSummary {
    pub id: Uuid,
    pub name: String,
    pub group: Option<String>,
    pub status: ProcessStatus,
    pub pid: Option<u32>,
    pub uptime_sec: Option<u64>,

    /// Unix timestamp when the program was created.
    #[serde(default)]
    pub created_at: u64,
    pub updated_at: u64,
    pub cpu_usage: Option<f32>,
    pub mem_usage: Option<u64>,
    pub last_error: Option<String>,

    /// Latest health_check failure while running (not yet Healthy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_error: Option<String>,

    #[serde(default)]
    pub depends_on: Vec<String>,

    #[serde(default)]
    pub resource_limits: Option<ResourceLimits>,
}

/// API response: program details
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProgramInfo {
    pub id: Uuid,
    pub state: ProcessStatus,
    pub pid: Option<u32>,
    pub config: ProgramConfig,
    pub last_error: Option<String>,

    /// Latest health_check failure while running (not yet Healthy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_error: Option<String>,
}

/// API response: readiness-aware reload (`POST /api/v1/system/reload`).
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ReloadResponse {
    /// Programs touched by the reload (created or updated from include stacks).
    pub affected: Vec<ProgramSummary>,
    /// Whether every affected program reached `Healthy` within the wait window
    /// (`true` immediately when no wait was requested or nothing was affected).
    pub ready: bool,
    /// Seconds actually waited (0 when wait was not requested).
    pub waited_secs: u64,
}

/// WebSocket message protocol
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "payload")]
pub enum WsMessage {
    StatusChange {
        id: Uuid,
        status: ProcessStatus,
        name: String,
    },
    Log {
        id: Uuid,
        source: String,
        line: String,
    },
}

/// Health check response
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub components: HashMap<String, String>,
}

/// Host-level resource snapshot (for dashboard system charts)
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct SystemStats {
    pub cpu_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub timestamp: u64,
}

/// Declarative stack apply request
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct StackApplyRequest {
    pub services: Vec<CreateProgramRequest>,
    #[serde(default)]
    pub prune: bool,
}

/// System events (notifications and audit)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum SystemEvent {
    /// Process entered Fatal or was rolled back after upgrade failure
    ProcessFatal {
        program_id: Uuid,
        program_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pid: Option<u32>,
        #[serde(default)]
        uptime_secs: u64,
        exit_code: Option<i32>,
        /// Terminating signal (e.g. `9` = SIGKILL, includes cgroup OOM kills).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signal: Option<i32>,
        msg: String,
        log_tail: Option<String>,
    },
    /// Process crashed but is retrying
    ProcessBackoff {
        program_id: Uuid,
        program_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pid: Option<u32>,
        #[serde(default)]
        uptime_secs: u64,
        exit_code: Option<i32>,
        /// Terminating signal (e.g. `9` = SIGKILL, includes cgroup OOM kills).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signal: Option<i32>,
        retry_count: u32,
    },
    /// Process started successfully
    ProcessStarted {
        program_id: Uuid,
        program_name: String,
        pid: u32,
    },
    /// Manager process started
    SystemStartup { hostname: String },
    /// Process recovered from unstable state
    ProcessRecovered {
        program_id: Uuid,
        program_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pid: Option<u32>,
        uptime_sec: u64,
    },
    /// Health probes failed `max_failures` times consecutively; the daemon is
    /// auto-restarting the process.
    HealthRestart {
        program_id: Uuid,
        program_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pid: Option<u32>,
        #[serde(default)]
        uptime_secs: u64,
        /// How many consecutive health-triggered restarts so far.
        retry_count: u32,
        /// Last probe failure detail.
        msg: String,
    },
    /// Manager process shutting down
    SystemShutdown,
    /// Anonymous memory of a limited cgroup crossed the warning threshold
    /// (pre-kill warning; process still running).
    MemoryPressure {
        program_id: Uuid,
        program_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pid: Option<u32>,
        /// Anonymous memory in bytes at the time of the warning.
        usage_bytes: u64,
        /// Hard limit (`memory.max`) in bytes.
        limit_bytes: u64,
        /// Effective warning threshold in bytes (percent or headroom derived).
        warn_bytes: u64,
    },
    /// Kernel OOM-killed a limited cgroup (`memory.events` → `oom_kill`
    /// incremented); definitive post-mortem confirmation.
    MemoryOomKill {
        program_id: Uuid,
        program_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pid: Option<u32>,
        /// Anonymous memory in bytes at detection.
        anon_bytes: u64,
        /// Hard limit (`memory.max`) in bytes.
        limit_bytes: u64,
        /// Total memory usage (`memory.current`) in bytes.
        usage_bytes: u64,
    },
}

impl SystemEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            SystemEvent::ProcessFatal { .. } => "process_fatal",
            SystemEvent::ProcessBackoff { .. } => "process_backoff",
            SystemEvent::ProcessStarted { .. } => "process_started",
            SystemEvent::SystemStartup { .. } => "system_startup",
            SystemEvent::ProcessRecovered { .. } => "process_recovered",
            SystemEvent::HealthRestart { .. } => "health_restart",
            SystemEvent::SystemShutdown => "system_shutdown",
            SystemEvent::MemoryPressure { .. } => "memory_pressure",
            SystemEvent::MemoryOomKill { .. } => "memory_oom_kill",
        }
    }

    pub fn program_name(&self) -> Option<&str> {
        match self {
            SystemEvent::ProcessFatal { program_name, .. }
            | SystemEvent::ProcessBackoff { program_name, .. }
            | SystemEvent::ProcessStarted { program_name, .. }
            | SystemEvent::ProcessRecovered { program_name, .. }
            | SystemEvent::HealthRestart { program_name, .. }
            | SystemEvent::MemoryPressure { program_name, .. }
            | SystemEvent::MemoryOomKill { program_name, .. } => Some(program_name),
            SystemEvent::SystemStartup { .. } | SystemEvent::SystemShutdown => None,
        }
    }
}

/// One persisted runtime/exception event in a program's lifecycle history.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProgramEventRecord {
    /// Unix timestamp (seconds).
    pub ts: u64,
    /// Event type: `process_fatal`, `process_backoff`, `process_recovered`, or `process_exit`.
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Terminating signal when killed by a signal (e.g. `9` = SIGKILL, includes OOM kills).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<u32>,
    pub msg: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct SignalProgramRequest {
    pub signal: String,
}

// Batch action variants
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", content = "payload")]
pub enum BatchAction {
    Start,
    Stop {
        force: bool,
    },
    Restart,
    /// Signal name (hup, int, term, kill, etc.)
    Signal {
        signal: String,
    },
    Remove,
}

// Batch operation request body
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchProgramRequest {
    // Filter: exactly one of
    // 1. Explicit ID list
    pub target_ids: Option<Vec<Uuid>>,
    // 2. Group name
    pub group_name: Option<String>,
    // 3. Select all
    #[serde(default)]
    pub select_all: bool,

    // Action to perform
    pub action: BatchAction,
}

// Batch operation response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BatchProgramResponse {
    // IDs of programs affected (succeeded)
    pub affected: Vec<Uuid>,
    // Failed programs (ID -> error message)
    pub failed: HashMap<Uuid, String>,
}

/// Historical log file content
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ProgramLogFile {
    pub source: String,
    pub content: String,
}

/// Response for GET /api/v1/programs/{id}/logs
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ProgramLogsResponse {
    pub id: Uuid,
    pub logs: Vec<ProgramLogFile>,
}
