use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::config::ServerConfig;
use crate::extension::Extension;
use crate::health;
use crate::logger;
use crate::manager::command::Command;
use crate::manager::registry::{ProcessRegistry, RuntimeState};
use crate::manager::tracker::FlappingTracker;
use crate::monitor::ResourceMonitor;
use crate::process;
use common::{ProcessStatus, ProgramConfig, SystemEvent, WsMessage};

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

/// Lifecycle controller: spawn, stop, and signal logic.
pub struct LifecycleController {
    // Core dependencies
    pub config: ServerConfig,
    pub tx_self: mpsc::Sender<Command>,
    pub log_tx: broadcast::Sender<WsMessage>,
    pub extension: Arc<dyn Extension>,

    // External component references
    pub monitor: Arc<ResourceMonitor>,

    // Internal flapping tracker
    pub tracker: FlappingTracker,
}

impl LifecycleController {
    pub fn new(
        config: ServerConfig,
        tx_self: mpsc::Sender<Command>,
        log_tx: broadcast::Sender<WsMessage>,
        extension: Arc<dyn Extension>,
        monitor: Arc<ResourceMonitor>,
    ) -> Self {
        Self {
            config,
            tx_self,
            log_tx,
            extension,
            monitor,
            tracker: FlappingTracker::new(),
        }
    }

    // Core spawn logic.
    // Registry is borrowed for state read/write with clear ownership.
    pub async fn spawn_program(
        &mut self,
        registry: &mut ProcessRegistry,
        id: Uuid,
        retry_count: u32,
    ) -> anyhow::Result<()> {
        // 1. Basic checks
        let config = registry
            .programs
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Program not found"))?;

        // Scheduled tasks with `max_concurrent > 1` may run several instances
        // at once; everything else allows a single running instance.
        let max_concurrent = config.max_concurrent_eff() as usize;
        if registry.running_count(&id) >= max_concurrent {
            let msg = if config.cron.is_some() {
                format!(
                    "Program is already running at max_concurrent={}",
                    config.max_concurrent_eff()
                )
            } else {
                "Program is already running".to_string()
            };
            return Err(anyhow::anyhow!(msg));
        }

        let program_name = config.name.clone();

        // 2. Flapping detection — only for long-running services. Cron jobs are
        // one-shot scheduled tasks: they exit and re-trigger on the next tick by
        // design, so restart-loop detection does not apply (and would otherwise
        // permanently disable short-interval schedules via `autostart = false`).
        if config.cron.is_none() {
            // Record start time
            self.tracker
                .record_start(id, self.config.server.flapping_threshold);

            // Check for rapid restarts within the window
            if self.tracker.is_flapping(
                id,
                self.config.server.flapping_window,
                self.config.server.flapping_threshold,
            ) {
                tracing::error!(
                    "FLAPPING DETECTED for {}! Restarted too frequently.",
                    program_name
                );

                // Mark state as Fatal
                registry.restarting.remove(&id);
                registry.waiting.remove(&id);
                registry.crashed.insert(id);

                if let Some(cfg) = registry.programs.get_mut(&id) {
                    cfg.autostart = false;
                    cfg.updated_at = chrono::Utc::now().timestamp() as u64;
                }

                // Record flapping error so the UI shows a reason, not just Fatal
                let err_msg = format!(
                    "FLAPPING DETECTED: Restarted too frequently in {}s.",
                    self.config.server.flapping_window
                );
                registry.startup_errors.insert(id, err_msg.clone());

                registry.mark_dirty();

                let _ = self.log_tx.send(WsMessage::StatusChange {
                    id,
                    status: ProcessStatus::Fatal,
                    name: program_name.clone(),
                });

                // Extension Event
                let event = SystemEvent::ProcessFatal {
                    program_id: id,
                    program_name: program_name.clone(),
                    pid: None,
                    uptime_secs: 0,
                    exit_code: None,
                    signal: None,
                    msg: format!(
                        "FLAPPING DETECTED: Restarted too frequently in {}s.",
                        self.config.server.flapping_window
                    ),
                    log_tail: None,
                };
                crate::event_hooks::emit(&self.extension, &self.config.event_hooks, event);

                return Err(anyhow::anyhow!("Program flapping detected"));
            }
        }

        // 3. Prepare for start
        registry.restarting.remove(&id);
        registry.waiting.remove(&id);
        registry.crashed.remove(&id);
        registry.startup_errors.remove(&id);

        // 4. Dependency check
        if !config.depends_on.is_empty() {
            let mut all_ready = true;
            let mut missing_deps = Vec::new();
            // Dependencies that exist, are idle, and opted into auto-start.
            let mut auto_start_deps = Vec::new();

            for dep_name in &config.depends_on {
                // Look up dependency service ID in registry
                let dep_id = registry
                    .programs
                    .iter()
                    .find(|(_, cfg)| &cfg.name == dep_name)
                    .map(|(id, _)| *id);

                match dep_id {
                    Some(did) => {
                        if let Some(state) = registry.get_running(&did) {
                            if !state.is_healthy {
                                all_ready = false;
                                missing_deps.push(format!("{} (Not Healthy)", dep_name));
                            }
                        } else {
                            all_ready = false;
                            missing_deps.push(format!("{} (Not Running)", dep_name));
                            // A missing dependency is auto-started through the
                            // normal start path, unless it is already busy in its
                            // own lifecycle (waiting/restarting/crashed) — that
                            // also breaks dependency cycles (A -> B, B -> A)
                            // from re-triggering this program.
                            let idle = !registry.waiting.contains(&did)
                                && !registry.restarting.contains(&did)
                                && !registry.crashed.contains(&did);
                            if idle {
                                auto_start_deps.push(did);
                            }
                        }
                    }
                    None => {
                        all_ready = false;
                        missing_deps.push(format!("{} (Missing)", dep_name));
                    }
                }
            }

            if !all_ready {
                tracing::info!(
                    "Program {} is WAITING. Dependencies not ready: {:?}",
                    config.name,
                    missing_deps
                );
                // Mark ourselves waiting *before* requesting dependency starts so
                // a dependency cycle cannot immediately re-trigger our own start.
                registry.waiting.insert(id);
                let _ = self.log_tx.send(WsMessage::StatusChange {
                    id,
                    status: ProcessStatus::Waiting,
                    name: config.name.clone(),
                });

                // Kick off eligible dependencies. They go through the normal start
                // path; once healthy, the waiting queue re-tries this program.
                for did in auto_start_deps {
                    if registry.is_running(&did) {
                        continue;
                    }
                    tracing::info!("Program {} auto-starting dependency {}", config.name, did);
                    let (reply, _rx) = tokio::sync::oneshot::channel();
                    let _ = self
                        .tx_self
                        .send(Command::StartProgram { id: did, reply })
                        .await;
                }
                return Ok(());
            }
        }

        // 5. [Hook] Before Start (Extension)
        let extra_envs_from_ext = match self.extension.before_start(id, &config) {
            Ok(envs) => envs,
            Err(e) => {
                let err_msg = format!("Extension blocked start: {}", e);
                tracing::error!("{}", err_msg);
                registry.startup_errors.insert(id, err_msg.clone());
                registry.crashed.insert(id);

                let _ = self.log_tx.send(WsMessage::StatusChange {
                    id,
                    status: ProcessStatus::Fatal,
                    name: config.name.clone(),
                });

                let event = SystemEvent::ProcessFatal {
                    program_id: id,
                    program_name: config.name.clone(),
                    pid: None,
                    uptime_secs: 0,
                    exit_code: None,
                    signal: None,
                    msg: err_msg,
                    log_tail: None,
                };
                crate::event_hooks::emit(&self.extension, &self.config.event_hooks, event);

                return Err(anyhow::anyhow!("Extension blocked start"));
            }
        };

        // 6. [Hook] Pre-Start Script (Shell)
        if let Some(cmd) = &config.hooks.pre_start {
            let _ = self.log_tx.send(WsMessage::StatusChange {
                id,
                status: ProcessStatus::Starting,
                name: config.name.clone(),
            });
            let mut envs = self.build_context(id, &config, None, None, None);
            if let Some(ext) = &extra_envs_from_ext {
                envs.extend(ext.clone());
            }

            use crate::hooks;
            match hooks::run_hook(cmd, &envs).await {
                Ok(true) => {
                    tracing::info!("Pre-start hook passed for {}", config.name);
                }
                _ => {
                    // false or Error
                    tracing::error!("Pre-start hook failed for {}. Aborting start.", config.name);
                    registry.crashed.insert(id);
                    let _ = self.log_tx.send(WsMessage::StatusChange {
                        id,
                        status: ProcessStatus::Fatal,
                        name: config.name.clone(),
                    });
                    registry
                        .startup_errors
                        .insert(id, "Pre-start hook failed".to_string());

                    let event = SystemEvent::ProcessFatal {
                        program_id: id,
                        program_name: config.name.clone(),
                        pid: None,
                        uptime_secs: 0,
                        exit_code: None,
                        signal: None,
                        msg: "Pre-start hook failed".to_string(),
                        log_tail: None,
                    };
                    crate::event_hooks::emit(&self.extension, &self.config.event_hooks, event);
                    return Err(anyhow::anyhow!("Pre-start hook failed"));
                }
            }
        }

        tracing::info!("Starting program: {} (Retry: {})", config.name, retry_count);
        let _ = self.log_tx.send(WsMessage::StatusChange {
            id,
            status: ProcessStatus::Running,
            name: config.name.clone(),
        });

        // 7. Spawn Process
        let mut proc_envs = self.build_context(id, &config, None, None, None);
        if let Some(ext) = extra_envs_from_ext {
            proc_envs.extend(ext);
        }

        let mut child = match process::spawn_process(&config, &proc_envs) {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("Spawn failed: {}", e);
                tracing::error!("Program {} failed to spawn: {}", config.name, err_msg);
                registry.startup_errors.insert(id, err_msg.clone());
                registry.crashed.insert(id);

                let _ = self.log_tx.send(WsMessage::StatusChange {
                    id,
                    status: ProcessStatus::Fatal,
                    name: config.name.clone(),
                });

                let event = SystemEvent::ProcessFatal {
                    program_id: id,
                    program_name: config.name.clone(),
                    pid: None,
                    uptime_secs: 0,
                    exit_code: None,
                    signal: None,
                    msg: err_msg,
                    log_tail: None,
                };
                crate::event_hooks::emit(&self.extension, &self.config.event_hooks, event);

                return Err(anyhow::anyhow!("Spawn process failed"));
            }
        };

        let pid = child
            .id()
            .ok_or_else(|| anyhow::anyhow!("Failed to get PID"))?;

        // 8. [Hook] After Start (Extension - Strict Policy)
        // Key logic: if cgroup/resource limits fail to apply, kill the process immediately
        let hook_result = {
            let ext = self.extension.clone();
            let cfg_clone = config.clone();
            tokio::task::spawn_blocking(move || ext.after_start(id, pid, &cfg_clone)).await
        };

        match hook_result {
            Ok(Err(e)) => {
                let err_msg = format!("Extension failed (Strict Policy): {}", e);
                tracing::error!("{}", err_msg);

                // Fail Secure: Kill Immediately
                let _ = child.start_kill();
                let _ = child.wait().await;

                registry.startup_errors.insert(id, err_msg.clone());
                registry.crashed.insert(id);

                let _ = self.log_tx.send(WsMessage::StatusChange {
                    id,
                    status: ProcessStatus::Fatal,
                    name: config.name.clone(),
                });

                let event = SystemEvent::ProcessFatal {
                    program_id: id,
                    program_name: config.name.clone(),
                    pid: None,
                    uptime_secs: 0,
                    exit_code: None,
                    signal: None,
                    msg: err_msg,
                    log_tail: None,
                };
                crate::event_hooks::emit(&self.extension, &self.config.event_hooks, event);

                return Err(anyhow::anyhow!("Extension blocked startup (Strict Policy)"));
            }
            Err(e) => {
                tracing::error!("Extension thread panic: {}", e);
                let _ = child.start_kill();
                let _ = child.wait().await;
                registry.crashed.insert(id);
                return Err(anyhow::anyhow!("Extension panic during startup"));
            }
            Ok(Ok(_)) => {
                tracing::debug!(
                    "Extension after_start executed for {} (PID: {})",
                    config.name,
                    pid
                );
            }
        }

        // 9. Monitor Watch
        self.monitor.watch(id, pid);

        // 10. [Event] Started
        crate::event_hooks::emit(
            &self.extension,
            &self.config.event_hooks,
            SystemEvent::ProcessStarted {
                program_id: id,
                program_name: config.name.clone(),
                pid,
            },
        );

        // 11. [Hook] Post-Start (Script)
        if let Some(cmd) = &config.hooks.post_start {
            let envs = self.build_context(id, &config, Some(pid), None, None);
            let cmd_clone = cmd.clone();
            tokio::spawn(async move {
                use crate::hooks;
                let _ = hooks::run_hook(&cmd_clone, &envs).await;
            });
        }

        // 12. Health Check
        let mut health_task = None;
        let mut is_healthy = false;

        if let Some(check_config) = &config.health_check {
            if check_config.is_enabled() {
                let tx = self.tx_self.clone();
                let check = check_config.clone();
                // Read tuning before entering the async block (they are cheap).
                let interval = check.interval_secs().max(1);
                let start_period = check.start_period_secs();
                let max_failures = check.max_failures();

                // Start background health check task
                health_task = Some(tokio::spawn(async move {
                    // Grace period before the first probe (start_period_secs)
                    tokio::time::sleep(tokio::time::Duration::from_secs(start_period)).await;
                    let mut consecutive_failures = 0u32;
                    loop {
                        let outcome = health::perform_check(&check).await;
                        if outcome.healthy {
                            consecutive_failures = 0;
                        } else {
                            consecutive_failures += 1;
                        }
                        if tx
                            .send(Command::InternalHealthUpdate {
                                id,
                                is_healthy: outcome.healthy,
                                failure_detail: outcome.detail.clone(),
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                        // Auto-restart after `max_failures` consecutive failures
                        // (0 disables). A fresh health task is spawned with the
                        // new process, so this task ends here.
                        if max_failures > 0 && consecutive_failures >= max_failures {
                            let detail = outcome
                                .detail
                                .unwrap_or_else(|| "health check failed".to_string());
                            if tx
                                .send(Command::HealthRestart {
                                    id,
                                    failure_detail: detail,
                                })
                                .await
                                .is_err()
                            {
                                break;
                            }
                            break;
                        }
                        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
                    }
                }));
            } else {
                // Disabled health check; treat as healthy.
                is_healthy = true;
                let tx = self.tx_self.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    let _ = tx
                        .send(Command::InternalHealthUpdate {
                            id,
                            is_healthy: true,
                            failure_detail: None,
                        })
                        .await;
                });
            }
        } else {
            // No health check configured; treat as healthy by default
            is_healthy = true;

            // Send one health update explicitly to trigger OTA commit logic
            let tx = self.tx_self.clone();
            tokio::spawn(async move {
                // Brief delay so registry state is inserted first
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                let _ = tx
                    .send(Command::InternalHealthUpdate {
                        id,
                        is_healthy: true,
                        failure_detail: None,
                    })
                    .await;
            });
        }

        // 13. Record runtime state
        registry.insert_running(
            id,
            RuntimeState {
                pid,
                start_time: chrono::Utc::now().timestamp() as u64,
                retry_count,
                stopping: false,
                restart_requested: false,
                is_healthy,
                health_error: None,
                health_task,
                alert_pending_recovery: retry_count > 0,
                cpu_usage: 0.0,
                mem_usage: 0,
            },
        );

        // 14. Start log capture
        let base_log_config = logger::LogConfig {
            log_dir: self.config.storage.log_dir.clone(),
            max_size: self.config.child_logging.max_size_mb * 1024 * 1024,
            backups: self.config.child_logging.max_backups,
            driver: self.config.child_logging.driver.clone(),
            program_name: config.name.clone(),
            max_line_bytes: (self.config.child_logging.max_line_size_kb * 1024) as usize,
            timestamp: self.config.child_logging.timestamp,
            custom_path: None,
        };

        let mut stdout_log_config = base_log_config.clone();
        stdout_log_config.custom_path = match config.stdout_logfile.as_deref() {
            None => None,
            Some(path) => Some(common::resolve_confined_log_path(
                &self.config.storage.log_dir,
                path,
            )?),
        };
        let mut stderr_log_config = base_log_config;
        stderr_log_config.custom_path = match config.stderr_logfile.as_deref() {
            None => None,
            Some(path) => Some(common::resolve_confined_log_path(
                &self.config.storage.log_dir,
                path,
            )?),
        };

        if let Some(stdout) = child.stdout.take() {
            logger::capture_stdout(id, stdout, stdout_log_config, self.log_tx.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            logger::capture_stderr(id, stderr, stderr_log_config, self.log_tx.clone());
        }

        // 15. [Exit Detection] Use child.wait() to detect managed process exit
        let tx = self.tx_self.clone();

        tokio::spawn(async move {
            let wait_result = child.wait().await;
            let (code, signal) = match wait_result {
                Ok(status) => {
                    // `status.code()` is None when the process was terminated by a
                    // signal (e.g. SIGKILL from cgroup OOM or manual kill); expose
                    // the raw signal so the manager can distinguish "killed" exits.
                    let code = status.code();
                    #[cfg(unix)]
                    let signal = status.signal();
                    #[cfg(not(unix))]
                    let signal = None;
                    (code, signal)
                }
                Err(e) => {
                    tracing::error!("Failed to wait on child process: {}", e);
                    (None, None)
                }
            };
            if let Err(e) = tx
                .send(Command::ProcessExited {
                    id,
                    pid,
                    code,
                    signal,
                })
                .await
            {
                tracing::error!("Failed to send ProcessExited for {}: {}", id, e);
            }
        });

        // 16. Trigger dependency scheduling
        if is_healthy {
            let _ = self.log_tx.send(WsMessage::StatusChange {
                id,
                status: ProcessStatus::Healthy,
                name: config.name.clone(),
            });
            let tx = self.tx_self.clone();
            tokio::spawn(async move {
                let _ = tx.send(Command::CheckWaitingQueue).await;
            });
        }

        Ok(())
    }

    // Helper: build environment variable context
    pub fn build_context(
        &self,
        id: Uuid,
        config: &ProgramConfig,
        pid: Option<u32>,
        exit_code: Option<i32>,
        uptime_secs: Option<u64>,
    ) -> HashMap<String, String> {
        let mut envs = HashMap::new();

        // 1. Load env_file dynamically (if present)
        if let Some(env_file) = &config.env_file {
            if let Ok(iter) = dotenvy::from_path_iter(env_file) {
                // flatten skips Err entries and yields Ok (K, V) pairs
                for (k, v) in iter.flatten() {
                    envs.insert(k, v);
                }
            } else {
                tracing::warn!("Failed to read or parse env_file: {}", env_file);
            }
        }

        // 2. Merge explicit env (higher priority; overrides env_file)
        for (k, v) in &config.env {
            envs.insert(k.clone(), v.clone());
        }

        // 3. Inject built-in system variables
        let hostname = common::resolve_hostname();
        envs.insert("SUPER_HOSTNAME".to_string(), hostname);
        envs.insert("SUPER_ID".to_string(), id.to_string());
        envs.insert("SUPER_NAME".to_string(), config.name.clone());
        if let Some(g) = &config.group {
            envs.insert("SUPER_GROUP".to_string(), g.clone());
        }
        if let Some(p) = pid {
            envs.insert("SUPER_PID".to_string(), p.to_string());
        }
        if let Some(code) = exit_code {
            envs.insert("SUPER_EXIT_CODE".to_string(), code.to_string());
        }
        if let Some(u) = uptime_secs {
            envs.insert("SUPER_UPTIME_SECS".to_string(), u.to_string());
        }

        envs
    }

    // Stop logic
    pub fn stop_timeout(&self, registry: &ProcessRegistry, id: Uuid) -> u64 {
        registry
            .get_config(&id)
            .and_then(|c| c.stopsecs)
            .map(u64::from)
            .unwrap_or(self.config.server.shutdown_timeout)
    }

    pub async fn stop_program(
        &mut self,
        registry: &mut ProcessRegistry,
        id: Uuid,
        force: bool,
    ) -> anyhow::Result<()> {
        // On manual stop, clear flapping history for a clean retry
        self.tracker.reset(&id);

        if let Some(conf) = registry.programs.get_mut(&id) {
            conf.autostart = false;
        }
        registry.mark_dirty();

        // Collect every running instance (a scheduled task may have several
        // overlapping instances when `max_concurrent > 1`).
        let running_states: Vec<u32> = registry
            .get_running_all(&id)
            .iter()
            .map(|s| s.pid)
            .collect();

        // Mark all as stopping so exit handling does not auto-restart them.
        if let Some(states) = registry.get_running_all_mut(&id) {
            for state in states.iter_mut() {
                state.stopping = true;
            }
        }

        // Case 1: Running
        if let Some(primary_pid) = running_states.first().copied() {
            // [Hook] Pre-Stop
            if let Some(config) = registry.programs.get(&id)
                && let Some(cmd) = &config.hooks.pre_stop
            {
                let envs = self.build_context(id, config, Some(primary_pid), None, None);
                use crate::hooks;
                tracing::info!("Executing pre-stop hook for {}", config.name);
                let _ = hooks::run_hook(cmd, &envs).await;
            }

            for pid in &running_states {
                // Send stop signal
                if force {
                    tracing::warn!("Force stopping program group: {} (PGID: {})", id, pid);
                    if let Err(e) = signal::kill(Pid::from_raw(-(*pid as i32)), Signal::SIGKILL) {
                        // Process not found; already exited — trigger cleanup
                        if e == nix::errno::Errno::ESRCH {
                            tracing::warn!(
                                "Process {} (PGID {}) not found during force stop. Assuming exited.",
                                id,
                                pid
                            );
                            let _ = self
                                .tx_self
                                .send(Command::ProcessExited {
                                    id,
                                    pid: *pid,
                                    code: None,
                                    signal: None,
                                })
                                .await;
                            continue;
                        }
                    }
                } else {
                    tracing::info!("Stopping program group: {} (PGID: {})", id, pid);
                    if let Err(e) = signal::kill(Pid::from_raw(-(*pid as i32)), Signal::SIGTERM) {
                        // ESRCH on signal: treat as exited immediately, do not wait for timeout
                        if e == nix::errno::Errno::ESRCH {
                            tracing::warn!(
                                "Process {} (PGID {}) not found during stop. Assuming exited.",
                                id,
                                pid
                            );
                            // Send ProcessExited to Manager for cleanup
                            let _ = self
                                .tx_self
                                .send(Command::ProcessExited {
                                    id,
                                    pid: *pid,
                                    code: None,
                                    signal: None,
                                })
                                .await;
                            continue;
                        }
                        return Err(e.into());
                    }
                }
            }

            // Notify UI immediately: Stopping (not stuck on Healthy)
            if let Some(config) = registry.programs.get(&id) {
                let _ = self.log_tx.send(WsMessage::StatusChange {
                    id,
                    status: ProcessStatus::Stopping,
                    name: config.name.clone(),
                });
            }

            // Start timeout watchdog
            let tx = self.tx_self.clone();
            let timeout_sec = if force {
                1
            } else {
                self.stop_timeout(registry, id)
            };

            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(timeout_sec)).await;
                let _ = tx
                    .send(Command::CheckTimeoutKill {
                        id,
                        target_pid: primary_pid,
                    })
                    .await;
            });
            return Ok(());
        }

        // Case 2: In restart queue (backoff)
        if registry.restarting.remove(&id) {
            tracing::info!("Program {} was waiting to restart. Cancelled by user.", id);
            let name = registry
                .programs
                .get(&id)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let _ = self.log_tx.send(WsMessage::StatusChange {
                id,
                status: ProcessStatus::Stopped,
                name,
            });
            return Ok(());
        }

        // Case 3: Waiting on dependencies
        if registry.waiting.remove(&id) {
            tracing::info!(
                "Program {} removed from waiting queue by user stop request.",
                id
            );
            let name = registry
                .programs
                .get(&id)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            let _ = self.log_tx.send(WsMessage::StatusChange {
                id,
                status: ProcessStatus::Stopped,
                name,
            });
            return Ok(());
        }

        Err(anyhow::anyhow!("Program is not running"))
    }
}
