pub mod command;
pub mod controller;
pub mod registry;
pub mod tracker;

pub use command::Command;

use crate::config::ServerConfig;
use crate::extension::Extension;
use crate::monitor::ResourceMonitor;
use crate::scheduler::CronScheduler;
use crate::store;

use self::controller::LifecycleController;
use self::registry::ProcessRegistry;

use common::{
    BatchAction, BatchProgramRequest, BatchProgramResponse, CreateProgramRequest, HealthResponse,
    ProcessStatus, ProgramConfig, ProgramInfo, ProgramSummary, ResourceLimits, StackApplyRequest,
    UpdateProgramRequest, WsMessage, resolve_confined_log_path, validate_create_program_request,
    validate_update_program_request, with_program_location,
};
use glob::glob;
use nix::sys::signal::Signal;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

/// Upper bound for `catchup=all` backfills, so a long daemon outage cannot
/// flood the machine with catch-up runs.
const CRON_CATCHUP_CAP: u32 = 10;

/// Reconstruct the last cron run instant from a persisted epoch timestamp.
fn cron_last_run_dt(config: &ProgramConfig) -> Option<chrono::DateTime<chrono::Utc>> {
    config
        .cron_last_run
        .and_then(|t| chrono::DateTime::<chrono::Utc>::from_timestamp(t as i64, 0))
}

/// Merge `resource_limits` from an update request into stored config.
/// `-1.0` cpu / `0` memory / `0` warn+high are sentinels that clear a field.
fn apply_resource_limits_patch(existing: &mut Option<ResourceLimits>, patch: ResourceLimits) {
    if let Some(old) = existing {
        if let Some(c) = patch.cpu_quota {
            old.cpu_quota = if c <= 0.0 { None } else { Some(c) };
        }
        if let Some(m) = patch.memory_limit {
            old.memory_limit = if m == 0 { None } else { Some(m) };
        }
        if let Some(w) = patch.memory_warn_percent {
            old.memory_warn_percent = if w == 0 { None } else { Some(w) };
        }
        if let Some(h) = patch.memory_warn_headroom {
            old.memory_warn_headroom = if h == 0 { None } else { Some(h) };
        }
        if let Some(h) = patch.memory_high {
            old.memory_high = if h == 0 { None } else { Some(h) };
        }
        if old.cpu_quota.is_none() && old.memory_limit.is_none() {
            *existing = None;
        }
    } else {
        let cpu = patch.cpu_quota.filter(|&c| c > 0.0);
        let mem = patch.memory_limit.filter(|&m| m > 0);
        let warn_percent = patch.memory_warn_percent.filter(|&w| w > 0 && w <= 100);
        let warn_headroom = patch.memory_warn_headroom.filter(|&h| h > 0);
        let high = patch.memory_high.filter(|&h| h > 0);
        if cpu.is_some()
            || mem.is_some()
            || warn_percent.is_some()
            || warn_headroom.is_some()
            || high.is_some()
        {
            *existing = Some(ResourceLimits {
                cpu_quota: cpu,
                memory_limit: mem,
                memory_warn_percent: warn_percent,
                memory_warn_headroom: warn_headroom,
                memory_high: high,
            });
        }
    }
}

fn validate_resource_limits_patch(limits: &ResourceLimits) -> anyhow::Result<()> {
    if let Some(cpu) = limits.cpu_quota
        && cpu <= 0.0
        && cpu != -1.0
    {
        return Err(anyhow::anyhow!("CPU quota must be positive"));
    }
    if let Some(w) = limits.memory_warn_percent
        && w > 100
    {
        return Err(anyhow::anyhow!(
            "memory_warn_percent must be 0–100 (got {w})"
        ));
    }
    Ok(())
}

fn warn_if_resource_limits_unenforced(
    extension: &dyn Extension,
    limits: &Option<ResourceLimits>,
    context: &str,
) {
    let Some(limits) = limits else {
        return;
    };
    if limits.cpu_quota.is_none()
        && limits.memory_limit.is_none()
        && limits.memory_warn_percent.is_none()
        && limits.memory_warn_headroom.is_none()
        && limits.memory_high.is_none()
    {
        return;
    }
    if extension.supports_resource_limits() {
        return;
    }
    tracing::warn!(
        "{context}: resource_limits set but the isolation plugin is not loaded — \
         limits are stored only, not enforced (Linux cgroup)"
    );
}

// Manager actor: core system coordinator
pub struct Manager {
    config: ServerConfig,
    config_path: PathBuf,
    log_reloader: Box<dyn Fn(String) -> anyhow::Result<()> + Send + Sync>,

    rx: mpsc::Receiver<Command>,
    tx_self: mpsc::Sender<Command>,
    log_tx: broadcast::Sender<WsMessage>,

    registry: ProcessRegistry,
    controller: LifecycleController,

    scheduler: CronScheduler,
    monitor: Arc<ResourceMonitor>,

    /// Cron runs queued behind a still-running instance (`overlap=queue`) or
    /// waiting to be backfilled (`catchup=all`). Drained one run per tick.
    pending_cron: HashMap<Uuid, u32>,

    extension: Arc<dyn Extension>,

    /// Persisted event history (SQLite). Writes go through `event_tx` to a
    /// background batch writer so the actor loop never blocks on disk I/O.
    event_db: crate::event_db::EventDb,
    event_tx: mpsc::Sender<common::ProgramEventRecord>,

    /// UTC day (epoch / 86400) of the last `events_keep_days` prune.
    last_event_prune: u64,
}
impl Manager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: ServerConfig,
        config_path: PathBuf,
        log_reloader: Box<dyn Fn(String) -> anyhow::Result<()> + Send + Sync>,
        rx: mpsc::Receiver<Command>,
        tx_self: mpsc::Sender<Command>,
        initial_programs: HashMap<Uuid, ProgramConfig>,
        log_tx: broadcast::Sender<WsMessage>,
        extension: Box<dyn Extension>,
        event_db: crate::event_db::EventDb,
    ) -> Self {
        // Persistence heartbeat (debounced flush)
        let tx_persist = tx_self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            interval.tick().await;
            loop {
                interval.tick().await;
                if tx_persist.send(Command::PersistTick).await.is_err() {
                    break;
                }
            }
        });

        // Event history batch writer: drain the queue into transaction batches.
        // Runs on its own task so event persistence never blocks the actor.
        let (event_tx, mut event_rx) = mpsc::channel::<common::ProgramEventRecord>(2048);
        {
            let db = event_db.clone();
            tokio::spawn(async move {
                let mut batch: Vec<common::ProgramEventRecord> = Vec::new();
                loop {
                    if batch.is_empty() {
                        match event_rx.recv().await {
                            Some(e) => batch.push(e),
                            None => break,
                        }
                    } else {
                        // Drain what's available without blocking, up to a cap.
                        while batch.len() < 512 {
                            match event_rx.try_recv() {
                                Ok(e) => batch.push(e),
                                Err(mpsc::error::TryRecvError::Empty) => break,
                                Err(mpsc::error::TryRecvError::Disconnected) => {
                                    if let Err(e) = db.insert_batch(&batch).await {
                                        tracing::error!("Failed to flush event batch: {}", e);
                                    }
                                    return;
                                }
                            }
                        }
                        if let Err(e) = db.insert_batch(&batch).await {
                            tracing::error!("Failed to flush event batch: {}", e);
                        }
                        batch.clear();
                    }
                }
            });
        }

        // Cron tick (once per second)
        let tx_cron = tx_self.clone();
        tokio::spawn(async move {
            // Align to next second boundary for timing accuracy
            let _now = tokio::time::Instant::now();
            let delay = 1000 - (chrono::Utc::now().timestamp_subsec_millis() as u64);
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;

            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                interval.tick().await;
                if tx_cron.send(Command::CronTick).await.is_err() {
                    break;
                }
            }
        });

        let scheduler = CronScheduler::new();
        let monitor = Arc::new(ResourceMonitor::new(tx_self.clone()));
        let extension: Arc<dyn Extension> = Arc::from(extension);

        // Expose the daemon event pipeline to plugins (plugin→host `emit_event`).
        crate::plugin::host_emit::install(extension.clone(), config.event_hooks.clone());

        let registry = ProcessRegistry::new(initial_programs);
        let controller = LifecycleController::new(
            config.clone(),
            tx_self.clone(),
            log_tx.clone(),
            extension.clone(),
            monitor.clone(),
        );

        Self {
            config,
            config_path,
            log_reloader,
            rx,
            tx_self,
            log_tx,
            registry,
            controller,
            scheduler,
            monitor,
            pending_cron: HashMap::new(),
            extension,
            event_db,
            event_tx,
            last_event_prune: 0,
        }
    }

    /// Prune retained events once per UTC day when `events_keep_days` is set.
    async fn maybe_prune_events(&mut self) {
        let keep_days = self.config.storage.events_keep_days;
        if keep_days == 0 {
            return;
        }
        let today = chrono::Utc::now().timestamp() as u64 / 86_400;
        if today == self.last_event_prune {
            return;
        }
        self.last_event_prune = today;
        match self.event_db.prune_older_than(keep_days).await {
            Ok(n) if n > 0 => {
                tracing::info!("Pruned {} event(s) older than {} day(s)", n, keep_days);
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("Event retention prune failed: {}", e),
        }
    }

    fn emit_event(&self, event: common::SystemEvent) {
        crate::event_hooks::emit(&self.extension, &self.config.event_hooks, event);
    }

    /// Append a lifecycle event to the persisted history. The record is queued
    /// to the background SQLite batch writer — this never blocks the actor.
    #[allow(clippy::too_many_arguments)]
    fn record_event(
        &self,
        id: Uuid,
        name: &str,
        event: &str,
        code: Option<i32>,
        signal: Option<i32>,
        retry_count: Option<u32>,
        duration_secs: Option<u64>,
        msg: String,
    ) {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let record = common::ProgramEventRecord {
            ts: now_ms / 1000,
            ts_ms: now_ms,
            program_id: Some(id),
            program_name: Some(name.to_string()),
            event: event.to_string(),
            exit_code: code,
            signal,
            retry_count,
            duration_secs,
            msg,
        };
        let _ = self.event_tx.try_send(record);
    }

    /// Record a system-wide event (no owning program).
    fn record_system_event(&self, event: &str, msg: String) {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        let record = common::ProgramEventRecord {
            ts: now_ms / 1000,
            ts_ms: now_ms,
            program_id: None,
            program_name: None,
            event: event.to_string(),
            exit_code: None,
            signal: None,
            retry_count: None,
            duration_secs: None,
            msg,
        };
        let _ = self.event_tx.try_send(record);
    }
    pub async fn run(mut self) {
        tracing::info!(
            "Manager Loop started. Loaded {} programs.",
            self.registry.programs.len()
        );

        let hostname = common::resolve_hostname();
        self.emit_event(common::SystemEvent::SystemStartup { hostname });
        self.record_system_event(
            "system_startup",
            format!(
                "Daemon started with {} program(s)",
                self.registry.programs.len()
            ),
        );

        if let Err(e) = self.process_includes().await {
            tracing::error!("Failed to process includes on startup: {}", e);
        }

        // Restore state & WAL check
        for (id, config) in &mut self.registry.programs {
            if let Some(cron) = &config.cron {
                let jitter = config.jitter_sec.unwrap_or(0);
                let last_run = cron_last_run_dt(config);
                self.scheduler.upsert(*id, cron, jitter, last_run);
            }
            // [WAL recovery check]
            // restore_path at startup means Manager crashed during upgrade validation.
            // Keep path and try new binary; handle_exited rolls back if it fails.
            if let Some(bak) = &config.restore_path {
                tracing::warn!(
                    "Found unfinished upgrade transaction for {}. Backup at: {}",
                    config.name,
                    bak
                );
            }
        }

        // Startup recovery (priority: lower value starts earlier, Supervisor-compatible)
        let mut startup_ids: Vec<(i32, Uuid)> = self
            .registry
            .programs
            .iter()
            .filter(|(_, config)| config.autostart && config.cron.is_none())
            .map(|(id, config)| (config.priority, *id))
            .collect();
        startup_ids.sort_by_key(|(priority, _)| *priority);

        let startup_count = startup_ids.len();
        if startup_count > 0 {
            tracing::info!(
                "Restoring {} programs with staggered startup (Anti-Avalanche)...",
                startup_count
            );

            for (i, (_, id)) in startup_ids.into_iter().enumerate() {
                if let Err(e) = self
                    .controller
                    .spawn_program(&mut self.registry, id, 0)
                    .await
                {
                    tracing::error!("Failed to restore program {}: {}", id, e);
                }

                // Staggered startup (anti-avalanche)
                // 100ms pause between services to smooth I/O and allocation spikes
                if i < startup_count - 1 {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }

        // Main message loop
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                Command::Shutdown { reply } => {
                    self.handle_shutdown().await;
                    let _ = reply.send(());
                    break;
                }
                Command::Reload { reply } => {
                    let res = self.handle_reload().await;
                    let _ = reply.send(res);
                }
                Command::BatchPrograms { request, reply } => {
                    let res = self.handle_batch_programs(request).await;
                    let _ = reply.send(res);
                }
                Command::CreateProgram { config: req, reply } => {
                    self.handle_create_request(req, reply).await;
                }
                Command::UpdateProgram { id, request, reply } => {
                    let res = self.handle_update(id, request).await;
                    let _ = reply.send(res);
                }
                Command::StartProgram { id, reply } => {
                    if let Some(conf) = self.registry.get_config_mut(&id) {
                        conf.autostart = true;
                        conf.updated_at = chrono::Utc::now().timestamp() as u64;
                    }
                    self.registry.mark_dirty();
                    let res = self
                        .controller
                        .spawn_program(&mut self.registry, id, 0)
                        .await;
                    let _ = reply.send(res);
                }
                Command::StopProgram { id, force, reply } => {
                    let res = self
                        .controller
                        .stop_program(&mut self.registry, id, force)
                        .await;
                    let _ = reply.send(res);
                }
                Command::RestartProgram { id, reply } => {
                    let res = self.handle_restart_request(id).await;
                    let _ = reply.send(res);
                }
                Command::RemoveProgram { id, reply } => {
                    let res = self.handle_remove(id).await;
                    let _ = reply.send(res);
                }
                Command::ListPrograms { reply } => {
                    let summary = self.handle_list();
                    let _ = reply.send(summary);
                }
                Command::GetProgram { id, reply } => {
                    let res = self.handle_get(id);
                    let _ = reply.send(res);
                }
                Command::GetProgramEvents { id, reply } => {
                    let q = crate::event_db::EventQuery {
                        program_id: Some(id),
                        ..Default::default()
                    };
                    let _ = reply.send(self.event_db.query(&q).await.unwrap_or_default());
                }
                Command::QueryEvents { query, reply } => {
                    let _ = reply.send(self.event_db.query(&query).await.unwrap_or_default());
                }
                Command::EventStats { program_id, reply } => {
                    let stats = self.event_db.stats(program_id).await.unwrap_or_default();
                    let _ = reply.send(stats);
                }

                Command::StartGroup { group, reply } => {
                    // 1. Select target IDs
                    let ids: Vec<Uuid> = self
                        .registry
                        .programs
                        .iter()
                        .filter(|(_, cfg)| cfg.group.as_deref() == Some(&group))
                        .map(|(id, _)| *id)
                        .collect();

                    let mut affected = Vec::new();
                    if ids.is_empty() {
                        let _ = reply.send(Err(anyhow::anyhow!("Group not found")));
                    } else {
                        // 2. Batch execute
                        for id in ids {
                            // Enable autostart
                            if let Some(conf) = self.registry.get_config_mut(&id) {
                                conf.autostart = true;
                                conf.updated_at = chrono::Utc::now().timestamp() as u64;
                            }
                            // Start; ignore individual failures
                            if self
                                .controller
                                .spawn_program(&mut self.registry, id, 0)
                                .await
                                .is_ok()
                            {
                                affected.push(id);
                            }
                        }
                        self.registry.mark_dirty();
                        let _ = reply.send(Ok(affected));
                    }
                }
                Command::StopGroup {
                    group,
                    force,
                    reply,
                } => {
                    let ids: Vec<Uuid> = self
                        .registry
                        .programs
                        .iter()
                        .filter(|(_, cfg)| cfg.group.as_deref() == Some(&group))
                        .map(|(id, _)| *id)
                        .collect();

                    let mut affected = Vec::new();
                    if ids.is_empty() {
                        let _ = reply.send(Err(anyhow::anyhow!("Group not found")));
                    } else {
                        for id in ids {
                            // stop_program sets autostart = false internally
                            if self
                                .controller
                                .stop_program(&mut self.registry, id, force)
                                .await
                                .is_ok()
                            {
                                affected.push(id);
                            }
                        }
                        let _ = reply.send(Ok(affected));
                    }
                }
                Command::RestartGroup { group, reply } => {
                    let ids: Vec<Uuid> = self
                        .registry
                        .programs
                        .iter()
                        .filter(|(_, cfg)| cfg.group.as_deref() == Some(&group))
                        .map(|(id, _)| *id)
                        .collect();

                    let mut affected = Vec::new();
                    if ids.is_empty() {
                        let _ = reply.send(Err(anyhow::anyhow!("Group not found")));
                    } else {
                        for id in ids {
                            // Reuse handle_restart_request
                            if self.handle_restart_request(id).await.is_ok() {
                                affected.push(id);
                            }
                        }
                        let _ = reply.send(Ok(affected));
                    }
                }

                Command::ProcessExited {
                    id,
                    pid,
                    code,
                    signal,
                } => {
                    self.handle_exited(id, pid, code, signal).await;
                }
                Command::CheckTimeoutKill { id, target_pid } => {
                    // 1. Check whether forced cleanup is needed
                    let mut force_cleanup = false;

                    // 2. Only if registry still considers process running
                    let pid_match = self.registry.is_running(&id)
                        && self
                            .registry
                            .get_running_all(&id)
                            .iter()
                            .any(|s| s.pid == target_pid);
                    if pid_match {
                        tracing::warn!("Stop timeout reached for {}. Sending SIGKILL.", id);

                        // Send SIGKILL
                        let kill_result = nix::sys::signal::kill(
                            nix::unistd::Pid::from_raw(-(target_pid as i32)),
                            Signal::SIGKILL,
                        );

                        match kill_result {
                            Ok(_) => {
                                // SIGKILL sent; wait for child.wait() -> ProcessExited
                            }
                            Err(nix::errno::Errno::ESRCH) => {
                                // Process already gone
                                // Force cleanup or state stays Stopping forever
                                tracing::warn!(
                                    "Process {} (PID {}) gone during timeout kill. Forcing cleanup.",
                                    id,
                                    target_pid
                                );
                                force_cleanup = true;
                            }
                            Err(e) => {
                                tracing::error!("Failed to SIGKILL {}: {}", id, e);
                            }
                        }
                    }

                    // 3. Force cleanup (avoids borrow conflict above)
                    if force_cleanup {
                        self.handle_exited(id, target_pid, None, None).await;
                    }
                }
                Command::ScheduledRestart { id, retry_count } => {
                    if self.registry.restarting.remove(&id)
                        && let Err(e) = self
                            .controller
                            .spawn_program(&mut self.registry, id, retry_count)
                            .await
                    {
                        tracing::error!("Failed to restart program {}: {}", id, e);
                    }
                }
                Command::HealthCheck { reply } => {
                    let res = self.handle_health_check().await;
                    let _ = reply.send(res);
                }
                Command::InternalHealthUpdate {
                    id,
                    is_healthy,
                    failure_detail,
                } => {
                    self.handle_health_update(id, is_healthy, failure_detail)
                        .await;
                }
                Command::HealthRestart { id, failure_detail } => {
                    self.handle_health_restart(id, failure_detail).await;
                }
                Command::ApplyStack { request, reply } => {
                    let res = self.handle_apply_stack(request).await;
                    let _ = reply.send(res.map(|(logs, _ids)| logs));
                }
                Command::DumpPrograms { reply } => {
                    let configs: Vec<ProgramConfig> =
                        self.registry.programs.values().cloned().collect();
                    let _ = reply.send(configs);
                }
                Command::InternalArtifactReady { id, path } => {
                    self.handle_artifact_ready(id, path).await;
                }
                Command::OtaVerifyTimeout { id } => {
                    self.handle_ota_verify_timeout(id).await;
                }
                Command::CheckWaitingQueue => {
                    self.check_waiting_queue().await;
                }
                Command::SignalProgram { id, signal, reply } => {
                    let res = self.apply_signal(id, signal);
                    let _ = reply.send(res);
                }
                Command::InternalMetricsUpdate { metrics } => {
                    for (id, (cpu, mem)) in metrics {
                        if let Some(state) = self.registry.get_running_mut(&id) {
                            state.cpu_usage = cpu;
                            state.mem_usage = mem;
                        }
                    }
                }
                Command::CronTick => {
                    let triggers = self.scheduler.tick();
                    for t in triggers {
                        let cfg = match self.registry.get_config(&t.id) {
                            Some(c) => c.clone(),
                            None => continue,
                        };
                        let name = cfg.name.clone();
                        let overlap = cfg.on_overlap.unwrap_or_default();
                        let catchup = cfg.catchup.unwrap_or_default();
                        let max_concurrent = cfg.max_concurrent_eff() as usize;
                        let max_queued = cfg.max_queued_eff();

                        // Catchup: how many runs this tick represents. On-time
                        // triggers (missed_slots == 1) always count as one run.
                        let mut runs = 1u32;
                        if t.missed_slots > 1 {
                            runs = match catchup {
                                common::CronCatchup::Skip => 0,
                                common::CronCatchup::Latest => 1,
                                common::CronCatchup::All => t.missed_slots.min(CRON_CATCHUP_CAP),
                            };
                            tracing::info!(
                                "Cron job {} missed {} slot(s); catchup={:?} -> {} run(s)",
                                name,
                                t.missed_slots,
                                catchup,
                                runs
                            );
                        }
                        if runs == 0 {
                            continue;
                        }

                        // Concurrency gate: a firing is admitted whenever fewer
                        // than `max_concurrent` instances are already running.
                        // Only when every slot is taken does `on_overlap` decide
                        // whether to skip, queue (bounded by `max_queued`), or
                        // kill the oldest run for the new one.
                        let active = self.registry.running_count(&t.id);
                        if active >= max_concurrent {
                            match overlap {
                                common::CronOverlap::Skip => {
                                    tracing::warn!(
                                        "Cron job {} is running at max_concurrent={max_concurrent}, skipping this tick.",
                                        name
                                    );
                                    continue;
                                }
                                common::CronOverlap::Queue => {
                                    let queued = self.pending_cron.entry(t.id).or_insert(0);
                                    let dropped = if *queued >= max_queued {
                                        tracing::warn!(
                                            "Cron job {} queue full ({} pending); dropping firing.",
                                            name,
                                            *queued
                                        );
                                        true
                                    } else {
                                        *queued = queued.saturating_add(runs);
                                        tracing::info!(
                                            "Cron job {} is at max_concurrent={max_concurrent}; queued {} run(s).",
                                            name,
                                            runs
                                        );
                                        false
                                    };
                                    if dropped {
                                        self.record_event(
                                            t.id,
                                            &name,
                                            "queue_full",
                                            None,
                                            None,
                                            None,
                                            None,
                                            format!(
                                                "Cron queue full ({max_queued}); firing dropped"
                                            ),
                                        );
                                    }
                                    continue;
                                }
                                common::CronOverlap::Kill => {
                                    tracing::warn!(
                                        "Cron job {} is running at max_concurrent={max_concurrent}; terminating oldest run for the new one.",
                                        name
                                    );
                                    let _ = self.apply_signal_oldest(t.id, Signal::SIGTERM);
                                    let queued = self.pending_cron.entry(t.id).or_insert(0);
                                    let dropped = if *queued >= max_queued {
                                        tracing::warn!(
                                            "Cron job {} queue full ({} pending); dropping firing.",
                                            name,
                                            *queued
                                        );
                                        true
                                    } else {
                                        *queued = queued.saturating_add(runs);
                                        false
                                    };
                                    if dropped {
                                        self.record_event(
                                            t.id,
                                            &name,
                                            "queue_full",
                                            None,
                                            None,
                                            None,
                                            None,
                                            format!(
                                                "Cron queue full ({max_queued}); firing dropped"
                                            ),
                                        );
                                    }
                                    continue;
                                }
                            }
                        }

                        // Admitted: enqueue for the drain pass below.
                        let queued = self.pending_cron.entry(t.id).or_insert(0);
                        let dropped = if *queued >= max_queued {
                            tracing::warn!(
                                "Cron job {} queue full ({} pending); dropping firing.",
                                name,
                                *queued
                            );
                            true
                        } else {
                            *queued = queued.saturating_add(runs);
                            false
                        };
                        if dropped {
                            self.record_event(
                                t.id,
                                &name,
                                "queue_full",
                                None,
                                None,
                                None,
                                None,
                                format!("Cron queue full ({max_queued}); firing dropped"),
                            );
                        }
                    }

                    // Drain the pending queue: spawn as many runs as free
                    // `max_concurrent` slots allow, so admitted firings that
                    // cannot overlap the current instance start as it exits.
                    let due: Vec<(Uuid, u32)> =
                        self.pending_cron.iter().map(|(id, n)| (*id, *n)).collect();
                    for (id, count) in due {
                        let mut remaining = count;
                        while remaining > 0 {
                            let cfg = match self.registry.get_config(&id) {
                                Some(c) => c.clone(),
                                None => {
                                    remaining = 0;
                                    break;
                                }
                            };
                            let max_concurrent = cfg.max_concurrent_eff() as usize;
                            if self.registry.running_count(&id) >= max_concurrent {
                                break; // no free slot right now; wait for a later tick
                            }
                            tracing::info!("Cron job triggered: {}", cfg.name);
                            let cron_start_ms = chrono::Utc::now().timestamp_millis() as u64;
                            if let Err(e) = self
                                .controller
                                .spawn_program(&mut self.registry, id, 0)
                                .await
                            {
                                tracing::error!("Failed to spawn cron job {}: {}", cfg.name, e);
                                self.record_event(
                                    id,
                                    &cfg.name,
                                    "cron_spawn_failed",
                                    None,
                                    None,
                                    None,
                                    None,
                                    format!("Failed to spawn cron job: {}", e),
                                );
                                break;
                            } else {
                                // Record the trigger (run start). The matching
                                // `cron_exit` is recorded when the instance exits.
                                self.record_event(
                                    id,
                                    &cfg.name,
                                    "cron_started",
                                    None,
                                    None,
                                    None,
                                    None,
                                    format!("Cron triggered (started at ms {cron_start_ms})"),
                                );
                                if let Some(cfg) = self.registry.get_config_mut(&id) {
                                    cfg.cron_last_run = Some(chrono::Utc::now().timestamp() as u64);
                                }
                            }
                            remaining -= 1;
                        }
                        if remaining == 0 {
                            self.pending_cron.remove(&id);
                        } else {
                            self.pending_cron.insert(id, remaining);
                        }
                    }
                }
                Command::PersistTick => {
                    if let Err(e) = self.flush_to_disk().await {
                        tracing::error!("Failed to auto-save state: {}", e);
                    }
                    self.maybe_prune_events().await;
                }
                Command::GenerateMetrics { reply } => {
                    let metrics = self.handle_generate_metrics();
                    let _ = reply.send(metrics);
                }
                Command::GetSystemStats { reply } => {
                    let _ = reply.send(self.monitor.system_stats());
                }
            }
        }
        tracing::info!("Manager Loop exited.");
    }

    //
    // Internal Helpers
    //

    // Unified signal delivery
    fn apply_signal(&self, id: Uuid, signal: Signal) -> anyhow::Result<()> {
        let states = self.registry.get_running_all(&id);
        if states.is_empty() {
            return Err(anyhow::anyhow!("Program is not running"));
        }
        for state in states {
            tracing::info!(
                "Sending signal {:?} to program {} (PGID: {})",
                signal,
                id,
                state.pid
            );
            // Negative PID targets the process group
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(-(state.pid as i32)), signal)
                .map_err(|e| anyhow::anyhow!("Failed to signal program {}: {}", id, e))?;
        }
        Ok(())
    }

    /// Send a signal to the oldest running instance only. Used by the cron
    /// `on_overlap = kill` policy, which must free a single `max_concurrent`
    /// slot for the new firing rather than terminate every instance.
    fn apply_signal_oldest(&self, id: Uuid, signal: Signal) -> anyhow::Result<()> {
        let Some(state) = self.registry.get_running(&id) else {
            return Err(anyhow::anyhow!("Program is not running"));
        };
        tracing::info!(
            "Sending signal {:?} to program {} oldest instance (PGID: {})",
            signal,
            id,
            state.pid
        );
        // Negative PID targets the process group
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(-(state.pid as i32)), signal)
            .map_err(|e| anyhow::anyhow!("Failed to signal program {}: {}", id, e))
    }

    //
    // Handlers
    //

    async fn handle_update(&mut self, id: Uuid, req: UpdateProgramRequest) -> anyhow::Result<()> {
        let existing_name = self.registry.get_config(&id).map(|c| c.name.clone());
        validate_update_program_request(&req, &self.config.storage.log_dir).map_err(|e| {
            with_program_location(e, req.name.as_deref().or(existing_name.as_deref()), None)
        })?;
        // Empty cron string is the clear-sentinel; only validate non-empty expressions.
        if let Some(c) = req.cron.as_deref()
            && !c.trim().is_empty()
        {
            self.validate_parameters(Some(c)).map_err(|e| {
                with_program_location(e, req.name.as_deref().or(existing_name.as_deref()), None)
            })?;
        }

        if let Some(limits) = &req.resource_limits {
            validate_resource_limits_patch(limits)?;
            let effective = ResourceLimits {
                cpu_quota: limits.cpu_quota.filter(|&c| c > 0.0),
                memory_limit: limits.memory_limit.filter(|&m| m > 0),
                memory_warn_percent: limits.memory_warn_percent.filter(|&w| w > 0 && w <= 100),
                memory_warn_headroom: limits.memory_warn_headroom.filter(|&h| h > 0),
                memory_high: limits.memory_high.filter(|&h| h > 0),
            };
            warn_if_resource_limits_unenforced(
                self.extension.as_ref(),
                &Some(effective),
                "update program",
            );
        }

        let pid = self.registry.get_running(&id).map(|s| s.pid);

        let old_config = self
            .registry
            .get_config(&id)
            .ok_or_else(|| anyhow::anyhow!("Program not found"))?
            .clone();

        if let Some(v) = &req.name
            && v != &old_config.name
        {
            self.ensure_program_name_available(v, Some(id))?;
        }

        let mut trigger_ota = false;
        let mut artifact_cfg = None;
        let mut _task_name = String::new();

        {
            let config = self
                .registry
                .get_config_mut(&id)
                .ok_or_else(|| anyhow::anyhow!("Program not found"))?;

            // [Trigger Logic] Checksum change detection to trigger OTA.
            // Empty `source` clears artifact (same sentinel pattern as cwd/user/logs).
            if let Some(v) = &req.artifact {
                if v.source.trim().is_empty() {
                    config.artifact = None;
                } else {
                    let old_sum = config
                        .artifact
                        .as_ref()
                        .map(|a| a.checksum.clone())
                        .unwrap_or_default();
                    if v.checksum != old_sum {
                        trigger_ota = true;
                        artifact_cfg = Some(v.clone());
                    }
                    config.artifact = Some(v.clone());
                }
            }

            if let Some(v) = req.name {
                config.name = v;
            }
            if let Some(v) = req.command {
                config.command = v;
            }
            if let Some(v) = req.args {
                config.args = v;
            }
            if let Some(v) = req.env {
                config.env = v;
            }

            if let Some(v) = req.env_file {
                config.env_file = if v.trim().is_empty() { None } else { Some(v) };
            }

            // Empty string for cwd/user/group clears the field
            if let Some(v) = req.cwd {
                config.cwd = if v.trim().is_empty() { None } else { Some(v) };
            }
            if let Some(v) = req.user {
                config.user = if v.trim().is_empty() { None } else { Some(v) };
            }
            if let Some(v) = req.group {
                config.group = if v.trim().is_empty() { None } else { Some(v) };
            }

            // Empty cron clears schedule + related policy (UI Enable-off / cleared expression).
            if let Some(v) = req.cron {
                if v.trim().is_empty() {
                    config.cron = None;
                    config.on_overlap = None;
                    config.catchup = None;
                    config.jitter_sec = None;
                    config.max_concurrent = None;
                    config.max_queued = None;
                    config.cron_last_run = None;
                    self.scheduler.remove(&id);
                    self.pending_cron.remove(&id);
                } else {
                    config.cron = Some(v.clone());
                    let last_run = cron_last_run_dt(config);
                    self.scheduler
                        .upsert(id, &v, config.jitter_sec.unwrap_or(0), last_run);
                }
            }

            if let Some(v) = req.on_overlap {
                config.on_overlap = Some(v);
            }
            if let Some(v) = req.catchup {
                config.catchup = Some(v);
            }
            if let Some(v) = req.jitter_sec {
                config.jitter_sec = Some(v);
                if config.cron.is_some() {
                    self.scheduler.set_jitter(&id, v);
                }
            }
            if let Some(v) = req.max_concurrent {
                config.max_concurrent = Some(v);
            }
            if let Some(v) = req.max_queued {
                config.max_queued = Some(v);
            }

            if let Some(v) = req.autostart {
                config.autostart = v;
            }
            if let Some(v) = req.retry_limit {
                config.retry_limit = v;
            }
            if let Some(v) = req.autorestart {
                config.autorestart = v;
            }
            if let Some(v) = req.exitcodes {
                config.exitcodes = v;
            }
            if let Some(v) = req.startsecs {
                config.startsecs = v;
            }
            if let Some(v) = req.stopsecs {
                config.stopsecs = Some(v);
            }
            if let Some(v) = req.priority {
                config.priority = v;
            }
            if let Some(v) = req.stdout_logfile {
                let new_val = if v.trim().is_empty() { None } else { Some(v) };
                if let Some(ref path) = new_val {
                    resolve_confined_log_path(&self.config.storage.log_dir, path)?;
                }
                config.stdout_logfile = new_val;
            }
            if let Some(v) = req.stderr_logfile {
                let new_val = if v.trim().is_empty() { None } else { Some(v) };
                if let Some(ref path) = new_val {
                    resolve_confined_log_path(&self.config.storage.log_dir, path)?;
                }
                config.stderr_logfile = new_val;
            }

            if let Some(v) = req.depends_on {
                config.depends_on = v;
            }

            if let Some(v) = req.health_check {
                config.health_check = match v {
                    common::HealthCheck::Disabled => None, // Disabled clears health check
                    _ => Some(v),
                };
            }

            if let Some(v) = req.hooks {
                config.hooks = v;
            }

            if let Some(new_limits) = req.resource_limits {
                apply_resource_limits_patch(&mut config.resource_limits, new_limits);
            }

            config.updated_at = chrono::Utc::now().timestamp() as u64;
            _task_name = config.name.clone();
        }

        let new_config = self
            .registry
            .get_config(&id)
            .ok_or_else(|| anyhow::anyhow!("Program not found"))?
            .clone();

        if old_config.resource_limits != new_config.resource_limits {
            self.extension
                .on_update(id, pid, &old_config, &new_config)?;
        }

        self.registry.mark_dirty();
        if let Err(e) = self.flush_to_disk().await {
            tracing::error!("Failed to persist program update for {}: {}", _task_name, e);
        }
        tracing::info!("Program updated: {} ({})", _task_name, id);

        if trigger_ota && let Some(ac) = artifact_cfg {
            let tx = self.tx_self.clone();
            let task_name = _task_name.clone();
            let download_timeout = self.config.server.download_timeout;

            tracing::info!(
                "Triggering OTA update for {} (Timeout: {}s)",
                task_name,
                download_timeout
            );
            tokio::spawn(async move {
                use crate::artifact;
                match artifact::download_to_staging(&ac, download_timeout).await {
                    Ok(path) => {
                        tracing::info!(
                            "OTA Download complete for {}. Staging: {:?}",
                            task_name,
                            path
                        );
                        let _ = tx.send(Command::InternalArtifactReady { id, path }).await;
                    }
                    Err(e) => {
                        tracing::error!("OTA Download failed for {}: {}", task_name, e);
                    }
                }
            });
        }
        Ok(())
    }

    // Transactional artifact apply
    async fn handle_artifact_ready(&mut self, id: Uuid, staging_path: PathBuf) {
        tracing::info!(
            "Artifact ready for program {}. Initiating Transactional Swap...",
            id
        );

        let config = match self.registry.get_config_mut(&id) {
            Some(c) => c,
            None => return,
        };
        let target_path = match config.artifact.as_ref().map(|a| a.destination.clone()) {
            Some(dest) if !dest.is_empty() => PathBuf::from(dest),
            _ => {
                tracing::error!(
                    "OTA apply aborted for {}: artifact destination missing from config.",
                    id
                );
                return;
            }
        };

        // 1. Create backup (hard link)
        use crate::artifact;
        let backup_path = match artifact::create_backup(&target_path).await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Backup failed: {}. Upgrade aborted.", e);
                return;
            }
        };

        // 2. Persist state (WAL)
        // Record backup path so crash recovery can roll back.
        config.restore_path = Some(backup_path.to_string_lossy().to_string());
        config.autostart = true;
        config.updated_at = chrono::Utc::now().timestamp() as u64;

        self.registry.mark_dirty();
        if let Err(e) = self.flush_to_disk().await {
            tracing::error!(
                "Critical: Failed to persist upgrade state: {}. Aborting.",
                e
            );
            return;
        }

        // 3. Atomic swap (overwrite)
        if let Err(e) = artifact::apply_update(&target_path, &staging_path).await {
            tracing::error!("Swap failed: {}. Rolling back state...", e);
            if let Some(cfg) = self.registry.get_config_mut(&id) {
                cfg.restore_path = None;
            }
            return;
        }

        // 4. Restart process
        tracing::info!("Restarting process to load new binary...");
        // Mark intentional restart so handle_exited does not treat upgrade as failed
        if let Some(state) = self.registry.get_running_mut(&id) {
            state.restart_requested = true;
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(state.pid as i32),
                Signal::SIGTERM,
            );
        } else {
            let _ = self
                .controller
                .spawn_program(&mut self.registry, id, 0)
                .await;
        }

        // 5. OTA verification deadline: if the new version is not Healthy within
        //    `server.ota_verify_timeout`, force-kill it so the exit handler rolls
        //    back to the previous binary. Disabled when set to 0.
        let verify_timeout = self.config.server.ota_verify_timeout;
        if verify_timeout > 0 {
            let tx = self.tx_self.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(verify_timeout)).await;
                let _ = tx.send(Command::OtaVerifyTimeout { id }).await;
            });
        }
    }

    /// Fired by the OTA verification timer when the new version did not reach
    /// `Healthy` in time. If the upgrade transaction is still pending and the
    /// process is running (but unhealthy), force-kill it; the exit handler owns
    /// the actual file rollback + restart, so there is a single rollback path.
    async fn handle_ota_verify_timeout(&mut self, id: Uuid) {
        let pending = self
            .registry
            .get_config(&id)
            .and_then(|c| c.restore_path.clone())
            .is_some();
        if !pending {
            // Already committed or rolled back.
            return;
        }
        let Some(state) = self.registry.get_running(&id) else {
            tracing::warn!(
                "OTA verify timeout for {} but process is not running; leaving WAL for next startup.",
                id
            );
            return;
        };
        if state.stopping || state.restart_requested {
            return;
        }
        let name = self
            .registry
            .programs
            .get(&id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        tracing::error!(
            "OTA verification timed out for {} ({}). Force-killing new version; rolling back.",
            name,
            id
        );
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(-(state.pid as i32)),
            Signal::SIGKILL,
        );
    }

    // Process exit handler (OTA commit/rollback & borrow fixes)
    async fn handle_exited(&mut self, id: Uuid, pid: u32, code: Option<i32>, signal: Option<i32>) {
        // 1. Clear runtime state (disambiguate concurrent instances by pid)
        let state = match self.registry.remove_running_by_pid(&id, pid) {
            Some(s) => s,
            None => {
                // A newer instance replaced this one before the exit event
                // arrived (rare race); treat as already handled.
                tracing::warn!(
                    "Program {} (PID {}) exited but no matching running state; ignoring.",
                    id,
                    pid
                );
                return;
            }
        };

        let exited_pid = state.pid;
        let exited_uptime = chrono::Utc::now().timestamp() as u64 - state.start_time;

        // Stop health check task and resource monitor
        if let Some(task) = state.health_task {
            task.abort();
        }
        // If sibling instances remain (scheduled task with `max_concurrent > 1`),
        // keep the monitor pointed at the new primary.
        let siblings = self.registry.get_running_all(&id).len();
        self.monitor.unwatch(&id);
        if siblings > 0
            && let Some(next) = self.registry.get_running(&id)
        {
            self.monitor.watch(id, next.pid);
        }

        tracing::info!(
            "Program exited: {} (PID: {}), Code: {:?}",
            id,
            exited_pid,
            code
        );

        // 2. Extension cleanup hook
        if let Some(cfg) = self.registry.get_config(&id).cloned() {
            let ext = self.extension.clone();
            let cfg_for_ext = cfg.clone();
            tokio::task::spawn_blocking(move || {
                let _ = ext.after_stop(id, &cfg_for_ext);
            });
        }

        // 3. Config snapshot
        // Must .clone() so config borrow does not block mut self later
        let config = match self.registry.get_config(&id) {
            Some(c) => c.clone(),
            None => return,
        };
        let program_name = config.name.clone();

        // 3.5 post_stop lifecycle hook
        if let Some(cmd) = &config.hooks.post_stop {
            let envs = self.controller.build_context(
                id,
                &config,
                Some(exited_pid),
                code,
                Some(exited_uptime),
            );
            let cmd = cmd.clone();
            tokio::spawn(async move {
                let _ = crate::hooks::run_hook(&cmd, &envs).await;
            });
        }

        // 4. OTA transaction handling
        if let Some(backup_file) = &config.restore_path {
            // OTA validation only when not manual stop and not intentional restart
            if !state.stopping && !state.restart_requested {
                // Case A: exit 0 (validation success) -> commit
                if let Some(0) = code {
                    tracing::info!(
                        "OTA Verification: Process {} exited with 0 (Success). Committing upgrade.",
                        program_name
                    );

                    // A1. Delete backup asynchronously
                    let backup_path = PathBuf::from(backup_file);
                    tokio::spawn(async move {
                        use crate::artifact;
                        artifact::commit(&backup_path).await;
                    });

                    // A2. Clear restore_path and flush (safe to take mut self now)
                    if let Some(cfg) = self.registry.get_config_mut(&id) {
                        cfg.restore_path = None;
                    }
                    self.registry.mark_dirty();
                    let _ = self.flush_to_disk().await;

                    // Do not return: exit 0 continues to normal-exit path -> Stopped
                }
                // Case B: non-zero exit (crash) -> rollback
                else {
                    tracing::error!(
                        "Upgrade Validation Failed for {}. Process crashed (Code: {:?}). Initiating ROLLBACK.",
                        program_name,
                        code
                    );

                    let target_path = match config.artifact.as_ref().map(|a| a.destination.clone())
                    {
                        Some(dest) if !dest.is_empty() => PathBuf::from(dest),
                        _ => {
                            tracing::error!(
                                "OTA rollback aborted for {}: artifact destination missing from config.",
                                id
                            );
                            self.registry.crashed.insert(id);
                            return;
                        }
                    };
                    let backup_path = PathBuf::from(backup_file);

                    use crate::artifact;
                    // B1. Roll back files
                    if let Err(e) = artifact::rollback(&target_path, &backup_path).await {
                        tracing::error!(
                            "CRITICAL: File rollback failed: {}. Manual intervention required.",
                            e
                        );
                        self.registry.crashed.insert(id);
                    } else {
                        tracing::info!("File rolled back successfully.");

                        // B2. Clear WAL state and flush
                        if let Some(cfg) = self.registry.get_config_mut(&id) {
                            cfg.restore_path = None;
                        }
                        self.registry.mark_dirty();
                        let _ = self.flush_to_disk().await;

                        // B3. Notify
                        self.record_event(
                            id,
                            &program_name,
                            "process_fatal",
                            code,
                            signal,
                            None,
                            None,
                            "OTA upgrade failed. Automatically rolled back to previous version."
                                .to_string(),
                        );
                        self.emit_event(common::SystemEvent::ProcessFatal {
                            program_id: id,
                            program_name: program_name.clone(),
                            pid: Some(exited_pid),
                            uptime_secs: exited_uptime,
                            exit_code: code,
                            signal,
                            msg:
                                "OTA upgrade failed. Automatically rolled back to previous version."
                                    .to_string(),
                            log_tail: None,
                        });

                        // B4. Restart previous version
                        tracing::info!("Restarting with stable version...");
                        let _ = self
                            .controller
                            .spawn_program(&mut self.registry, id, 0)
                            .await;
                    }
                    return; // rollback done; skip remaining exit logic
                }
            }
        }

        // 5. Cron job handling
        // Intentional stop/restart must run before the cron early-return — otherwise
        // Restart API sets restart_requested, the process dies, and we never respawn.
        if config.cron.is_some() {
            if state.restart_requested {
                tracing::info!("Restarting cron program {} immediately...", id);
                let _ = self
                    .controller
                    .spawn_program(&mut self.registry, id, 0)
                    .await;
                return;
            }
            if state.stopping {
                let _ = self.log_tx.send(WsMessage::StatusChange {
                    id,
                    status: ProcessStatus::Stopped,
                    name: program_name.clone(),
                });
                return;
            }
            // With `max_concurrent > 1`, sibling instances may still be running;
            // only flip the status when the last instance exits.
            let siblings = self.registry.running_count(&id);
            let cron_duration = Some(exited_uptime.max(1));
            match code {
                Some(0) => {
                    tracing::info!(
                        "Cron job '{}' finished successfully{}.",
                        program_name,
                        if siblings > 0 {
                            " (sibling still running)"
                        } else {
                            ""
                        }
                    );
                    self.record_event(
                        id,
                        &program_name,
                        "cron_exit",
                        code,
                        signal,
                        None,
                        cron_duration,
                        format!("Cron job finished successfully (ran {}s)", exited_uptime),
                    );
                    if siblings == 0 {
                        let _ = self.log_tx.send(WsMessage::StatusChange {
                            id,
                            status: ProcessStatus::Stopped,
                            name: program_name.clone(),
                        });
                    }
                }
                _ => {
                    tracing::error!("Cron job '{}' failed with code {:?}.", program_name, code);
                    self.record_event(
                        id,
                        &program_name,
                        "cron_exit",
                        code,
                        signal,
                        None,
                        cron_duration,
                        format!(
                            "Cron job failed with code {:?} after {}s",
                            code, exited_uptime
                        ),
                    );
                    let event = common::SystemEvent::ProcessFatal {
                        program_id: id,
                        program_name: program_name.clone(),
                        pid: Some(exited_pid),
                        uptime_secs: exited_uptime,
                        exit_code: code,
                        signal,
                        msg: "Cron job execution failed".to_string(),
                        log_tail: None,
                    };
                    self.emit_event(event);
                    if siblings == 0 {
                        let _ = self.log_tx.send(WsMessage::StatusChange {
                            id,
                            status: ProcessStatus::Fatal,
                            name: program_name.clone(),
                        });
                    }
                }
            }
            return; // cron jobs do not auto-restart
        }

        // 6. User/system initiated stop/restart
        // Case: Restart API (intentional restart)
        if state.restart_requested {
            tracing::info!("Restarting program {} immediately...", id);
            let _ = self
                .controller
                .spawn_program(&mut self.registry, id, 0)
                .await;
            return;
        }

        // Case: Stop API (intentional stop)
        if state.stopping {
            let _ = self.log_tx.send(WsMessage::StatusChange {
                id,
                status: ProcessStatus::Stopped,
                name: program_name.clone(),
            });
            return;
        }

        // 7. Exit does not warrant auto-restart (Supervisor autorestart / exitcodes)
        if !config.should_autorestart(code) {
            tracing::info!(
                "Program {} exited (code {:?}). Not restarting (autorestart={:?}).",
                program_name,
                code,
                config.autorestart
            );
            let _ = self.log_tx.send(WsMessage::StatusChange {
                id,
                status: ProcessStatus::Stopped,
                name: program_name.clone(),
            });

            // Persist the exit so every run leaves a trace in history.
            let msg = match signal {
                Some(sig) => format!("Process killed by signal {} (exit code {:?})", sig, code),
                None => format!("Process exited with code {:?}", code),
            };
            self.record_event(
                id,
                &program_name,
                "process_exit",
                code,
                signal,
                None,
                None,
                msg,
            );
            return;
        }

        // 8. Crash handling (backoff retry)
        let retry_limit = config.retry_limit;
        // startsecs: stable run resets retry counter (Supervisor startsecs)
        let uptime = exited_uptime;
        let retry_count_to_use = if uptime >= config.startsecs as u64 {
            0
        } else {
            state.retry_count + 1
        };

        if retry_count_to_use > retry_limit {
            // A. Retries exhausted -> Fatal
            self.registry.crashed.insert(id);
            tracing::error!(
                "Program {} failed too many times. Entering FATAL state.",
                id
            );

            if let Some(cfg) = self.registry.get_config_mut(&id) {
                cfg.autostart = false; // prevent auto-start on next Manager restart
                cfg.updated_at = chrono::Utc::now().timestamp() as u64;
            }

            // Record crash reason in startup_errors for UI error display
            let err_msg = format!(
                "Stopped after {} retries. Last exit code: {:?}",
                retry_count_to_use, code
            );
            self.registry.startup_errors.insert(id, err_msg.clone());
            self.record_event(
                id,
                &program_name,
                "process_fatal",
                code,
                signal,
                Some(retry_count_to_use),
                None,
                err_msg.clone(),
            );

            self.registry.mark_dirty();

            let _ = self.log_tx.send(WsMessage::StatusChange {
                id,
                status: ProcessStatus::Fatal,
                name: program_name.clone(),
            });

            // Read log tail asynchronously and fire alert
            let extension = self.extension.clone();
            let name_clone = program_name.clone();
            let log_dir = self.config.storage.log_dir.clone();
            let stdout_logfile = config.stdout_logfile.clone();
            let stderr_logfile = config.stderr_logfile.clone();

            let hooks = self.config.event_hooks.clone();
            let fatal_pid = exited_pid;
            let fatal_uptime = exited_uptime;

            tokio::spawn(async move {
                use crate::logger;
                let log_tail = logger::read_log_tail(
                    &log_dir,
                    id,
                    logger::LogSource::Stderr,
                    2048,
                    stdout_logfile.as_deref(),
                    stderr_logfile.as_deref(),
                )
                .await;
                let event = common::SystemEvent::ProcessFatal {
                    program_id: id,
                    program_name: name_clone,
                    pid: Some(fatal_pid),
                    uptime_secs: fatal_uptime,
                    exit_code: code,
                    signal,
                    msg: format!("Stopped after {} retries.", retry_count_to_use),
                    log_tail,
                };
                crate::event_hooks::emit(&extension, &hooks, event);
            });

            // Trigger immediate persist
            let tx = self.tx_self.clone();
            tokio::spawn(async move {
                let _ = tx.send(Command::PersistTick).await;
            });
        } else {
            // B. Retries remaining -> backoff
            self.registry.restarting.insert(id);
            // Exponential backoff: 1s, 2s, 4s, ... max 60s
            let delay_sec = std::cmp::min(1 << (retry_count_to_use.saturating_sub(1)), 60);
            tracing::warn!(
                "Program {} crashed. Backoff {}s (Retry {})",
                id,
                delay_sec,
                retry_count_to_use
            );

            let _ = self.log_tx.send(WsMessage::StatusChange {
                id,
                status: ProcessStatus::Backoff,
                name: program_name.clone(),
            });

            self.record_event(
                id,
                &program_name,
                "process_backoff",
                code,
                signal,
                Some(retry_count_to_use),
                None,
                format!("Crash detected, retrying in {}s", delay_sec),
            );

            let event = common::SystemEvent::ProcessBackoff {
                program_id: id,
                program_name,
                pid: Some(exited_pid),
                uptime_secs: exited_uptime,
                exit_code: code,
                signal,
                retry_count: retry_count_to_use,
            };
            self.emit_event(event);

            let tx = self.tx_self.clone();
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(delay_sec)).await;
                let _ = tx
                    .send(Command::ScheduledRestart {
                        id,
                        retry_count: retry_count_to_use,
                    })
                    .await;
            });
        }
    }

    // Health Check Commit
    async fn handle_health_update(
        &mut self,
        id: Uuid,
        is_healthy: bool,
        failure_detail: Option<String>,
    ) {
        // Snapshot config-derived values before mutating runtime state so the
        // `get_running_mut` borrow does not span other registry reads.
        let program_name = self
            .registry
            .programs
            .get(&id)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let log_cfg = self
            .registry
            .programs
            .get(&id)
            .map(|c| (c.stdout_logfile.clone(), c.stderr_logfile.clone()));

        let mut recovery: Option<(u32, u64)> = None;
        {
            let state = match self.registry.get_running_mut(&id) {
                Some(s) => s,
                None => return,
            };
            // Ignore health updates while stopping
            // Prevents Stop -> Stopping -> (health race) -> Healthy
            if state.stopping {
                return;
            }

            if !is_healthy {
                if let Some(detail) = failure_detail {
                    let changed = state.health_error.as_deref() != Some(detail.as_str());
                    state.health_error = Some(detail.clone());
                    if changed {
                        let log_dir = &self.config.storage.log_dir;
                        let log_timestamp = self.config.child_logging.timestamp;
                        let (stdout_logfile, stderr_logfile) = log_cfg
                            .as_ref()
                            .map(|c| (c.0.as_deref(), c.1.as_deref()))
                            .unwrap_or((None, None));
                        crate::logger::emit_superd_line(
                            id,
                            &format!("health_check failed: {detail}"),
                            log_dir,
                            stdout_logfile,
                            stderr_logfile,
                            log_timestamp,
                            &self.log_tx,
                        )
                        .await;
                    }
                }
            } else {
                state.health_error = None;
            }

            if state.is_healthy != is_healthy {
                state.is_healthy = is_healthy;

                let display_status = if is_healthy {
                    ProcessStatus::Healthy
                } else {
                    ProcessStatus::Running
                };
                tracing::info!("Program {} health changed: {}", program_name, is_healthy);

                let _ = self.log_tx.send(WsMessage::StatusChange {
                    id,
                    status: display_status,
                    name: program_name.clone(),
                });

                if is_healthy && state.alert_pending_recovery {
                    state.alert_pending_recovery = false;
                    let recovered_pid = state.pid;
                    let uptime = chrono::Utc::now().timestamp() as u64 - state.start_time;
                    recovery = Some((recovered_pid, uptime));
                }
            }
        }

        if let Some((recovered_pid, uptime)) = recovery {
            self.record_event(
                id,
                &program_name,
                "process_recovered",
                None,
                None,
                None,
                None,
                format!("Process recovered and healthy (up {}s)", uptime),
            );
            self.emit_event(common::SystemEvent::ProcessRecovered {
                program_id: id,
                program_name: program_name.clone(),
                pid: Some(recovered_pid),
                uptime_sec: uptime,
            });
            tracing::info!("Program {} has RECOVERED from crash!", program_name);
        }

        // Commit Upgrade Transaction
        // If program is healthy and has a pending restore path, commit the upgrade.
        if is_healthy {
            // The program recovered; reset the health-restart counter so a
            // later failure cycle starts fresh against `retry_limit`.
            self.registry.health_restart_count.remove(&id);

            let mut backup_to_delete = None;
            if let Some(cfg) = self.registry.get_config_mut(&id)
                && let Some(backup) = cfg.restore_path.take()
            {
                backup_to_delete = Some(backup);
                tracing::info!("Upgrade verified for {}. Committing changes.", id);
            }

            if let Some(backup) = backup_to_delete {
                // Persist clean state (restore_path removed)
                self.registry.mark_dirty();
                let _ = self.flush_to_disk().await;

                // Async delete backup
                tokio::spawn(async move {
                    use crate::artifact;
                    artifact::commit(Path::new(&backup)).await;
                });
            }

            let tx = self.tx_self.clone();
            tokio::spawn(async move {
                let _ = tx.send(Command::CheckWaitingQueue).await;
            });
        }
    }

    /// Auto-restart triggered by `max_failures` consecutive health-check
    /// failures. Restarts are counted per program and guarded by `retry_limit`:
    /// once a program stays unhealthy across that many health restarts, it
    /// enters the Fatal state instead of restarting forever. The counter resets
    /// as soon as the program reports healthy again (`handle_health_update`).
    async fn handle_health_restart(&mut self, id: Uuid, failure_detail: String) {
        // Guards: program must exist, still be running, and not be in the
        // middle of a stop/restart already.
        if self.registry.running_count(&id) == 0 {
            return;
        }
        let Some(config) = self.registry.programs.get(&id).cloned() else {
            return;
        };
        let states = self.registry.get_running_all(&id);
        if states.is_empty() || states.iter().any(|s| s.stopping || s.restart_requested) {
            return;
        }
        let state = &states[0];
        let now = chrono::Utc::now().timestamp() as u64;
        let program_name = config.name.clone();
        let pid = state.pid;
        let uptime_secs = now.saturating_sub(state.start_time);

        let retry_count = self
            .registry
            .health_restart_count
            .get(&id)
            .copied()
            .unwrap_or(0)
            + 1;
        let retry_limit = config.retry_limit;

        if retry_count > retry_limit {
            // Give up: mark Fatal and stop the unhealthy process. `stopping` is
            // set by stop_program so exit handling does not auto-restart it.
            tracing::error!(
                "Program {} failed health checks too many times. Entering FATAL state.",
                program_name
            );
            self.registry.health_restart_count.remove(&id);
            self.registry.crashed.insert(id);
            if let Some(cfg) = self.registry.get_config_mut(&id) {
                cfg.autostart = false; // prevent auto-start on next Manager restart
                cfg.updated_at = now;
            }
            let err_msg = format!(
                "Stopped after {retry_count} health restarts. Last failure: {failure_detail}"
            );
            self.registry.startup_errors.insert(id, err_msg.clone());
            self.record_event(
                id,
                &program_name,
                "process_fatal",
                None,
                None,
                Some(retry_count),
                None,
                err_msg.clone(),
            );
            self.emit_event(common::SystemEvent::ProcessFatal {
                program_id: id,
                program_name: program_name.clone(),
                pid: Some(pid),
                uptime_secs,
                exit_code: None,
                signal: None,
                msg: err_msg,
                log_tail: None,
            });
            let _ = self.log_tx.send(WsMessage::StatusChange {
                id,
                status: ProcessStatus::Fatal,
                name: program_name.clone(),
            });
            self.registry.mark_dirty();
            let _ = self
                .controller
                .stop_program(&mut self.registry, id, true)
                .await;
            return;
        }

        self.registry.health_restart_count.insert(id, retry_count);

        // Record the trigger event so operators see why the restart happened,
        // then terminate the process(es); exit handling respawns immediately
        // via the `restart_requested` path.
        self.record_event(
            id,
            &program_name,
            "health_restart",
            None,
            None,
            Some(retry_count),
            None,
            format!("Health check failed {retry_count} time(s): {failure_detail}"),
        );
        self.emit_event(common::SystemEvent::HealthRestart {
            program_id: id,
            program_name: program_name.clone(),
            pid: Some(pid),
            uptime_secs,
            retry_count,
            msg: failure_detail,
        });

        let pids: Vec<u32> = self
            .registry
            .get_running_all(&id)
            .iter()
            .map(|s| s.pid)
            .collect();
        if let Some(states) = self.registry.get_running_all_mut(&id) {
            for s in states.iter_mut() {
                s.restart_requested = true;
            }
        }
        for state_pid in &pids {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(*state_pid as i32),
                Signal::SIGTERM,
            );
        }
        // Arm a force-kill timer in case the process ignores SIGTERM.
        let tx = self.tx_self.clone();
        let target_pid = pids.first().copied().unwrap_or(0);
        let timeout_sec = self.controller.stop_timeout(&self.registry, id);
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(timeout_sec)).await;
            let _ = tx.send(Command::CheckTimeoutKill { id, target_pid }).await;
        });
    }

    async fn handle_apply_stack(
        &mut self,
        req: StackApplyRequest,
    ) -> anyhow::Result<(Vec<String>, Vec<Uuid>)> {
        let mut logs = Vec::new();
        let mut touched_programs = HashSet::new();
        let mut affected_ids = Vec::new();

        self.validate_stack_service_names(&req.services)?;

        for (i, service_req) in req.services.iter().enumerate() {
            validate_create_program_request(service_req, &self.config.storage.log_dir)
                .map_err(|e| with_program_location(e, service_req.name.as_deref(), Some(i)))?;
            self.validate_parameters(service_req.cron.as_deref())
                .map_err(|e| with_program_location(e, service_req.name.as_deref(), Some(i)))?;
            warn_if_resource_limits_unenforced(
                self.extension.as_ref(),
                &service_req.resource_limits,
                "apply stack",
            );
        }

        for service_req in req.services {
            let expanded_configs = self.expand_request(&service_req);

            for config in expanded_configs {
                let name = config.name.clone();
                touched_programs.insert(name.clone());

                let existing_id = self
                    .registry
                    .programs
                    .iter()
                    .find(|(_, cfg)| cfg.name == name)
                    .map(|(id, _)| *id);

                if let Some(id) = existing_id {
                    affected_ids.push(id);
                    logs.push(format!("Updating service: {}", name));
                    // Construct Update Request to trigger potential OTA
                    #[allow(unused_mut)]
                    let mut update_req = UpdateProgramRequest {
                        name: Some(config.name),
                        command: Some(config.command),
                        args: Some(config.args),
                        env: Some(config.env),
                        cwd: config.cwd,
                        user: config.user,
                        autostart: Some(config.autostart),
                        retry_limit: Some(config.retry_limit),
                        autorestart: Some(config.autorestart),
                        exitcodes: Some(config.exitcodes.clone()),
                        startsecs: Some(config.startsecs),
                        stopsecs: config.stopsecs,
                        priority: Some(config.priority),
                        stdout_logfile: config.stdout_logfile.clone(),
                        stderr_logfile: config.stderr_logfile.clone(),
                        group: config.group,
                        depends_on: Some(config.depends_on),
                        health_check: config.health_check,
                        hooks: Some(config.hooks),
                        artifact: config.artifact,
                        cron: config.cron,
                        on_overlap: config.on_overlap,
                        catchup: config.catchup,
                        jitter_sec: config.jitter_sec,
                        max_concurrent: config.max_concurrent,
                        max_queued: config.max_queued,

                        ..Default::default()
                    };

                    update_req.resource_limits = config.resource_limits;

                    if let Err(e) = self.handle_update(id, update_req).await {
                        logs.push(format!("Failed to update {}: {}", name, e));
                        tracing::error!("Failed to update {}: {}", name, e);
                    }
                } else {
                    logs.push(format!("Creating service: {}", name));
                    let id = Uuid::new_v4();
                    affected_ids.push(id);
                    let mut should_start = config.autostart;

                    if let Some(cron_expr) = &config.cron {
                        should_start = false;
                        let jitter = config.jitter_sec.unwrap_or(0);
                        self.scheduler.upsert(id, cron_expr, jitter, None);
                        tracing::info!("Cron job '{}' registered via stack apply.", name);
                    }

                    self.registry.programs.insert(id, config);
                    if should_start
                        && let Err(e) = self
                            .controller
                            .spawn_program(&mut self.registry, id, 0)
                            .await
                    {
                        tracing::error!("Failed to autostart {}: {}", name, e);
                        logs.push(format!("Failed to start {}: {}", name, e));
                    }
                }
            }
        }

        if req.prune {
            let mut ids_to_remove = Vec::new();
            for (id, cfg) in &self.registry.programs {
                if !touched_programs.contains(&cfg.name) {
                    ids_to_remove.push(*id);
                }
            }
            for id in ids_to_remove {
                let name_str = self
                    .registry
                    .programs
                    .get(&id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                logs.push(format!("Pruning service: {} ({})", name_str, id));
                if let Err(e) = self.handle_remove(id).await {
                    logs.push(format!("Failed to prune {}: {}", name_str, e));
                }
            }
        }

        self.registry.mark_dirty();
        if let Err(e) = self.flush_to_disk().await {
            tracing::error!("Failed to persist stack apply: {}", e);
        }
        Ok((logs, affected_ids))
    }

    async fn handle_shutdown(&mut self) {
        tracing::info!("System shutting down...");

        if let Err(e) = self.flush_to_disk().await {
            tracing::error!("Failed to save state during shutdown: {}", e);
        }

        if let Err(e) = self.extension.on_shutdown() {
            tracing::error!("Extension shutdown hook failed: {}", e);
        }
        self.emit_event(common::SystemEvent::SystemShutdown);

        let order = self.get_shutdown_order();
        let total = order.len();
        tracing::info!("Shutdown plan computed for {} services.", total);

        for (i, id) in order.iter().enumerate() {
            if self.registry.is_running(id) {
                if let Some(conf) = self.registry.get_config(id) {
                    tracing::info!("[{}/{}] Stopping {}...", i + 1, total, conf.name);
                }
                if let Err(e) = self
                    .controller
                    .stop_program(&mut self.registry, *id, false)
                    .await
                {
                    tracing::error!("Failed to stop program {}: {}", id, e);
                }
            }
        }

        let deadline = tokio::time::Instant::now()
            + std::time::Duration::from_secs(self.config.server.shutdown_timeout);
        let check_interval = std::time::Duration::from_millis(100);

        tracing::info!("Waiting for processes to exit...");

        loop {
            if self.registry.running_empty() {
                tracing::info!("All processes exited cleanly.");
                break;
            }

            if tokio::time::Instant::now() > deadline {
                let running = self.registry.all_running_pids();
                tracing::warn!(
                    "Shutdown timeout reached. {} processes still running.",
                    running.len()
                );
                for (_id, state_pid) in running {
                    tracing::warn!("Force killing PID {}", state_pid);
                    let _ = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(state_pid as i32),
                        Signal::SIGKILL,
                    );
                }
                break;
            }

            match self.rx.try_recv() {
                Ok(cmd) => {
                    if let Command::ProcessExited {
                        id,
                        pid,
                        code,
                        signal,
                    } = cmd
                    {
                        self.handle_exited(id, pid, code, signal).await;
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    tokio::time::sleep(check_interval).await;
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    break;
                }
            }
        }
        self.record_system_event("system_shutdown", "Daemon stopped".to_string());
        tracing::info!("Bye!");
    }

    fn get_shutdown_order(&self) -> Vec<Uuid> {
        let mut adj: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        let mut in_degree: HashMap<Uuid, usize> = HashMap::new();
        let mut id_map: HashMap<String, Uuid> = HashMap::new();

        for (id, config) in &self.registry.programs {
            id_map.insert(config.name.clone(), *id);
            in_degree.insert(*id, 0);
            adj.insert(*id, Vec::new());
        }

        for (id, config) in &self.registry.programs {
            for dep_name in &config.depends_on {
                if let Some(dep_id) = id_map.get(dep_name) {
                    // `dep_id` and `id` were both inserted above, so these
                    // entries always exist; skip defensively instead of panic.
                    if let Some(edges) = adj.get_mut(dep_id) {
                        edges.push(*id);
                    }
                    if let Some(deg) = in_degree.get_mut(id) {
                        *deg += 1;
                    }
                }
            }
        }

        let mut queue: Vec<Uuid> = in_degree
            .iter()
            .filter(|&(_, &deg)| deg == 0)
            .map(|(id, _)| *id)
            .collect();
        queue.sort();
        let mut start_order = Vec::new();

        while let Some(u) = queue.pop() {
            start_order.push(u);
            if let Some(neighbors) = adj.get(&u) {
                for &v in neighbors {
                    // `v` came from `adj`, whose keys are all registered ids.
                    if let Some(deg) = in_degree.get_mut(&v) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push(v);
                        }
                    }
                }
            }
        }

        if start_order.len() < self.registry.programs.len() {
            for id in self.registry.programs.keys() {
                if !start_order.contains(id) {
                    start_order.push(*id);
                }
            }
        }
        start_order.reverse();
        start_order
    }

    async fn handle_create_request(
        &mut self,
        req: CreateProgramRequest,
        reply: tokio::sync::oneshot::Sender<anyhow::Result<Vec<Uuid>>>,
    ) {
        if let Err(e) = validate_create_program_request(&req, &self.config.storage.log_dir) {
            let e = with_program_location(e, req.name.as_deref(), None);
            tracing::warn!("CreateProgram validation failed: {}", e);
            let _ = reply.send(Err(e));
            return;
        }
        if let Err(e) = self.validate_parameters(req.cron.as_deref()) {
            let e = with_program_location(e, req.name.as_deref(), None);
            tracing::warn!("CreateProgram validation failed: {}", e);
            let _ = reply.send(Err(e));
            return;
        }

        let configs = self.expand_request(&req);
        for cfg in &configs {
            if let Err(e) = self.ensure_program_name_available(&cfg.name, None) {
                tracing::warn!("CreateProgram name conflict: {}", e);
                let _ = reply.send(Err(e));
                return;
            }
            warn_if_resource_limits_unenforced(
                self.extension.as_ref(),
                &cfg.resource_limits,
                "create program",
            );
        }

        let mut created_ids = Vec::new();
        for config in configs {
            let id = Uuid::new_v4();
            let should_start = config.autostart;
            let name = config.name.clone();

            if let Some(cron_expr) = &config.cron {
                let jitter = config.jitter_sec.unwrap_or(0);
                self.scheduler.upsert(id, cron_expr, jitter, None);
            }

            self.registry.programs.insert(id, config);
            created_ids.push(id);
            tracing::info!("Program created: {} ({})", name, id);

            if should_start
                && self.scheduler.get_next_run(&id).is_none()
                && let Err(e) = self
                    .controller
                    .spawn_program(&mut self.registry, id, 0)
                    .await
            {
                tracing::error!("Failed to autostart {}: {}", id, e);
            }
        }
        self.registry.mark_dirty();
        if let Err(e) = self.flush_to_disk().await {
            tracing::error!("Failed to persist new program(s): {}", e);
        }
        let _ = reply.send(Ok(created_ids));
    }

    async fn handle_reload(&mut self) -> anyhow::Result<Vec<ProgramSummary>> {
        tracing::info!("Reloading configuration from {:?}", self.config_path);
        let content = tokio::fs::read_to_string(&self.config_path).await?;
        let new_config: ServerConfig = toml::from_str(&content)?;

        if new_config.logging.log_level != self.config.logging.log_level {
            tracing::info!(
                "Updating log level: {} -> {}",
                self.config.logging.log_level,
                new_config.logging.log_level
            );
            (self.log_reloader)(new_config.logging.log_level.clone())?;
        }
        self.config = new_config.clone();
        self.controller.config = new_config;

        if let Err(e) = self.extension.on_reload() {
            tracing::error!("Failed to reload extension: {}", e);
        }
        let affected_ids = match self.process_includes().await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!("Failed to process includes during reload: {}", e);
                Vec::new()
            }
        };

        let affected: Vec<ProgramSummary> = affected_ids
            .into_iter()
            .filter_map(|id| {
                self.registry
                    .programs
                    .get(&id)
                    .map(|cfg| self.build_summary(&id, cfg))
            })
            .collect();

        tracing::info!(
            "Configuration reloaded successfully ({} affected program(s)).",
            affected.len()
        );
        Ok(affected)
    }

    async fn flush_to_disk(&mut self) -> anyhow::Result<()> {
        if self.registry.dirty {
            store::save(&self.config.storage.data_file, &self.registry.programs).await?;
            self.registry.dirty = false;
        }
        tracing::debug!("State persisted to disk (Debounced).");
        Ok(())
    }

    fn handle_list(&self) -> Vec<ProgramSummary> {
        let mut list = Vec::new();
        for (id, config) in &self.registry.programs {
            list.push(self.build_summary(id, config));
        }
        list
    }

    fn build_summary(&self, id: &Uuid, config: &ProgramConfig) -> ProgramSummary {
        let (status, pid, uptime, cpu, mem) = if let Some(state) = self.registry.get_running(id) {
            let now = chrono::Utc::now().timestamp() as u64;

            let s = if state.stopping {
                ProcessStatus::Stopping
            } else if state.is_healthy {
                ProcessStatus::Healthy
            } else {
                ProcessStatus::Running
            };

            (
                s,
                Some(state.pid),
                Some(now.saturating_sub(state.start_time)),
                Some(state.cpu_usage),
                Some(state.mem_usage),
            )
        } else if self.registry.restarting.contains(id) {
            (ProcessStatus::Backoff, None, None, None, None)
        } else if self.registry.waiting.contains(id) {
            (ProcessStatus::Waiting, None, None, None, None)
        } else if self.registry.crashed.contains(id) {
            (ProcessStatus::Fatal, None, None, None, None)
        } else {
            (ProcessStatus::Stopped, None, None, None, None)
        };

        ProgramSummary {
            id: *id,
            name: config.name.clone(),
            group: config.group.clone(),
            status,
            pid,
            uptime_sec: uptime,
            created_at: config.created_at,
            updated_at: config.updated_at,
            last_error: self.registry.startup_errors.get(id).cloned(),
            health_error: self
                .registry
                .get_running(id)
                .and_then(|s| s.health_error.clone()),
            cpu_usage: cpu,
            mem_usage: mem,
            depends_on: config.depends_on.clone(),
            resource_limits: config.resource_limits.clone(),
        }
    }

    fn handle_get(&self, id: Uuid) -> anyhow::Result<ProgramInfo> {
        let config = self
            .registry
            .get_config(&id)
            .ok_or_else(|| anyhow::anyhow!("Program not found"))?;

        let (status, pid) = if let Some(state) = self.registry.get_running(&id) {
            let s = if state.stopping {
                ProcessStatus::Stopping
            } else if state.is_healthy {
                ProcessStatus::Healthy
            } else {
                ProcessStatus::Running
            };

            (s, Some(state.pid))
        } else if self.registry.restarting.contains(&id) {
            (ProcessStatus::Backoff, None)
        } else if self.registry.waiting.contains(&id) {
            (ProcessStatus::Waiting, None)
        } else if self.registry.crashed.contains(&id) {
            (ProcessStatus::Fatal, None)
        } else {
            (ProcessStatus::Stopped, None)
        };

        Ok(ProgramInfo {
            id,
            state: status,
            pid,
            config: config.clone(),
            last_error: self.registry.startup_errors.get(&id).cloned(),
            health_error: self
                .registry
                .get_running(&id)
                .and_then(|s| s.health_error.clone()),
        })
    }

    async fn handle_restart_request(&mut self, id: Uuid) -> anyhow::Result<()> {
        if self.registry.is_running(&id) {
            tracing::info!(
                "Restart requested for {}. Stopping current process(es)...",
                id
            );
            let pids: Vec<u32> = self
                .registry
                .get_running_all(&id)
                .iter()
                .map(|s| s.pid)
                .collect();
            if let Some(states) = self.registry.get_running_all_mut(&id) {
                for state in states.iter_mut() {
                    state.restart_requested = true;
                }
            }
            for state_pid in &pids {
                let _ = nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(*state_pid as i32),
                    Signal::SIGTERM,
                );
            }

            let tx = self.tx_self.clone();
            let target_pid = *pids.first().unwrap_or(&0);
            let timeout_sec = self.controller.stop_timeout(&self.registry, id);
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(timeout_sec)).await;
                let _ = tx.send(Command::CheckTimeoutKill { id, target_pid }).await;
            });
            return Ok(());
        }

        tracing::info!("Program {} is not running. Starting directly...", id);
        if let Some(cfg) = self.registry.get_config_mut(&id) {
            cfg.autostart = true;
            cfg.updated_at = chrono::Utc::now().timestamp() as u64;
        }
        self.registry.mark_dirty();
        self.controller
            .spawn_program(&mut self.registry, id, 0)
            .await
    }

    async fn handle_remove(&mut self, id: Uuid) -> anyhow::Result<()> {
        if self.registry.is_running(&id) {
            return Err(anyhow::anyhow!("Cannot remove running program"));
        }
        let config_opt = self.registry.programs.remove(&id);
        if config_opt.is_none() {
            return Err(anyhow::anyhow!("Program not found"));
        }

        self.registry.restarting.remove(&id);
        self.registry.waiting.remove(&id);
        self.registry.crashed.remove(&id);
        self.registry.startup_errors.remove(&id);
        self.registry.health_restart_count.remove(&id);
        self.scheduler.remove(&id);
        self.pending_cron.remove(&id);
        self.registry.mark_dirty();

        // Drop the program's persisted event history.
        if let Err(e) = self.event_db.delete_program(id).await {
            tracing::warn!("Failed to delete event history for {}: {}", id, e);
        }

        if let Some(cfg) = config_opt {
            let extension = self.extension.clone();
            tokio::task::spawn_blocking(move || {
                if let Err(e) = extension.after_stop(id, &cfg) {
                    tracing::warn!("Extension cleanup failed for removed program {}: {}", id, e);
                }
            });
        }
        tracing::info!("Program removed: {}", id);
        Ok(())
    }

    async fn process_includes(&mut self) -> anyhow::Result<Vec<Uuid>> {
        let patterns = self.config.include.files.clone();
        if patterns.is_empty() {
            return Ok(Vec::new());
        }
        let root = crate::resolve_root();
        let mut affected = Vec::new();

        for pattern in patterns {
            let pattern_path = std::path::Path::new(&pattern);
            let full_pattern = if pattern_path.is_relative() {
                root.join(pattern).to_string_lossy().to_string()
            } else if pattern_path.starts_with(&root) {
                pattern
            } else {
                tracing::warn!("Skipping include pattern outside SUPER_ROOT: {}", pattern);
                continue;
            };
            if let Ok(paths) = glob(&full_pattern) {
                for entry in paths.flatten() {
                    if let Ok(content) = tokio::fs::read_to_string(&entry).await
                        && let Ok(stack) = common::parse_stack_from_str(&content, &entry)
                    {
                        match self.handle_apply_stack(stack).await {
                            Ok((_logs, ids)) => affected.extend(ids),
                            Err(e) => {
                                tracing::error!("Failed to apply include stack {:?}: {}", entry, e)
                            }
                        }
                    }
                }
            }
        }
        Ok(affected)
    }

    async fn check_waiting_queue(&mut self) {
        let mut waiting_ids: Vec<(i32, Uuid)> = self
            .registry
            .waiting
            .iter()
            .filter_map(|id| self.registry.get_config(id).map(|cfg| (cfg.priority, *id)))
            .collect();
        if waiting_ids.is_empty() {
            return;
        }
        waiting_ids.sort_by_key(|(priority, _)| *priority);
        tracing::debug!("Checking waiting queue, size: {}", waiting_ids.len());

        for (_, id) in waiting_ids {
            if let Err(e) = self
                .controller
                .spawn_program(&mut self.registry, id, 0)
                .await
            {
                tracing::error!("Failed to spawn waiting program {}: {}", id, e);
            }
        }
    }

    fn handle_generate_metrics(&self) -> String {
        let mut buffer = String::new();
        let now = chrono::Utc::now().timestamp() as u64;

        buffer.push_str("# HELP super_process_up Process status\n");
        buffer.push_str("# TYPE super_process_up gauge\n");

        // state code (0=Stopped, 2=Fatal, 3=Backoff/retry, 4=Waiting)
        for (id, config) in &self.registry.programs {
            let safe_name = config.name.replace("\"", "\\\"");
            let safe_group = config.group.as_deref().unwrap_or("").replace("\"", "\\\"");
            let labels = format!(
                "id=\"{}\",name=\"{}\",group=\"{}\"",
                id, safe_name, safe_group
            );

            let (is_up, cpu, mem, uptime, restarts, status_code) =
                if let Some(state) = self.registry.get_running(id) {
                    (
                        1,
                        state.cpu_usage,
                        state.mem_usage,
                        now.saturating_sub(state.start_time),
                        state.retry_count,
                        if state.is_healthy { 1 } else { 5 },
                    )
                } else {
                    let code = if self.registry.crashed.contains(id) {
                        2
                    } else if self.registry.restarting.contains(id) {
                        3
                    } else if self.registry.waiting.contains(id) {
                        4
                    } else {
                        0
                    };
                    (0, 0.0, 0, 0, 0, code)
                };

            buffer.push_str(&format!("super_process_up{{{}}} {}\n", labels, is_up));
            buffer.push_str(&format!(
                "super_process_cpu_percent{{{}}} {:.2}\n",
                labels, cpu
            ));
            buffer.push_str(&format!(
                "super_process_memory_bytes{{{}}} {}\n",
                labels, mem
            ));
            buffer.push_str(&format!(
                "super_process_uptime_seconds{{{}}} {}\n",
                labels, uptime
            ));
            buffer.push_str(&format!(
                "super_process_restart_count{{{}}} {}\n",
                labels, restarts
            ));
            buffer.push_str(&format!(
                "super_process_status_code{{{}}} {}\n",
                labels, status_code
            ));
        }

        buffer.push_str("\n# --- Extension Metrics ---\n");
        buffer.push_str(&self.extension.collect_metrics());
        buffer
    }

    async fn handle_health_check(&self) -> HealthResponse {
        let mut components = HashMap::new();
        components.insert("web".to_string(), "up".to_string());
        components.insert("manager".to_string(), "up".to_string());

        let persistence_status = if self.config.storage.data_file.exists() {
            match tokio::fs::metadata(&self.config.storage.data_file).await {
                Ok(m) => {
                    if m.permissions().readonly() {
                        "error: read-only"
                    } else {
                        "up"
                    }
                }
                Err(_e) => {
                    return HealthResponse {
                        status: "degraded".to_string(),
                        components,
                    };
                }
            }
            .to_string()
        } else {
            "up (no data)".to_string()
        };
        components.insert("persistence".to_string(), persistence_status);

        HealthResponse {
            status: "healthy".to_string(),
            components,
        }
    }

    async fn handle_batch_programs(
        &mut self,
        req: BatchProgramRequest,
    ) -> anyhow::Result<BatchProgramResponse> {
        // 1. Select target IDs
        let mut target_ids: Vec<Uuid> = Vec::new();

        if req.select_all {
            target_ids = self.registry.programs.keys().cloned().collect();
        } else if let Some(group) = req.group_name {
            target_ids = self
                .registry
                .programs
                .iter()
                .filter(|(_, cfg)| cfg.group.as_deref() == Some(&group))
                .map(|(id, _)| *id)
                .collect();
        } else if let Some(ids) = req.target_ids {
            // Filter to existing IDs only
            target_ids = ids
                .into_iter()
                .filter(|id| self.registry.programs.contains_key(id))
                .collect();
        }

        if target_ids.is_empty() {
            return Ok(BatchProgramResponse {
                affected: vec![],
                failed: HashMap::new(),
            });
        }

        // 2. Batch execute
        let mut affected = Vec::new();
        let mut failed = HashMap::new();

        for id in target_ids {
            let result = match &req.action {
                BatchAction::Start => {
                    // Enable autostart
                    if let Some(conf) = self.registry.get_config_mut(&id) {
                        conf.autostart = true;
                        conf.updated_at = chrono::Utc::now().timestamp() as u64;
                    }
                    self.controller
                        .spawn_program(&mut self.registry, id, 0)
                        .await
                }
                BatchAction::Stop { force } => {
                    // stop_program sets autostart = false internally
                    self.controller
                        .stop_program(&mut self.registry, id, *force)
                        .await
                }
                BatchAction::Restart => self.handle_restart_request(id).await,
                BatchAction::Remove => self.handle_remove(id).await,
                BatchAction::Signal { signal } => {
                    // Parse signal string
                    let sig = match signal.to_lowercase().as_str() {
                        "hup" => Signal::SIGHUP,
                        "int" => Signal::SIGINT,
                        "term" => Signal::SIGTERM,
                        "kill" => Signal::SIGKILL,
                        "quit" => Signal::SIGQUIT,
                        "usr1" => Signal::SIGUSR1,
                        "usr2" => Signal::SIGUSR2,
                        _ => Err(anyhow::anyhow!("Unsupported signal"))?,
                    };

                    self.apply_signal(id, sig)
                }
            };

            match result {
                Ok(_) => affected.push(id),
                Err(e) => {
                    failed.insert(id, e.to_string());
                }
            }
        }

        // Mark dirty if anything changed (triggers flush)
        if !affected.is_empty() {
            self.registry.mark_dirty();
        }

        Ok(BatchProgramResponse { affected, failed })
    }

    fn expand_request(&self, req: &CreateProgramRequest) -> Vec<ProgramConfig> {
        let count = std::cmp::max(1, req.numprocs);
        let base_name = req.name.clone().unwrap_or_else(|| req.command.clone());
        let mut result = Vec::new();

        for i in 0..count {
            let final_name = if count > 1 {
                let template = req
                    .process_name
                    .clone()
                    .unwrap_or_else(|| "{name}-{num}".to_string());
                template
                    .replace("{name}", &base_name)
                    .replace("{num}", &i.to_string())
            } else {
                base_name.clone()
            };

            let mut final_env = req.env.clone();
            if count > 1 {
                final_env.insert("SUPER_PROCESS_NUM".to_string(), i.to_string());
                final_env.insert("SUPER_PROCESS_TOTAL".to_string(), count.to_string());
            }

            #[allow(unused_mut, clippy::needless_update)]
            let mut cfg = ProgramConfig {
                name: final_name,
                command: req.command.clone(),
                args: req.args.clone(),
                env: final_env,
                env_file: req.env_file.clone(),
                cwd: req.cwd.clone(),
                user: req.user.clone(),
                autostart: req.autostart,
                retry_limit: req.retry_limit,
                autorestart: req.autorestart,
                exitcodes: if req.exitcodes.is_empty() {
                    vec![0]
                } else {
                    req.exitcodes.clone()
                },
                startsecs: if req.startsecs == 0 {
                    10
                } else {
                    req.startsecs
                },
                stopsecs: req.stopsecs,
                priority: req.priority,
                stdout_logfile: req.stdout_logfile.clone(),
                stderr_logfile: req.stderr_logfile.clone(),
                group: req.group.clone(),
                depends_on: req.depends_on.clone(),
                health_check: req.health_check.clone(),
                hooks: req.hooks.clone(),
                artifact: req.artifact.clone(),
                cron: req.cron.clone(),
                on_overlap: req.on_overlap,
                catchup: req.catchup,
                jitter_sec: req.jitter_sec,
                max_concurrent: req.max_concurrent,
                max_queued: req.max_queued,
                created_at: chrono::Utc::now().timestamp() as u64,
                updated_at: chrono::Utc::now().timestamp() as u64,
                restore_path: None,

                ..Default::default()
            };

            cfg.resource_limits = req.resource_limits.clone();

            result.push(cfg);
        }
        result
    }

    fn validate_parameters(&self, cron: Option<&str>) -> anyhow::Result<()> {
        if let Some(c) = cron
            && cron::Schedule::from_str(c).is_err()
        {
            return Err(anyhow::anyhow!("cron: invalid expression {c:?}"));
        }
        Ok(())
    }

    fn find_program_id_by_name(&self, name: &str) -> Option<Uuid> {
        self.registry
            .programs
            .iter()
            .find(|(_, cfg)| cfg.name == name)
            .map(|(id, _)| *id)
    }

    fn ensure_program_name_available(
        &self,
        name: &str,
        except_id: Option<Uuid>,
    ) -> anyhow::Result<()> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Program name cannot be empty"));
        }
        if let Some(existing_id) = self.find_program_id_by_name(name)
            && except_id != Some(existing_id)
        {
            return Err(anyhow::anyhow!(
                "Program name '{}' already exists (id: {})",
                name,
                existing_id
            ));
        }
        Ok(())
    }

    fn validate_stack_service_names(
        &self,
        services: &[CreateProgramRequest],
    ) -> anyhow::Result<()> {
        let mut counts: HashMap<String, u32> = HashMap::new();
        for service_req in services {
            for config in self.expand_request(service_req) {
                *counts.entry(config.name).or_insert(0) += 1;
            }
        }
        for (name, count) in counts {
            if count > 1 {
                return Err(anyhow::anyhow!(
                    "Duplicate program name '{}' in stack (appears {} times)",
                    name,
                    count
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod resource_limits_tests {
    use super::{apply_resource_limits_patch, validate_resource_limits_patch};
    use common::ResourceLimits;

    #[test]
    fn patch_applies_new_limits() {
        let mut existing = None;
        apply_resource_limits_patch(
            &mut existing,
            ResourceLimits {
                cpu_quota: Some(0.5),
                memory_limit: Some(512),
                memory_warn_percent: Some(80),
                memory_warn_headroom: None,
                memory_high: Some(448),
            },
        );
        let limits = existing.unwrap();
        assert_eq!(limits.cpu_quota, Some(0.5));
        assert_eq!(limits.memory_limit, Some(512));
        assert_eq!(limits.memory_warn_percent, Some(80));
        assert_eq!(limits.memory_high, Some(448));
    }

    #[test]
    fn patch_sentinels_clear_fields() {
        let mut existing = Some(ResourceLimits {
            cpu_quota: Some(0.5),
            memory_limit: Some(512),
            memory_warn_percent: Some(80),
            memory_warn_headroom: None,
            memory_high: Some(448),
        });
        apply_resource_limits_patch(
            &mut existing,
            ResourceLimits {
                cpu_quota: Some(-1.0),
                memory_limit: Some(0),
                memory_warn_percent: Some(0),
                memory_warn_headroom: None,
                memory_high: Some(0),
            },
        );
        assert!(existing.is_none());
    }

    #[test]
    fn patch_allows_removal_sentinels_in_validation() {
        validate_resource_limits_patch(&ResourceLimits {
            cpu_quota: Some(-1.0),
            memory_limit: Some(0),
            memory_warn_percent: Some(0),
            memory_warn_headroom: None,
            memory_high: None,
        })
        .unwrap();
    }
}
