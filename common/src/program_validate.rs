//! Shared validation for create/update program API bodies.
//! Called by the Manager before persist so CLI, HTTP, and the dashboard share one rule set.
//!
//! Error strings always name the JSON field (and, when known, the service index / program name)
//! so operators can find the bad value without grepping the payload.

use crate::security::resolve_confined_log_path;
use crate::{
    ArtifactConfig, CreateProgramRequest, HealthCheck, ResourceLimits, UpdateProgramRequest,
};
use anyhow::{Context, bail};
use std::path::Path;

/// Prefix a field-level error with `services[i] (name=…)` or `program '…'`.
pub fn with_program_location(
    err: anyhow::Error,
    name: Option<&str>,
    service_index: Option<usize>,
) -> anyhow::Error {
    match program_location(name, service_index) {
        Some(loc) => anyhow::anyhow!("{loc}: {err}"),
        None => err,
    }
}

/// `file:line:col: {serde error}` for include JSON / request-body parse failures.
pub fn format_serde_json_error(source: &str, err: &serde_json::Error) -> String {
    format!("{source}:{}:{}: {err}", err.line(), err.column())
}

fn program_location(name: Option<&str>, service_index: Option<usize>) -> Option<String> {
    let name = name.map(str::trim).filter(|s| !s.is_empty());
    match (service_index, name) {
        (Some(i), Some(n)) => Some(format!("services[{i}] (name={n})")),
        (Some(i), None) => Some(format!("services[{i}]")),
        (None, Some(n)) => Some(format!("program '{n}'")),
        (None, None) => None,
    }
}

/// Structural checks for `POST /api/v1/programs` (and stack apply / include JSON).
pub fn validate_create_program_request(
    req: &CreateProgramRequest,
    log_dir: &Path,
) -> anyhow::Result<()> {
    if req.command.trim().is_empty() {
        bail!("command: must not be empty");
    }
    if let Some(name) = &req.name
        && name.trim().is_empty()
    {
        bail!("name: must not be empty");
    }
    validate_health_check(req.health_check.as_ref())?;
    validate_program_log_paths(
        log_dir,
        req.stdout_logfile.as_deref(),
        req.stderr_logfile.as_deref(),
    )?;
    if let Some(artifact) = &req.artifact {
        validate_artifact_config(artifact)?;
    }
    if let Some(limits) = &req.resource_limits {
        validate_resource_limits_create(limits)?;
    }
    validate_cron_concurrency(req.max_concurrent, req.max_queued)?;
    Ok(())
}

/// Structural checks for `PUT /api/v1/programs/{id}` (only fields that are present).
pub fn validate_update_program_request(
    req: &UpdateProgramRequest,
    log_dir: &Path,
) -> anyhow::Result<()> {
    if let Some(command) = &req.command
        && command.trim().is_empty()
    {
        bail!("command: must not be empty");
    }
    if let Some(name) = &req.name
        && name.trim().is_empty()
    {
        bail!("name: must not be empty");
    }
    validate_health_check(req.health_check.as_ref())?;
    validate_program_log_paths(
        log_dir,
        req.stdout_logfile.as_deref(),
        req.stderr_logfile.as_deref(),
    )?;
    if let Some(artifact) = &req.artifact {
        validate_artifact_config(artifact)?;
    }
    validate_cron_concurrency(req.max_concurrent, req.max_queued)?;
    Ok(())
}

/// Shared bounds for cron concurrency fields (create and update).
fn validate_cron_concurrency(
    max_concurrent: Option<u32>,
    max_queued: Option<u32>,
) -> anyhow::Result<()> {
    if let Some(c) = max_concurrent {
        if c == 0 {
            // 0 means "use the default" — allowed, same as unset.
        } else if c > crate::MAX_CONCURRENT_CAP {
            bail!(
                "max_concurrent: must be ≤ {} (got {c})",
                crate::MAX_CONCURRENT_CAP
            );
        }
    }
    if let Some(q) = max_queued
        && q > crate::MAX_QUEUED_CAP
    {
        bail!("max_queued: must be ≤ {} (got {q})", crate::MAX_QUEUED_CAP);
    }
    Ok(())
}

pub fn validate_program_log_paths(
    log_dir: &Path,
    stdout_logfile: Option<&str>,
    stderr_logfile: Option<&str>,
) -> anyhow::Result<()> {
    if let Some(path) = stdout_logfile.filter(|s| !s.trim().is_empty()) {
        resolve_confined_log_path(log_dir, path)
            .with_context(|| format!("stdout_logfile ({path})"))?;
    }
    if let Some(path) = stderr_logfile.filter(|s| !s.trim().is_empty()) {
        resolve_confined_log_path(log_dir, path)
            .with_context(|| format!("stderr_logfile ({path})"))?;
    }
    Ok(())
}

pub fn validate_artifact_config(artifact: &ArtifactConfig) -> anyhow::Result<()> {
    if artifact.source.trim().is_empty() {
        bail!("artifact.source: must not be empty");
    }
    if artifact.destination.trim().is_empty() {
        bail!("artifact.destination: must not be empty");
    }
    let sum = artifact.checksum.trim();
    if sum.len() != 64 || !sum.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!(
            "artifact.checksum: must be a 64-char hex SHA256 digest (got {} chars)",
            sum.len()
        );
    }
    Ok(())
}

fn validate_resource_limits_create(limits: &ResourceLimits) -> anyhow::Result<()> {
    if let Some(c) = limits.cpu_quota
        && c <= 0.0
    {
        bail!("resource_limits.cpu_quota: must be > 0 cores (got {c})");
    }
    if let Some(m) = limits.memory_limit
        && m == 0
    {
        bail!("resource_limits.memory_limit: must be > 0 MB");
    }
    if let Some(w) = limits.memory_warn_percent
        && w > 100
    {
        bail!("resource_limits.memory_warn_percent: must be 0–100 (got {w})");
    }
    if let Some(h) = limits.memory_warn_headroom
        && h > 0
        && limits.memory_limit.is_none()
    {
        bail!("resource_limits.memory_warn_headroom: requires memory_limit");
    }
    if let Some(h) = limits.memory_high
        && h == 0
    {
        bail!("resource_limits.memory_high: must be > 0 MB");
    }
    Ok(())
}

fn validate_health_check(hc: Option<&HealthCheck>) -> anyhow::Result<()> {
    match hc {
        None | Some(HealthCheck::Disabled) => Ok(()),
        Some(check @ HealthCheck::Tcp { host, port, .. }) => {
            if host.trim().is_empty() {
                bail!("health_check.host: must not be empty");
            }
            if *port == 0 {
                bail!("health_check.port: must not be 0");
            }
            validate_health_tuning(check)
        }
        Some(check @ HealthCheck::Http { url, method, .. }) => {
            let u = url.trim();
            if !(u.starts_with("http://") || u.starts_with("https://")) {
                bail!("health_check.url: must start with http:// or https:// (got {u:?})");
            }
            if let Some(m) = method {
                let m = m.to_ascii_uppercase();
                if !matches!(m.as_str(), "GET" | "HEAD" | "POST") {
                    bail!("health_check.method: must be GET, HEAD, or POST (got {method:?})");
                }
            }
            validate_health_tuning(check)
        }
        Some(check @ HealthCheck::Exec { command, .. }) => {
            if command.trim().is_empty() {
                bail!("health_check.command: must not be empty");
            }
            validate_health_tuning(check)
        }
    }
}

/// Shared bounds for the health probe tuning knobs.
fn validate_health_tuning(hc: &HealthCheck) -> anyhow::Result<()> {
    let interval = hc.interval_secs();
    if interval == 0 || interval > crate::MAX_HEALTH_INTERVAL_SECS {
        bail!(
            "health_check.interval_secs: must be 0 (default) or 1..={} (got {interval})",
            crate::MAX_HEALTH_INTERVAL_SECS
        );
    }
    let timeout = hc.timeout_secs();
    if timeout == 0 || timeout > crate::MAX_HEALTH_TIMEOUT_SECS {
        bail!(
            "health_check.timeout_secs: must be 0 (default) or 1..={} (got {timeout})",
            crate::MAX_HEALTH_TIMEOUT_SECS
        );
    }
    let start_period = hc.start_period_secs();
    if start_period > crate::MAX_HEALTH_INTERVAL_SECS {
        bail!(
            "health_check.start_period_secs: must be 0 (default) or 1..={} (got {start_period})",
            crate::MAX_HEALTH_INTERVAL_SECS
        );
    }
    if hc.max_failures() > crate::MAX_HEALTH_MAX_FAILURES {
        bail!(
            "health_check.max_failures: must be 0 (disabled) or 1..={} (got {})",
            crate::MAX_HEALTH_MAX_FAILURES,
            hc.max_failures()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_logs() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn minimal_create(command: &str) -> CreateProgramRequest {
        CreateProgramRequest {
            command: command.into(),
            ..Default::default()
        }
    }

    #[test]
    fn create_rejects_empty_command() {
        let dir = tmp_logs();
        let err = validate_create_program_request(&minimal_create("  "), dir.path()).unwrap_err();
        assert!(err.to_string().contains("command:"), "{err}");
    }

    #[test]
    fn create_accepts_minimal() {
        let dir = tmp_logs();
        validate_create_program_request(&minimal_create("/bin/true"), dir.path()).unwrap();
    }

    #[test]
    fn create_rejects_http_health_without_scheme() {
        let dir = tmp_logs();
        let mut req = minimal_create("/bin/true");
        req.health_check = Some(HealthCheck::Http {
            url: "127.0.0.1/health".into(),
            method: None,
            interval_secs: 0,
            timeout_secs: 0,
            start_period_secs: 0,
            max_failures: 0,
        });
        let err = validate_create_program_request(&req, dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("health_check.url"), "{msg}");
        assert!(msg.contains("127.0.0.1/health"), "{msg}");
    }

    #[test]
    fn create_rejects_health_tuning_out_of_bounds() {
        let dir = tmp_logs();
        let mut req = minimal_create("/bin/true");
        req.health_check = Some(HealthCheck::Exec {
            command: "true".into(),
            interval_secs: crate::MAX_HEALTH_INTERVAL_SECS + 1,
            timeout_secs: 0,
            start_period_secs: 0,
            max_failures: 0,
        });
        let err = validate_create_program_request(&req, dir.path()).unwrap_err();
        assert!(err.to_string().contains("interval_secs"), "{err}");

        let mut req = minimal_create("/bin/true");
        req.health_check = Some(HealthCheck::Exec {
            command: "true".into(),
            interval_secs: 0,
            timeout_secs: crate::MAX_HEALTH_TIMEOUT_SECS + 1,
            start_period_secs: 0,
            max_failures: 0,
        });
        let err = validate_create_program_request(&req, dir.path()).unwrap_err();
        assert!(err.to_string().contains("timeout_secs"), "{err}");

        let mut req = minimal_create("/bin/true");
        req.health_check = Some(HealthCheck::Exec {
            command: "true".into(),
            interval_secs: 0,
            timeout_secs: 0,
            start_period_secs: crate::MAX_HEALTH_INTERVAL_SECS + 1,
            max_failures: 0,
        });
        let err = validate_create_program_request(&req, dir.path()).unwrap_err();
        assert!(err.to_string().contains("start_period_secs"), "{err}");

        let mut req = minimal_create("/bin/true");
        req.health_check = Some(HealthCheck::Exec {
            command: "true".into(),
            interval_secs: 0,
            timeout_secs: 0,
            start_period_secs: 0,
            max_failures: crate::MAX_HEALTH_MAX_FAILURES + 1,
        });
        let err = validate_create_program_request(&req, dir.path()).unwrap_err();
        assert!(err.to_string().contains("max_failures"), "{err}");
    }

    #[test]
    fn create_accepts_health_tuning_defaults_and_zero() {
        let dir = tmp_logs();
        let mut req = minimal_create("/bin/true");
        req.health_check = Some(HealthCheck::Exec {
            command: "true".into(),
            interval_secs: 0,
            timeout_secs: 0,
            start_period_secs: 0,
            max_failures: 0,
        });
        validate_create_program_request(&req, dir.path()).unwrap();
    }

    #[test]
    fn create_rejects_unknown_json_field() {
        let err = serde_json::from_str::<CreateProgramRequest>(
            r#"{"command":"/bin/true","not_a_field":1}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("unknown") || err.to_string().contains("not_a_field"));
        assert!(err.line() >= 1 && err.column() >= 1);
        let located = format_serde_json_error("stack.json", &err);
        assert!(located.starts_with("stack.json:"), "{located}");
        assert!(located.contains("not_a_field") || located.contains("unknown"));
    }

    #[test]
    fn update_rejects_empty_command() {
        let dir = tmp_logs();
        let req = UpdateProgramRequest {
            command: Some("".into()),
            ..Default::default()
        };
        let err = validate_update_program_request(&req, dir.path()).unwrap_err();
        assert!(err.to_string().contains("command:"), "{err}");
    }

    #[test]
    fn create_rejects_bad_artifact_checksum() {
        let dir = tmp_logs();
        let mut req = minimal_create("/bin/true");
        req.artifact = Some(ArtifactConfig {
            source: "https://example.com/a.tar.gz".into(),
            checksum: "abc".into(),
            extract: false,
            destination: "/tmp/x".into(),
            restart_policy: "always".into(),
        });
        let err = validate_create_program_request(&req, dir.path()).unwrap_err();
        assert!(err.to_string().contains("artifact.checksum"), "{err}");
    }

    #[test]
    fn location_includes_service_index_and_name() {
        let err = with_program_location(
            anyhow::anyhow!("command: must not be empty"),
            Some("web"),
            Some(2),
        );
        assert_eq!(
            err.to_string(),
            "services[2] (name=web): command: must not be empty"
        );
    }

    #[test]
    fn create_rejects_max_concurrent_over_cap() {
        let dir = tmp_logs();
        let mut req = minimal_create("/bin/true");
        req.max_concurrent = Some(crate::MAX_CONCURRENT_CAP + 1);
        let err = validate_create_program_request(&req, dir.path()).unwrap_err();
        assert!(err.to_string().contains("max_concurrent"), "{err}");
    }

    #[test]
    fn create_rejects_max_queued_over_cap() {
        let dir = tmp_logs();
        let mut req = minimal_create("/bin/true");
        req.max_queued = Some(crate::MAX_QUEUED_CAP + 1);
        let err = validate_create_program_request(&req, dir.path()).unwrap_err();
        assert!(err.to_string().contains("max_queued"), "{err}");
    }

    #[test]
    fn create_accepts_zero_and_defaults() {
        let dir = tmp_logs();
        let mut req = minimal_create("/bin/true");
        req.max_concurrent = Some(0); // 0 means default
        req.max_queued = Some(0);
        validate_create_program_request(&req, dir.path()).unwrap();
    }

    #[test]
    fn update_validates_cron_concurrency_bounds() {
        let dir = tmp_logs();
        let req = UpdateProgramRequest {
            max_concurrent: Some(crate::MAX_CONCURRENT_CAP + 1),
            max_queued: Some(3),
            ..Default::default()
        };
        let err = validate_update_program_request(&req, dir.path()).unwrap_err();
        assert!(err.to_string().contains("max_concurrent"), "{err}");

        let req = UpdateProgramRequest {
            max_concurrent: Some(4),
            max_queued: Some(crate::MAX_QUEUED_CAP + 1),
            ..Default::default()
        };
        let err = validate_update_program_request(&req, dir.path()).unwrap_err();
        assert!(err.to_string().contains("max_queued"), "{err}");
    }

    #[test]
    fn effective_values_normalize() {
        use crate::ProgramConfig;

        let default = ProgramConfig {
            name: "x".into(),
            command: "true".into(),
            ..Default::default()
        };
        assert_eq!(default.max_concurrent_eff(), 1);
        assert_eq!(default.max_queued_eff(), 100);

        let zero = ProgramConfig {
            name: "x".into(),
            command: "true".into(),
            max_concurrent: Some(0),
            max_queued: Some(0),
            ..Default::default()
        };
        assert_eq!(zero.max_concurrent_eff(), 1);
        assert_eq!(zero.max_queued_eff(), 100);

        let capped = ProgramConfig {
            name: "x".into(),
            command: "true".into(),
            max_concurrent: Some(crate::MAX_CONCURRENT_CAP * 2),
            max_queued: Some(crate::MAX_QUEUED_CAP * 2),
            ..Default::default()
        };
        assert_eq!(capped.max_concurrent_eff(), crate::MAX_CONCURRENT_CAP);
        assert_eq!(capped.max_queued_eff(), crate::MAX_QUEUED_CAP);
    }
}
