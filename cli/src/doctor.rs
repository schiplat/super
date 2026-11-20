//! `super doctor` — one-shot diagnostics for support and first-run triage.
//!
//! Aggregates: CLI version, config-file validation (shared with `super check`),
//! daemon connectivity/health, and license/plugin status.

use crate::check;
use crate::client;
use colored::Colorize;
use common::config::{ServerConfig, resolve_license_key};
use common::sensitive_file::{PathKind, PermissionFinding, PermissionSeverity, inspect};
use common::{
    HealthResponse, LicenseInfo, PidfileStatus, embedded_keyring_summary, inspect_pidfile,
    pidfile_parent_unwritable, resolve_pidfile_path, resolve_super_root, under_systemd,
    verify_license_for_superd,
};

pub async fn run(base_url: &str, token: Option<&String>) -> anyhow::Result<()> {
    println!("{}", "Super Doctor".bold());
    println!("   CLI version:     {}", env!("CARGO_PKG_VERSION"));
    println!("   Root key:        {}", embedded_keyring_summary().cyan());
    let mut diagnostics_ok = true;

    // 1. Config file validation (reuse `super check`; it prints its own report).
    println!("\n{}", "== Configuration ==".bold());
    if let Err(e) = check::run(None, false) {
        diagnostics_ok = false;
        println!("   {}", format!("config check reported: {e}").yellow());
    }
    report_daemon_config();

    // 1b. Sensitive-file permissions: credentials must not be readable by other
    // users, and nothing that decides what the daemon executes may be writable
    // by them.
    println!("\n{}", "== Permissions ==".bold());
    if !report_permissions() {
        diagnostics_ok = false;
    }

    // 2. Daemon connectivity + health.
    println!("\n{}", "== Daemon ==".bold());
    let (base, _socket) = client::split_server(base_url);
    println!("   Server URL:      {}", base_url.cyan());
    report_pidfile_status();

    let client = match client::build_api_client(base_url, token) {
        Ok(c) => c,
        Err(e) => {
            println!("   {}", format!("cannot build API client: {e}").red());
            return Err(e);
        }
    };

    let health_url = format!("{base}/health");
    let resp = match client.get(&health_url).send().await {
        Ok(r) => r,
        Err(e) => {
            println!("   Status:          {}", format!("unreachable ({e})").red());
            println!(
                "   Hint: start the daemon (`superd`) or pass --server / edit ~/.super/cli.json"
            );
            return Err(anyhow::anyhow!("Daemon health endpoint is unreachable"));
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
            if !http_status.is_success() || h.status != "healthy" {
                diagnostics_ok = false;
            }
            for (k, v) in &h.components {
                println!("     - {k}: {v}");
            }
        }
        Err(e) => {
            diagnostics_ok = false;
            println!(
                "   Status:          {}",
                format!("HTTP {http_status}, unreadable health body: {e}").yellow()
            );
        }
    }

    // 3. License / edition (404 in OSS mode is expected, not an error).
    println!("\n{}", "== License ==".bold());
    let config_license = config_license_status();
    let license_url = format!("{base}/api/v1/system/license");
    match client.get(&license_url).send().await {
        Ok(r) if r.status() == reqwest::StatusCode::NOT_FOUND => match &config_license {
            ConfigLicenseStatus::Invalid { reason } => {
                diagnostics_ok = false;
                println!(
                    "   Mode:            {}",
                    "OSS (license verification failed)".yellow()
                );
                println!(
                    "   Config key:      {}",
                    format!("invalid — {reason}").red()
                );
                println!(
                    "   Hint:            superd refuses startup when strict or licensed deployment signals apply; otherwise runs OSS without plugins"
                );
            }
            ConfigLicenseStatus::Valid => {
                diagnostics_ok = false;
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
            Err(e) => {
                diagnostics_ok = false;
                println!("   {}", format!("unreadable license body: {e}").yellow());
            }
        },
        Ok(r) => {
            if !r.status().is_success() {
                diagnostics_ok = false;
            }
            println!(
                "   {}",
                format!("license endpoint returned HTTP {}", r.status()).yellow()
            );
        }
        Err(e) => {
            if matches!(&config_license, ConfigLicenseStatus::Valid) {
                diagnostics_ok = false;
            }
            println!(
                "   {}",
                format!("license endpoint unreachable: {e}").yellow()
            );
        }
    }

    if diagnostics_ok {
        Ok(())
    } else {
        Err(anyhow::anyhow!("One or more diagnostics failed"))
    }
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

/// Check the mode bits on everything that carries credentials or decides what
/// the daemon executes. Returns `false` only for integrity problems
/// (world-writable paths); confidentiality exposure is reported as a warning.
fn report_permissions() -> bool {
    let Some((root, cfg)) = load_server_config() else {
        println!(
            "   {}",
            "conf/super.toml not readable — sensitive-path checks skipped".yellow()
        );
        return true;
    };

    let audit = common::sensitive_file::audit_instance(&root, &cfg);
    let mut checked = audit.checked;
    let mut findings = audit.findings;

    // The CLI's own credential store (`auth_token`, often the admin secret)
    // lives outside the instance root but on the same host.
    if let Some(path) = crate::config::CliConfig::try_path() {
        match inspect(&path, PathKind::Secret) {
            Ok(Some(finding)) => {
                checked += 1;
                findings.push(finding);
            }
            Ok(None) => {}
            Err(e) => println!(
                "   {}",
                format!("cannot inspect {}: {e}", path.display()).yellow()
            ),
        }
    }

    findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.path.as_os_str().cmp(b.path.as_os_str()))
    });

    println!("   Instance root:   {}", root.display());
    if findings.is_empty() {
        println!(
            "   {}",
            format!("{checked} sensitive paths checked — none group/world accessible").green()
        );
        return true;
    }

    for finding in &findings {
        print_permission_finding(finding);
    }

    let errors = findings
        .iter()
        .filter(|f| f.severity == PermissionSeverity::Error)
        .count();
    let warnings = findings.len() - errors;
    if errors > 0 {
        println!(
            "   {}",
            format!("{errors} error(s), {warnings} warning(s) — apply the chmod hints above").red()
        );
    } else {
        println!(
            "   {}",
            format!("{warnings} warning(s), no world-writable paths").yellow()
        );
    }
    errors == 0
}

fn print_permission_finding(finding: &PermissionFinding) {
    let tag = match finding.severity {
        PermissionSeverity::Error => "ERROR".red(),
        PermissionSeverity::Warning => "WARN ".yellow(),
    };
    println!(
        "   {tag} {} 0{:03o} {} — {}",
        finding.path.display(),
        finding.mode,
        finding.reason,
        finding.hint
    );
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
