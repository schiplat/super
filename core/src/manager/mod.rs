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
    UpdateProgramRequest, WsMessage, resolve_confined_log_path,
};
use glob::glob;
use nix::sys::signal::Signal;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

/// Merge `resource_limits` from an update request into stored config.
/// `-1.0` cpu / `0` memory are sentinels that clear the corresponding field.
fn apply_resource_limits_patch(existing: &mut Option<ResourceLimits>, patch: ResourceLimits) {
    if let Some(old) = existing {
        if let Some(c) = patch.cpu_quota {
            old.cpu_quota = if c <= 0.0 { None } else { Some(c) };
        }
        if let Some(m) = patch.memory_limit {
            old.memory_limit = if m == 0 { None } else { Some(m) };
        }
        if old.cpu_quota.is_none() && old.memory_limit.is_none() {
            *existing = None;
        }
    } else {
        let cpu = patch.cpu_quota.filter(|&c| c > 0.0);
        let mem = patch.memory_limit.filter(|&m| m > 0);
        if cpu.is_some() || mem.is_some() {
            *existing = Some(ResourceLimits {
                cpu_quota: cpu,
                memory_limit: mem,
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
    Ok(())
}

/// Validate an OTA artifact config at the API boundary.
///
/// URL policy (HTTPS, private/link-local blocking) is enforced separately at
/// download time in `artifact::download_to_staging`; here we only reject
/// configs that are structurally unusable, so a bad request fails fast with a
/// 400 instead of an opaque background download error.
fn validate_artifact_config(artifact: &common::ArtifactConfig) -> anyhow::Result<()> {
    if artifact.source.trim().is_empty() {
        return Err(anyhow::anyhow!("artifact.source must not be empty"));
    }
    if artifact.destination.trim().is_empty() {
        return Err(anyhow::anyhow!("artifact.destination must not be empty"));
    }
    let sum = artifact.checksum.trim();
    if sum.len() != 64 || !sum.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(anyhow::anyhow!(
            "artifact.checksum must be a 64-char hex SHA256 digest"
        ));
    }
    Ok(())
}

fn validate_program_log_paths(
    log_dir: &Path,
    stdout_logfile: Option<&str>,
    stderr_logfile: Option<&str>,
) -> anyhow::Result<()> {
    if let Some(path) = stdout_logfile.filter(|s| !s.trim().is_empty()) {
        resolve_confined_log_path(log_dir, path)?;
    }
    if let Some(path) = stderr_logfile.filter(|s| !s.trim().is_empty()) {
        resolve_confined_log_path(log_dir, path)?;
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
    if limits.cpu_quota.is_none() && limits.memory_limit.is_none() {
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

    extension: Arc<dyn Extension>,
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
            extension,
        }
    }

    fn emit_event(&self, event: common::SystemEvent) {
        crate::event_hooks::emit(&self.extension, &self.config.event_hooks, event);
    }

    pub async fn run(mut self) {
        tracing::info!(
            "Manager Loop started. Loaded {} programs.",
            self.registry.programs.len()
        );

        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        self.emit_event(common::SystemEvent::SystemStartup { hostname });

        if let Err(e) = self.process_includes().await {
            tracing::error!("Failed to process includes on startup: {}", e);
        }

        // Restore state & WAL check
        for (id, config) in &mut self.registry.programs {
            if let Some(cron) = &config.cron {
                self.scheduler.upsert(*id, cron);
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

                Command::ProcessExited { id, code } => {
                    self.handle_exited(id, code).await;
                }
                Command::CheckTimeoutKill { id, target_pid } => {
                    // 1. Check whether forced cleanup is needed
                    let mut force_cleanup = false;

                    // 2. Only if registry still considers process running
                    if let Some(state) = self.registry.get_running(&id) {
                        // Kill only if PID matches (avoid killing a new instance after restart)
                        if state.pid == target_pid {
                            tracing::warn!("Stop timeout reached for {}. Sending SIGKILL.", id);

                            // Send SIGKILL
                            let kill_result = nix::sys::signal::kill(
                                nix::unistd::Pid::from_raw(-(state.pid as i32)),
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
                                        state.pid
                                    );
                                    force_cleanup = true;
                                }
                                Err(e) => {
                                    tracing::error!("Failed to SIGKILL {}: {}", id, e);
                                }
                            }
                        }
                    }

                    // 3. Force cleanup (avoids borrow conflict above)
                    if force_cleanup {
                        self.handle_exited(id, None).await;
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
                Command::ApplyStack { request, reply } => {
                    let res = self.handle_apply_stack(request).await;
                    let _ = reply.send(res);
                }
                Command::DumpPrograms { reply } => {
                    let configs: Vec<ProgramConfig> =
                        self.registry.programs.values().cloned().collect();
                    let _ = reply.send(configs);
                }
                Command::InternalArtifactReady { id, path } => {
                    self.handle_artifact_ready(id, path).await;
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
                    let triggered_ids = self.scheduler.tick();
                    for id in triggered_ids {
                        let name = match self.registry.get_config(&id) {
                            Some(cfg) => cfg.name.clone(),
                            None => continue,
                        };
                        if self.registry.running.contains_key(&id) {
                            tracing::warn!(
                                "Cron job {} is still running, skipping this tick.",
                                name
                            );
                            continue;
                        }
                        tracing::info!("Cron job triggered: {}", name);
                        if let Err(e) = self
                            .controller
                            .spawn_program(&mut self.registry, id, 0)
                            .await
                        {
                            tracing::error!("Failed to spawn cron job {}: {}", name, e);
                        }
                    }
                }
                Command::PersistTick => {
                    if let Err(e) = self.flush_to_disk().await {
                        tracing::error!("Failed to auto-save state: {}", e);
                    }
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
        if let Some(state) = self.registry.get_running(&id) {
            tracing::info!(
                "Sending signal {:?} to program {} (PGID: {})",
                signal,
                id,
                state.pid
            );
            // Negative PID targets the process group
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(-(state.pid as i32)), signal)
                .map_err(|e| e.into())
        } else {
            Err(anyhow::anyhow!("Program is not running"))
        }
    }

    //
    // Handlers
    //

    async fn handle_update(&mut self, id: Uuid, req: UpdateProgramRequest) -> anyhow::Result<()> {
        if req.cron.is_some() {
            self.validate_parameters(req.cron.as_deref())?;
        }

        if let Some(limits) = &req.resource_limits {
            validate_resource_limits_patch(limits)?;
            let effective = ResourceLimits {
                cpu_quota: limits.cpu_quota.filter(|&c| c > 0.0),
                memory_limit: limits.memory_limit.filter(|&m| m > 0),
            };
            if effective.cpu_quota.is_some() || effective.memory_limit.is_some() {
                warn_if_resource_limits_unenforced(
                    self.extension.as_ref(),
                    &Some(effective),
                    "update program",
                );
            }
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

            // [Trigger Logic] Checksum change detection to trigger OTA
            if let Some(v) = &req.artifact {
                validate_artifact_config(v)?;
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

            if let Some(v) = req.cron {
                config.cron = Some(v.clone());
                self.scheduler.upsert(id, &v);
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
    }

    // Process exit handler (OTA commit/rollback & borrow fixes)
    async fn handle_exited(&mut self, id: Uuid, code: Option<i32>) {
        // 1. Clear runtime state
        let state = match self.registry.running.remove(&id) {
            Some(s) => s,
            None => return,
        };

        let exited_pid = state.pid;
        let exited_uptime = chrono::Utc::now().timestamp() as u64 - state.start_time;

        // Stop health check task and resource monitor
        if let Some(task) = state.health_task {
            task.abort();
        }
        self.monitor.unwatch(&id);

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
                        self.emit_event(common::SystemEvent::ProcessFatal {
                            program_id: id,
                            program_name: program_name.clone(),
                            pid: Some(exited_pid),
                            uptime_secs: exited_uptime,
                            exit_code: code,
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
        if config.cron.is_some() {
            if let Some(0) = code {
                tracing::info!("Cron job '{}' finished successfully.", program_name);
                let _ = self.log_tx.send(WsMessage::StatusChange {
                    id,
                    status: ProcessStatus::Stopped,
                    name: program_name.clone(),
                });
            } else {
                tracing::error!("Cron job '{}' failed with code {:?}.", program_name, code);
                let _ = self.log_tx.send(WsMessage::StatusChange {
                    id,
                    status: ProcessStatus::Fatal,
                    name: program_name.clone(),
                });
                let event = common::SystemEvent::ProcessFatal {
                    program_id: id,
                    program_name: program_name.clone(),
                    pid: Some(exited_pid),
                    uptime_secs: exited_uptime,
                    exit_code: code,
                    msg: "Cron job execution failed".to_string(),
                    log_tail: None,
                };
                self.emit_event(event);
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

            let event = common::SystemEvent::ProcessBackoff {
                program_id: id,
                program_name,
                pid: Some(exited_pid),
                uptime_secs: exited_uptime,
                exit_code: code,
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
        if let Some(state) = self.registry.running.get_mut(&id) {
            // Ignore health updates while stopping
            // Prevents Stop -> Stopping -> (health race) -> Healthy
            if state.stopping {
                return;
            }

            if !is_healthy {
                if let Some(detail) = failure_detail {
                    let changed = state.health_error.as_deref() != Some(detail.as_str());
                    state.health_error = Some(detail.clone());
                    if changed && let Some(cfg) = self.registry.programs.get(&id) {
                        let log_dir = &self.config.storage.log_dir;
                        crate::logger::emit_superd_line(
                            id,
                            &format!("health_check failed: {detail}"),
                            log_dir,
                            cfg.stdout_logfile.as_deref(),
                            cfg.stderr_logfile.as_deref(),
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

                let name = self
                    .registry
                    .programs
                    .get(&id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                let display_status = if is_healthy {
                    ProcessStatus::Healthy
                } else {
                    ProcessStatus::Running
                };
                tracing::info!("Program {} health changed: {}", name, is_healthy);

                let _ = self.log_tx.send(WsMessage::StatusChange {
                    id,
                    status: display_status,
                    name: name.clone(),
                });

                if is_healthy && state.alert_pending_recovery {
                    state.alert_pending_recovery = false;
                    let recovered_pid = state.pid;
                    let uptime = chrono::Utc::now().timestamp() as u64 - state.start_time;
                    self.emit_event(common::SystemEvent::ProcessRecovered {
                        program_id: id,
                        program_name: name.clone(),
                        pid: Some(recovered_pid),
                        uptime_sec: uptime,
                    });
                    tracing::info!("Program {} has RECOVERED from crash!", name);
                }

                // Commit Upgrade Transaction
                // If program is healthy and has a pending restore path, commit the upgrade.
                if is_healthy {
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
        }
    }

    async fn handle_apply_stack(&mut self, req: StackApplyRequest) -> anyhow::Result<Vec<String>> {
        let mut logs = Vec::new();
        let mut touched_programs = HashSet::new();

        self.validate_stack_service_names(&req.services)?;

        for service_req in &req.services {
            for config in self.expand_request(service_req) {
                self.validate_parameters(config.cron.as_deref())?;
                validate_program_log_paths(
                    &self.config.storage.log_dir,
                    config.stdout_logfile.as_deref(),
                    config.stderr_logfile.as_deref(),
                )?;
            }
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
                    let mut should_start = config.autostart;

                    if let Some(cron_expr) = &config.cron {
                        should_start = false;
                        self.scheduler.upsert(id, cron_expr);
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
        Ok(logs)
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
            if self.registry.running.contains_key(id) {
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
            if self.registry.running.is_empty() {
                tracing::info!("All processes exited cleanly.");
                break;
            }

            if tokio::time::Instant::now() > deadline {
                tracing::warn!(
                    "Shutdown timeout reached. {} processes still running.",
                    self.registry.running.len()
                );
                for state in self.registry.running.values() {
                    tracing::warn!("Force killing PID {}", state.pid);
                    let _ = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(state.pid as i32),
                        Signal::SIGKILL,
                    );
                }
                break;
            }

            match self.rx.try_recv() {
                Ok(cmd) => {
                    if let Command::ProcessExited { id, code } = cmd {
                        self.handle_exited(id, code).await;
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
        let configs = self.expand_request(&req);
        let mut validation_error = None;
        for cfg in &configs {
            if let Err(e) = self.validate_parameters(cfg.cron.as_deref()) {
                validation_error = Some(e);
                break;
            }

            if let Err(e) = validate_program_log_paths(
                &self.config.storage.log_dir,
                cfg.stdout_logfile.as_deref(),
                cfg.stderr_logfile.as_deref(),
            ) {
                validation_error = Some(e);
                break;
            }

            if let Some(l) = &cfg.resource_limits {
                if let Some(c) = l.cpu_quota
                    && c <= 0.0
                {
                    validation_error = Some(anyhow::anyhow!("CPU quota must be > 0"));
                    break;
                }
                if let Some(m) = l.memory_limit
                    && m == 0
                {
                    validation_error = Some(anyhow::anyhow!("Memory limit must be > 0"));
                    break;
                }
                warn_if_resource_limits_unenforced(
                    self.extension.as_ref(),
                    &cfg.resource_limits,
                    "create program",
                );
            }

            if let Some(a) = &cfg.artifact
                && let Err(e) = validate_artifact_config(a)
            {
                validation_error = Some(e);
                break;
            }
        }

        if let Some(e) = validation_error {
            tracing::warn!("CreateProgram validation failed: {}", e);
            let _ = reply.send(Err(e));
        } else {
            for cfg in &configs {
                if let Err(e) = self.ensure_program_name_available(&cfg.name, None) {
                    tracing::warn!("CreateProgram name conflict: {}", e);
                    let _ = reply.send(Err(e));
                    return;
                }
            }

            let mut created_ids = Vec::new();
            for config in configs {
                let id = Uuid::new_v4();
                let should_start = config.autostart;
                let name = config.name.clone();

                if let Some(cron_expr) = &config.cron {
                    self.scheduler.upsert(id, cron_expr);
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
    }

    async fn handle_reload(&mut self) -> anyhow::Result<()> {
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
        if let Err(e) = self.process_includes().await {
            tracing::error!("Failed to process includes during reload: {}", e);
        }
        tracing::info!("Configuration reloaded successfully.");
        Ok(())
    }

    async fn flush_to_disk(&mut self) -> anyhow::Result<()> {
        if self.registry.dirty {
            store::save(&self.config.storage.data_file, &self.registry.programs).await?;
            self.registry.dirty = false;
            tracing::debug!("State persisted to disk (Debounced).");
        }
        Ok(())
    }

    fn handle_list(&self) -> Vec<ProgramSummary> {
        let mut list = Vec::new();
        for (id, config) in &self.registry.programs {
            let (status, pid, uptime, cpu, mem) = if let Some(state) = self.registry.get_running(id)
            {
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

            list.push(ProgramSummary {
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
                    .running
                    .get(id)
                    .and_then(|s| s.health_error.clone()),
                cpu_usage: cpu,
                mem_usage: mem,
                depends_on: config.depends_on.clone(),
                resource_limits: config.resource_limits.clone(),
            });
        }
        list
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
                .running
                .get(&id)
                .and_then(|s| s.health_error.clone()),
        })
    }

    async fn handle_restart_request(&mut self, id: Uuid) -> anyhow::Result<()> {
        if let Some(state) = self.registry.get_running_mut(&id) {
            tracing::info!("Restart requested for {}. Stopping current process...", id);
            state.restart_requested = true;
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(state.pid as i32),
                Signal::SIGTERM,
            );

            let tx = self.tx_self.clone();
            let target_pid = state.pid;
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
        if self.registry.running.contains_key(&id) {
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
        self.scheduler.remove(&id);
        self.registry.mark_dirty();

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

    async fn process_includes(&mut self) -> anyhow::Result<()> {
        let patterns = self.config.include.files.clone();
        if patterns.is_empty() {
            return Ok(());
        }
        let root = crate::resolve_root();

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
                        && let Ok(stack) = serde_json::from_str::<StackApplyRequest>(&content)
                    {
                        let _ = self.handle_apply_stack(stack).await;
                    }
                }
            }
        }
        Ok(())
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
            return Err(anyhow::anyhow!("Invalid cron: {}", c));
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
                cpu_quota: Some(50.0),
                memory_limit: Some(1024),
            },
        );
        let limits = existing.unwrap();
        assert_eq!(limits.cpu_quota, Some(50.0));
        assert_eq!(limits.memory_limit, Some(1024));
    }

    #[test]
    fn patch_sentinels_clear_fields() {
        let mut existing = Some(ResourceLimits {
            cpu_quota: Some(50.0),
            memory_limit: Some(1024),
        });
        apply_resource_limits_patch(
            &mut existing,
            ResourceLimits {
                cpu_quota: Some(-1.0),
                memory_limit: Some(0),
            },
        );
        assert!(existing.is_none());
    }

    #[test]
    fn patch_allows_removal_sentinels_in_validation() {
        validate_resource_limits_patch(&ResourceLimits {
            cpu_quota: Some(-1.0),
            memory_limit: Some(0),
        })
        .unwrap();
    }
}

#[cfg(test)]
mod artifact_config_tests {
    use super::validate_artifact_config;
    use common::ArtifactConfig;

    fn valid() -> ArtifactConfig {
        ArtifactConfig {
            source: "https://example.com/app.tar.gz".to_string(),
            checksum: "a".repeat(64),
            extract: false,
            destination: "/opt/app/bin/app".to_string(),
            restart_policy: "immediate".to_string(),
        }
    }

    #[test]
    fn accepts_valid_config() {
        validate_artifact_config(&valid()).unwrap();
    }

    #[test]
    fn rejects_empty_source() {
        let mut a = valid();
        a.source = "   ".to_string();
        assert!(validate_artifact_config(&a).is_err());
    }

    #[test]
    fn rejects_empty_destination() {
        let mut a = valid();
        a.destination = String::new();
        assert!(validate_artifact_config(&a).is_err());
    }

    #[test]
    fn rejects_bad_checksum() {
        let mut a = valid();
        a.checksum = "not-hex".to_string();
        assert!(validate_artifact_config(&a).is_err());

        let mut b = valid();
        b.checksum = "abc".to_string(); // too short
        assert!(validate_artifact_config(&b).is_err());
    }
}
