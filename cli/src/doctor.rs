//! `super doctor` — one-shot diagnostics for support and first-run triage.
//!
//! Aggregates: CLI version, config-file validation (shared with `super check`),
//! daemon connectivity/health, and license/plugin status.

use crate::check;
use crate::client;
use colored::Colorize;
use common::config::{ServerConfig, resolve_license_key};
use common::{
    HealthResponse, LicenseInfo, PidfileStatus, inspect_pidfile, pidfile_parent_unwritable,
    resolve_pidfile_path, resolve_super_root, under_systemd, verify_license_for_superd,
};

pub async fn run(base_url: &str, token: Option<&String>) -> anyhow::Result<()> {
    println!("{}", "Super Doctor".bold());
    println!("   CLI version:     {}", env!("CARGO_PKG_VERSION"));

    // 1. Config file validation (reuse `super check`; it prints its own report).
    println!("\n{}", "== Configuration ==".bold());
    match check::run(None) {
        Ok(()) => {}
        Err(e) => {
            println!("   {}", format!("config check reported: {e}").yellow());
        }
    }
    report_daemon_config();

    // 2. Daemon connectivity + health.
    println!("\n{}", "== Daemon ==".bold());
    let base_url = base_url.trim_end_matches('/');
    println!("   Server URL:      {}", base_url.cyan());
    report_pidfile_status();

    let client = match client::build_client(token) {
        Ok(c) => c,
        Err(e) => {
            println!("   {}", format!("cannot build HTTP client: {e}").red());
            return Ok(());
        }
    };

    let health_url = format!("{base_url}/health");
    let resp = match client.get(&health_url).send().await {
        Ok(r) => r,
        Err(e) => {
            println!("   Status:          {}", format!("unreachable ({e})").red());
            println!(
                "   Hint: start the daemon (`superd`) or pass --server / edit ~/.super/cli.json"
            );
            return Ok(());
        }
    };

    let http_status = resp.status();
    match resp.json::<HealthResponse>().await {
        Ok(h) => {
            let status_colored = match h.status.as_str() {
                "healthy" => h.status.green(),
                "degraded" => h.status.yellow(),
                other => other.red(),
            };
            println!("   Status:          {status_colored} (HTTP {http_status})");
            for (k, v) in &h.components {
                println!("     - {k}: {v}");
            }
        }
        Err(e) => {
            println!(
                "   Status:          {}",
                format!("HTTP {http_status}, unreadable health body: {e}").yellow()
            );
        }
    }

    // 3. License / edition (404 in OSS mode is expected, not an error).
    println!("\n{}", "== License ==".bold());
    let config_license = config_license_status();
    let license_url = format!("{base_url}/api/v1/system/license");
    match client.get(&license_url).send().await {
        Ok(r) if r.status() == reqwest::StatusCode::NOT_FOUND => match &config_license {
            ConfigLicenseStatus::Invalid { reason } => {
                println!(
                    "   Mode:            {}",
                    "OSS (license verification failed)".yellow()
                );
                println!(
                    "   Config key:      {}",
                    format!("invalid — {reason}").red()
                );
                println!(
                    "   Hint:            superd ignores invalid keys and runs without plugins; fix the key or remove [license].key"
                );
            }
            ConfigLicenseStatus::Valid => {
                println!(
                    "   Mode:            {}",
                    "OSS (daemon license endpoint unavailable)".yellow()
                );
                println!(
                    "   Config key:      {}",
                    "valid in config — restart superd or check SUPER_LICENSE override".green()
                );
            }
            ConfigLicenseStatus::None => {
                println!(
                    "   Mode:            {}",
                    "OSS (no license configured)".cyan()
                );
            }
        },
        Ok(r) if r.status().is_success() => match r.json::<LicenseInfo>().await {
            Ok(info) => {
                println!("   Mode:            {}", "Licensed".green());
                println!("   Issued to:       {}", info.issued_to);
                println!("   Subscription:    {}", info.subscription_status);
                if let Some(v) = &info.superd_version {
                    println!("   superd version:  {v}");
                }
                if let Some(in_range) = info.version_in_range
                    && !in_range
                {
                    println!(
                        "   {}",
                        format!(
                            "superd version outside licensed range (max {})",
                            info.max_superd_version
                        )
                        .yellow()
                    );
                }
                if !info.plugin_versions.is_empty() {
                    println!("   Plugins:");
                    for (id, ver) in &info.plugin_versions {
                        println!("     - {id}: {ver}");
                    }
                }
            }
            Err(e) => println!("   {}", format!("unreadable license body: {e}").yellow()),
        },
        Ok(r) => {
            println!(
                "   {}",
                format!("license endpoint returned HTTP {}", r.status()).yellow()
            );
        }
        Err(e) => {
            println!(
                "   {}",
                format!("license endpoint unreachable: {e}").yellow()
            );
        }
    }

    Ok(())
}

enum ConfigLicenseStatus {
    None,
    Valid,
    Invalid { reason: String },
}

/// Inspect `[license].key` / `SUPER_LICENSE` before interpreting the daemon license API.
fn config_license_status() -> ConfigLicenseStatus {
    let root = resolve_super_root();
    let path = root.join("conf/super.toml");
    let Ok(Some(key)) = resolve_license_key(&path) else {
        return ConfigLicenseStatus::None;
    };
    match verify_license_for_superd(&key) {
        Ok(_) => ConfigLicenseStatus::Valid,
        Err(e) => ConfigLicenseStatus::Invalid {
            reason: e.to_string(),
        },
    }
}

fn load_server_config() -> Option<(std::path::PathBuf, ServerConfig)> {
    let root = resolve_super_root();
    let path = root.join("conf/super.toml");
    let content = std::fs::read_to_string(&path).ok()?;
    let cfg = toml::from_str::<ServerConfig>(&content).ok()?;
    Some((root, cfg))
}

fn report_daemon_config() {
    let Some((root, cfg)) = load_server_config() else {
        return;
    };
    let daemon = cfg.server.daemon;
    println!(
        "   daemon:         {}",
        if daemon {
            "true".yellow().to_string()
        } else {
            "false".green().to_string()
        }
    );
    if daemon && under_systemd() {
        println!(
            "   {}",
            "ERROR: [server].daemon = true under systemd — set daemon = false and use Type=simple / superd --foreground"
                .red()
        );
    } else if daemon {
        println!(
            "   {}",
            "info: self-daemonize enabled; stop with `super shutdown` (not for systemd/Docker)"
                .cyan()
        );
    }

    let pidfile = resolve_pidfile_path(&root, cfg.server.pidfile.as_deref());
    println!("   pidfile:        {}", pidfile.display());
    if (daemon || cfg.server.pidfile.is_some()) && pidfile_parent_unwritable(&pidfile) {
        let parent = pidfile
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(none)".into());
        println!(
            "   {}",
            format!("ERROR: pidfile parent is not writable: {parent}").red()
        );
    }
}

fn report_pidfile_status() {
    let Some((root, cfg)) = load_server_config() else {
        return;
    };
    // Only report when daemon mode or an explicit pidfile is configured.
    if !cfg.server.daemon && cfg.server.pidfile.is_none() {
        return;
    }
    let path = resolve_pidfile_path(&root, cfg.server.pidfile.as_deref());
    match inspect_pidfile(&path) {
        PidfileStatus::Missing => {
            println!("   Pidfile:         {} (missing)", path.display());
        }
        PidfileStatus::Invalid => {
            println!(
                "   {}",
                format!("Pidfile:         {} (invalid contents)", path.display()).yellow()
            );
        }
        PidfileStatus::Stale { pid } => {
            println!(
                "   {}",
                format!(
                    "WARN: pidfile {} is stale (pid {pid} not running)",
                    path.display()
                )
                .yellow()
            );
        }
        PidfileStatus::Alive { pid } => {
            println!("   Pidfile:         {} (pid {pid} alive)", path.display());
        }
    }
}
