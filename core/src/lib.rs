pub mod api;
pub mod logger;
pub mod manager;
pub mod process;
pub mod store;

pub mod artifact;
pub mod event_hooks;
pub mod extension;
pub mod health;
pub mod hooks;
pub mod plugin;

pub mod client;
pub mod monitor;
pub mod scheduler;

pub use crate::client::ManagerHandle;
pub mod config {
    pub use common::config::*;
}
use crate::config::ServerConfig;
use crate::extension::Extension;
use crate::manager::Manager;

use std::path::PathBuf;
use tokio::sync::{broadcast, mpsc};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, reload, util::SubscriberInitExt};

// Public core handle: holds everything needed for system lifecycle
pub struct SystemCore {
    pub config: ServerConfig,
    pub manager_handle: ManagerHandle,
    pub log_tx: broadcast::Sender<common::WsMessage>,
    pub shutdown_tx: broadcast::Sender<()>,
    pub shutdown_rx: broadcast::Receiver<()>,

    pub paths: SystemPaths,

    // [Core Logic] Hold log guard so drop does not shut down the async log writer
    pub _log_guard: Option<WorkerGuard>,
}

#[derive(Clone, Debug)]
pub struct SystemPaths {
    pub root: PathBuf,
    pub config_file: PathBuf,
    pub notify_file: PathBuf,
    pub state_file: PathBuf,
    pub auth_file: PathBuf,
    pub log_dir: PathBuf,
    pub plugins_dir: PathBuf,
}

pub fn resolve_root() -> PathBuf {
    common::resolve_super_root()
}

// Bootstrap: env init, config load, logging, and Manager startup
pub async fn bootstrap(extension: Box<dyn Extension>) -> anyhow::Result<SystemCore> {
    // [Linux Only] Lower OOM score so the kernel is less likely to kill us under memory pressure
    #[cfg(target_os = "linux")]
    {
        let path = "/proc/self/oom_score_adj";
        if let Err(e) = tokio::fs::write(path, b"-1000").await {
            eprintln!("Failed to adjust OOM score: {}. Ignoring.", e);
        }
    }

    // 1. Resolve path layout
    let root = resolve_root();
    let conf_dir = root.join("conf");
    let data_dir = root.join("data");
    let log_dir = root.join("logs");
    let run_dir = root.join("run");

    let paths = SystemPaths {
        root: root.clone(),
        config_file: conf_dir.join("super.toml"),
        notify_file: conf_dir.join("notify.toml"),
        state_file: data_dir.join("snapshot.json"),
        auth_file: data_dir.join("auth.json"),
        log_dir: log_dir.clone(),
        plugins_dir: root.join("plugins"),
    };

    // Ensure plugin directory exists (drop-in `.so` files at startup).
    tokio::fs::create_dir_all(&paths.plugins_dir).await?;

    // 2. Create directories (run/ holds optional pidfile for --daemon)
    tokio::fs::create_dir_all(&conf_dir).await?;
    tokio::fs::create_dir_all(&data_dir).await?;
    tokio::fs::create_dir_all(&log_dir).await?;
    tokio::fs::create_dir_all(&run_dir).await?;

    // 3. Load config (strict: parse errors fail fast)
    let server_config = if paths.config_file.exists() {
        let content = tokio::fs::read_to_string(&paths.config_file).await?;
        match toml::from_str::<ServerConfig>(&content) {
            Ok(c) => c,
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Failed to parse config file {:?}: {}",
                    paths.config_file,
                    e
                ));
            }
        }
    } else {
        ServerConfig::default()
    };

    // 4. Init async logging
    tokio::fs::create_dir_all(&server_config.storage.log_dir).await?;
    let file_appender = tracing_appender::rolling::daily(&server_config.storage.log_dir, "app.log");

    // Keep guard alive for ongoing log writes
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Support dynamic log level via reload layer
    let (filter, reload_handle) =
        reload::Layer::new(EnvFilter::new(&server_config.logging.log_level));

    let stdout_layer = tracing_subscriber::fmt::layer().with_target(true);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(non_blocking);

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(stdout_layer)
        .with(file_layer)
        .try_init();

    tracing::info!("Super Core starting...");
    tracing::info!(root = ?root, config = ?paths.config_file, data = ?paths.state_file);

    if server_config.webhook.is_some() {
        tracing::warn!(
            "[webhook] in super.toml is parsed but not wired in OSS — use [[event_hooks]] for local scripts \
             or conf/notify.toml with the notify plugin for HTTP webhooks; see config reference"
        );
    }

    if !paths.config_file.exists() {
        tracing::warn!("Config file not found, using defaults");
    }

    // 5. Load persisted runtime snapshot
    let initial_programs = match store::load_with_recovery(&paths.state_file).await {
        Ok(p) => p,
        Err(e) => {
            // Unrecoverable error: log fatal and exit
            tracing::error!("FATAL: Configuration corruption detected!");
            tracing::error!("Error: {}", e);
            tracing::error!("System will NOT start to prevent data loss.");
            return Err(e); // abort bootstrap
        }
    };

    let (log_tx, _) = broadcast::channel(100);
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    let (tx, rx) = mpsc::channel(2048);

    // Build log-level reload callback
    let log_reloader = Box::new(move |new_level: String| -> anyhow::Result<()> {
        let new_filter = EnvFilter::new(new_level);
        reload_handle.reload(new_filter)?;
        Ok(())
    });

    let mut runtime_config = server_config.clone();
    runtime_config.storage.data_file = paths.state_file.clone();
    runtime_config.storage.log_dir = paths.log_dir.clone();

    // 6. Init Manager (core actor)
    let manager = Manager::new(
        runtime_config.clone(),
        paths.config_file.clone(),
        log_reloader,
        rx,
        tx.clone(),
        initial_programs,
        log_tx.clone(),
        extension,
    );
    let manager_handle = ManagerHandle::new(tx.clone());

    tokio::spawn(async move {
        manager.run().await;
    });

    Ok(SystemCore {
        config: runtime_config,
        manager_handle,
        log_tx,
        shutdown_tx,
        shutdown_rx,
        paths,
        _log_guard: Some(guard), // main must hold guard
    })
}
