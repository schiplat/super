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
        bail!("resource_limits.cpu_quota: must be > 0 (got {c})");
    }
    if let Some(m) = limits.memory_limit
        && m == 0
    {
        bail!("resource_limits.memory_limit: must be > 0");
    }
    Ok(())
}

fn validate_health_check(hc: Option<&HealthCheck>) -> anyhow::Result<()> {
    match hc {
        None | Some(HealthCheck::Disabled) => Ok(()),
        Some(HealthCheck::Tcp { host, port }) => {
            if host.trim().is_empty() {
                bail!("health_check.host: must not be empty");
            }
            if *port == 0 {
                bail!("health_check.port: must not be 0");
            }
            Ok(())
        }
        Some(HealthCheck::Http { url, method }) => {
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
            Ok(())
        }
        Some(HealthCheck::Exec { command }) => {
            if command.trim().is_empty() {
                bail!("health_check.command: must not be empty");
            }
            Ok(())
        }
    }
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
        });
        let err = validate_create_program_request(&req, dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("health_check.url"), "{msg}");
        assert!(msg.contains("127.0.0.1/health"), "{msg}");
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
}
