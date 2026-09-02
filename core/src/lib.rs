pub mod api;
pub mod event_db;
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
    let run_dir = root.join("run");

    let paths = SystemPaths {
        root: root.clone(),
        config_file: conf_dir.join("super.toml"),
        notify_file: conf_dir.join("notify.toml"),
        state_file: root.join("data/snapshot.json"),
        auth_file: root.join("data/auth.json"),
        log_dir: root.join("logs"),
        plugins_dir: root.join("plugins"),
    };

    // Ensure plugin directory exists (drop-in `.so` files at startup).
    tokio::fs::create_dir_all(&paths.plugins_dir).await?;

    // 2. Load config (strict: parse errors fail fast)
    let server_config = if paths.config_file.exists() {
        let content = tokio::fs::read_to_string(&paths.config_file).await?;
        if common::config::legacy_webhook_section_present(&content) {
            return Err(anyhow::anyhow!(common::config::LEGACY_WEBHOOK_SECTION_MSG));
        }
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

    // Resolve `[storage]` paths under SUPER_ROOT (never relative to process CWD).
    let storage = server_config.storage.resolve_under_root(&root);
    if let Some(parent) = storage.data_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::create_dir_all(&storage.log_dir).await?;
    tokio::fs::create_dir_all(&conf_dir).await?;
    tokio::fs::create_dir_all(&run_dir).await?;

    // 3. Init async logging (same resolved log_dir as child process logs)
    let file_appender = tracing_appender::rolling::daily(&storage.log_dir, "app.log");

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
    tracing::info!(
        root = ?root,
        config = ?paths.config_file,
        data = ?storage.data_file,
        logs = ?storage.log_dir,
    );

    if !paths.config_file.exists() {
        tracing::warn!("Config file not found, using defaults");
    }

    // 4. Load persisted runtime snapshot
    let initial_programs = match store::load_with_recovery(&storage.data_file).await {
        Ok(p) => p,
        Err(e) => {
            // Unrecoverable error: log fatal and exit
            tracing::error!("FATAL: Configuration corruption detected!");
            tracing::error!("Error: {}", e);
            tracing::error!("System will NOT start to prevent data loss.");
            return Err(e); // abort bootstrap
        }
    };

    // 4b. Open the SQLite-backed event history store (auxiliary; never fatal).
    let event_db = match crate::event_db::EventDb::open(&storage.events_file).await {
        Ok(db) => db,
        Err(e) => {
            tracing::error!(
                "Failed to open events database at {}: {} — event history disabled",
                storage.events_file.display(),
                e
            );
            return Err(e);
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
    runtime_config.storage = storage.clone();

    let mut paths = paths;
    paths.state_file = storage.data_file.clone();
    paths.log_dir = storage.log_dir.clone();

    // 5. Init Manager (core actor)
    let manager = Manager::new(
        runtime_config.clone(),
        paths.config_file.clone(),
        log_reloader,
        rx,
        tx.clone(),
        initial_programs,
        log_tx.clone(),
        extension,
        event_db,
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
