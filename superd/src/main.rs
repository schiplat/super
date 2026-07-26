use axum::{
    Router,
    http::{HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use clap::Parser;
use common::config::ServerConfig;
use common::license::{LicenseInfo, superd_within_license};
use common::{
    claim_pidfile, release_pidfile, resolve_daemonize, resolve_pidfile_path, should_write_pidfile,
    under_systemd,
};
use std::path::{Path, PathBuf};
use super_core::{
    ManagerHandle, api, bootstrap,
    plugin::{
        PluginHost, RunMode, attach_http_plugins, load_ui_plugin, normalize_ui_path,
        validate_licensed_auth_secret, validate_licensed_security,
    },
    resolve_root,
};
use tokio::signal;

#[cfg(unix)]
mod daemonize;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
#[command(
    name = "superd",
    version = VERSION,
    about = "Project Super daemon",
    long_about = "Long-running process manager. Configuration: $SUPER_ROOT/conf/super.toml\n\
Docs: https://super.docs.sconts.com/docs/"
)]
struct Cli {
    /// Self-daemonize (Unix). Overrides `[server].daemon = false`.
    #[arg(long, conflicts_with = "foreground")]
    daemon: bool,

    /// Stay in the foreground (default). Overrides `[server].daemon = true`.
    /// Use under systemd/Docker.
    #[arg(long, conflicts_with = "daemon")]
    foreground: bool,

    /// Pidfile path (absolute or relative to SUPER_ROOT).
    /// When daemonizing and unset, defaults to run/superd.pid.
    #[arg(long, value_name = "PATH")]
    pidfile: Option<PathBuf>,
}

const OSS_UI_MESSAGE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Super — OSS</title>
  <style>
    :root { color-scheme: light dark; }
    body {
      font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      max-width: 40rem;
      margin: 0 auto;
      padding: 3rem 1.25rem 4rem;
      line-height: 1.6;
      background: #fafaf7;
    }
    .card {
      background: #fff;
      color: #171613;
      border: 1px solid #e8e6e1;
      border-radius: 1rem;
      padding: 1.75rem 1.5rem;
      box-shadow: 0 1px 2px rgba(0,0,0,.04);
    }
    h1 { font-size: 1.375rem; margin: 0 0 .4rem; letter-spacing: -0.02em; font-weight: 700; color: inherit; }
    .lead { font-size: 0.975rem; margin: 0 0 1rem; color: #6b6760; }
    p { margin: 0 0 .85rem; color: inherit; font-size: 0.9375rem; }
    ul { margin: .35rem 0 1rem; padding-left: 1.2rem; color: inherit; font-size: 0.9375rem; }
    li { margin: .2rem 0; }
    code { background: #f3f2ef; color: #171613; padding: .1rem .35rem; border-radius: .25rem; font-size: .9em; }
    .actions { display: flex; flex-wrap: wrap; gap: .6rem; margin-top: 1.25rem; }
    .actions a {
      display: inline-block;
      padding: .5rem .95rem;
      border-radius: .5rem;
      font-size: .875rem;
      font-weight: 600;
      text-decoration: none;
      transition: opacity .15s;
    }
    .actions a:hover { opacity: .88; }
    .cta-primary { background: #0d9488; color: #fff; }
    .cta-secondary {
      background: #f3f2ef;
      color: #171613;
      border: 1px solid #e8e6e1;
    }
    .muted { font-size: .8125rem; color: #9c9890; margin-top: 1.25rem; margin-bottom: 0; }
    @media (prefers-color-scheme: dark) {
      body { background: #0d0d0c; }
      .card { background: #161613; color: #ededeb; border-color: #2a2a25; }
      .lead { color: #a19e96; }
      code { background: #1c1c18; color: #ededeb; }
      .muted { color: #6e6b64; }
      .cta-primary { background: #14b8a6; }
      .cta-secondary { background: #1c1c18; color: #ededeb; border-color: #2a2a25; }
    }
  </style>
</head>
<body>
  <div class="card">
    <h1>Super is running</h1>
    <p class="lead">Open-source build — manage processes with the CLI and HTTP API.</p>
    <p>Included in this binary:</p>
    <ul>
      <li><code>super</code> CLI for day-to-day control</li>
      <li><code>/api/v1/*</code> for scripts and CI/CD</li>
      <li><code>/metrics</code> for Prometheus</li>
    </ul>
    <p>
      There is no built-in dashboard in OSS.
      <strong>Super Pro</strong> loads signed plugins on this same <code>superd</code>
      binary: Web UI, API auth, notifications, and Linux resource limits.
    </p>
    <div class="actions">
      <a class="cta-primary" href="https://super.docs.sconts.com/go/pro/" rel="noopener noreferrer">Get Super Pro</a>
      <a class="cta-secondary" href="https://super.docs.sconts.com/docs/07-editions/feature-matrix" rel="noopener noreferrer">Feature matrix</a>
      <a class="cta-secondary" href="https://super.docs.sconts.com/docs/" rel="noopener noreferrer">Docs</a>
    </div>
    <p class="muted">Version VERSION_PLACEHOLDER · MIT open-source core</p>
  </div>
</body>
</html>
"#;

async fn ui_fallback_handler(
    uri: Uri,
    ui: Option<std::sync::Arc<super_core::plugin::UiPluginHandle>>,
    auth_required: bool,
    is_licensed: bool,
) -> Response {
    let path = uri.path();
    if path.starts_with("/api/")
        || path == "/health"
        || path == "/metrics"
        || path.starts_with("/ws")
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(ui) = ui else {
        let html = OSS_UI_MESSAGE.replace("VERSION_PLACEHOLDER", VERSION);
        return html_response(&html);
    };

    serve_ui_asset(
        &ui,
        &normalize_ui_path(path),
        auth_required,
        is_licensed,
        false,
    )
    .unwrap_or_else(|| spa_fallback(&ui, auth_required, is_licensed))
}

fn spa_fallback(
    ui: &super_core::plugin::UiPluginHandle,
    auth_required: bool,
    is_licensed: bool,
) -> Response {
    serve_ui_asset(ui, "index.html", auth_required, is_licensed, true)
        .unwrap_or(StatusCode::NOT_FOUND.into_response())
}

fn serve_ui_asset(
    ui: &super_core::plugin::UiPluginHandle,
    file_path: &str,
    auth_required: bool,
    is_licensed: bool,
    inject_config: bool,
) -> Option<Response> {
    let asset = ui.resolve(file_path)?;
    let body = if inject_config || file_path == "index.html" {
        inject_ui_config(asset.data, auth_required, is_licensed)
    } else {
        bytes::Bytes::copy_from_slice(asset.data)
    };

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(asset.mime)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );

    Some((headers, body).into_response())
}

fn inject_ui_config(raw_html: &[u8], auth_required: bool, is_licensed: bool) -> bytes::Bytes {
    let html_str = String::from_utf8_lossy(raw_html);
    let edition = if is_licensed { "licensed" } else { "oss" };
    let config_js = format!(
        "window.__SUPER_CONFIG__ = {{ edition: '{edition}', auth_required: {auth_required}, version: '{VERSION}' }};",
        auth_required = auth_required,
    );
    let mut injected = html_str.replace("window.__SUPER_CONFIG__ = defaultConfig;", &config_js);
    if injected == html_str {
        injected = html_str.replace("// __INJECT_CONFIG__", &config_js);
    }
    bytes::Bytes::from(injected.into_bytes())
}

fn html_response(html: &str) -> Response {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));
    (headers, html.to_string()).into_response()
}

async fn shutdown_signal(mut rx: tokio::sync::broadcast::Receiver<()>, manager: ManagerHandle) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = rx.recv() => {
            tracing::info!("Internal shutdown signal received. Web server stopping.");
        },
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C. Initiating graceful shutdown...");
            if let Err(e) = manager.shutdown().await {
                tracing::error!("Manager shutdown failed: {}", e);
            }
        },
        _ = terminate => {
            tracing::info!("Received SIGTERM. Initiating graceful shutdown...");
            if let Err(e) = manager.shutdown().await {
                tracing::error!("Manager shutdown failed: {}", e);
            }
        },
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let root = resolve_root();
    let (config_daemon, config_pidfile) = peek_server_daemon_settings(&root);
    let daemonize = resolve_daemonize(cli.foreground, cli.daemon, config_daemon);
    let explicit_pidfile = cli.pidfile.is_some() || config_pidfile.is_some();
    let write_pid = should_write_pidfile(daemonize, explicit_pidfile);
    let pidfile_path = if write_pid {
        Some(resolve_pidfile_path(
            &root,
            cli.pidfile.as_deref().or(config_pidfile.as_deref()),
        ))
    } else {
        None
    };

    if daemonize {
        #[cfg(not(unix))]
        {
            anyhow::bail!("self-daemonize (--daemon / [server].daemon) is only supported on Unix");
        }
        #[cfg(unix)]
        {
            if under_systemd() {
                anyhow::bail!(
                    "daemonize is enabled but this process was started by systemd \
                     (INVOCATION_ID/NOTIFY_SOCKET set). Set [server].daemon = false and use \
                     Type=simple with a foreground ExecStart (e.g. superd --foreground)."
                );
            }
            if daemonize::is_pid1() {
                anyhow::bail!(
                    "refusing to daemonize as PID 1 (container/init). Run in the foreground."
                );
            }
            if let Some(ref path) = pidfile_path {
                daemonize::preflight_pidfile(path)?;
            }
            daemonize::daemonize()?;
        }
    }

    if let Some(ref path) = pidfile_path {
        claim_pidfile(path, std::process::id() as i32)?;
    }

    let pidfile_for_cleanup = pidfile_path.clone();
    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async_main());

    if let Some(ref path) = pidfile_for_cleanup {
        release_pidfile(path, std::process::id() as i32);
    }
    result
}

/// Read only `[server].daemon` / `[server].pidfile` before full bootstrap.
fn peek_server_daemon_settings(root: &Path) -> (bool, Option<PathBuf>) {
    let path = root.join("conf/super.toml");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return (false, None);
    };
    let Ok(cfg) = toml::from_str::<ServerConfig>(&content) else {
        // Full bootstrap will surface parse errors.
        return (false, None);
    };
    (cfg.server.daemon, cfg.server.pidfile)
}

async fn async_main() -> anyhow::Result<()> {
    let root = resolve_root();

    let plugin_host = PluginHost::discover(&root, VERSION);
    let licensed_plugins = plugin_host.licensed_plugins.clone();
    let loaded_plugins = plugin_host.loaded_plugins.clone();
    let is_licensed = plugin_host.is_licensed();
    let mut plugin_runtime = plugin_host.runtime;
    let auth_expected = plugin_runtime.loaded_ids.iter().any(|id| id == "security");
    let extension = plugin_runtime.take_extension();
    let ui_plugin = load_ui_plugin(&plugin_runtime);

    let core = bootstrap(extension).await?;

    validate_licensed_security(
        plugin_host.mode,
        plugin_host.claims.as_ref(),
        &loaded_plugins,
        &plugin_host.installed_plugins,
        &plugin_host.plugins_dir,
    )?;
    validate_licensed_auth_secret(
        plugin_host.mode,
        &loaded_plugins,
        core.config.auth_secret.as_deref(),
    )?;

    if is_licensed {
        tracing::info!(
            "Licensed plugins active: {:?} (loaded: {:?})",
            licensed_plugins,
            loaded_plugins
        );
    }

    if ui_plugin.is_some() {
        tracing::info!("Licensed UI plugin active");
    }

    let license_info = plugin_host.claims.as_ref().map(|claims| {
        let mut info = LicenseInfo::from(claims);
        info.plugin_versions = plugin_runtime.plugin_versions.clone();
        info.superd_version = Some(VERSION.to_string());
        info.version_in_range = Some(superd_within_license(claims, VERSION));
        info
    });
    if license_info.is_some() {
        tracing::info!("License API enabled at GET /api/v1/system/license");
    } else {
        tracing::warn!("No license in AppState; GET /api/v1/system/license will return 404");
    }

    let base_router = api::make_api_router(
        core.manager_handle.clone(),
        core.log_tx,
        core.shutdown_tx,
        core.config.clone(),
        !auth_expected,
        license_info,
    );

    let (api_router, auth_required) =
        attach_http_plugins(base_router, &plugin_runtime, &core.paths)?;

    if auth_required {
        tracing::info!("Plugin HTTP auth middleware active");
    } else if plugin_host.mode == RunMode::Licensed {
        anyhow::bail!(
            "Licensed deployment requires the security plugin HTTP auth middleware, but it is not active. \
             Ensure security.so exports authenticate and re-check superd logs."
        );
    }

    let auth_flag = auth_required;
    let licensed_flag = is_licensed;
    let ui_handle = ui_plugin.clone();
    let app = Router::new().merge(api_router).fallback(move |uri: Uri| {
        let ui = ui_handle.clone();
        async move { ui_fallback_handler(uri, ui, auth_flag, licensed_flag).await }
    });

    let addr = format!("{}:{}", core.config.server.host, core.config.server.port);

    if !common::is_loopback_bind_host(&core.config.server.host)
        && !auth_required
        && !core.config.server.allow_insecure_public_bind
    {
        anyhow::bail!(
            "Refusing to bind to {} without authentication. \
             Set server.allow_insecure_public_bind = true to acknowledge the risk, \
             bind to 127.0.0.1, or load the security plugin.",
            core.config.server.host
        );
    }

    let listener = tokio::net::TcpListener::bind(&addr).await?;

    if auth_required {
        tracing::info!(
            "Superd listening on {} (plugins: {:?}, auth enabled)",
            addr,
            loaded_plugins
        );
    } else if is_licensed {
        tracing::info!(
            "Superd listening on {} (plugins: {:?})",
            addr,
            loaded_plugins
        );
    } else {
        tracing::info!("Superd (OSS) listening on {}", addr);
    }

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(core.shutdown_rx, core.manager_handle))
    .await?;

    Ok(())
}
