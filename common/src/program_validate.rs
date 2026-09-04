//! Shared validation for create/update program API bodies.
//! Called by the Manager before persist so CLI, HTTP, and the dashboard share one rule set.
//!
//! Error strings always name the JSON field (and, when known, the service index / program name)
//! so operators can find the bad value without grepping the payload.

use crate::security::resolve_confined_log_path;
use crate::{
    ArtifactConfig, CreateProgramRequest, HealthCheck, ResourceLimits, StackApplyRequest,
    UpdateProgramRequest,
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

/// Parse a declarative stack file into `StackApplyRequest`.
///
/// Dispatch on extension: `.json` → JSON (legacy); anything else — including no
/// extension — → TOML (default). Both formats feed the same validation pipeline.
pub fn parse_stack_from_str(content: &str, path: &Path) -> anyhow::Result<StackApplyRequest> {
    let source = path.display().to_string();
    if path.extension().is_some_and(|ext| ext == "json") {
        serde_json::from_str::<StackApplyRequest>(content)
            .map_err(|e| anyhow::anyhow!(format_serde_json_error(&source, &e)))
    } else {
        toml::from_str::<StackApplyRequest>(content)
            .map_err(|e| anyhow::anyhow!(format_toml_error(&source, content, &e)))
    }
}

/// `file:line:col: {toml error}` mirroring `format_serde_json_error`.
pub fn format_toml_error(source: &str, input: &str, err: &toml::de::Error) -> String {
    match err.span().and_then(|span| line_col_at(input, span.start)) {
        Some((line, col)) => format!("{source}:{line}:{col}: {}", err.message()),
        None => format!("{source}: {}", err.message()),
    }
}

fn line_col_at(input: &str, byte: usize) -> Option<(usize, usize)> {
    let prefix = input.get(..byte)?;
    let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
    let col = prefix[prefix.rfind('\n').map_or(0, |i| i + 1)..]
        .chars()
        .count()
        + 1;
    Some((line, col))
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
        validate_signal_restart_requires_health_probe(Some(artifact), req.health_check.as_ref())?;
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
    // Empty `source` is the update clear-sentinel (omit field = no change; empty = remove).
    if let Some(artifact) = &req.artifact
        && !artifact.source.trim().is_empty()
    {
        validate_artifact_config(artifact)?;
        // Full create-style check when both fields are in the same request.
        // Updates that only touch one side are validated after merge in Manager.
        if req.health_check.is_some() {
            validate_signal_restart_requires_health_probe(
                Some(artifact),
                req.health_check.as_ref(),
            )?;
        }
    }
    validate_cron_concurrency(req.max_concurrent, req.max_queued)?;
    Ok(())
}

/// `restart_policy=signal*` does not exec a new process, so a live health probe is
/// required to verify the hot-reload. Synthetic Healthy / `startsecs` alone is not enough.
pub fn validate_signal_restart_requires_health_probe(
    artifact: Option<&ArtifactConfig>,
    health_check: Option<&HealthCheck>,
) -> anyhow::Result<()> {
    if signal_restart_missing_health_probe(artifact, health_check) {
        bail!(
            "artifact.restart_policy: signal* requires an enabled health_check \
             (tcp/http/exec) so the hot-reload can be verified; configure health_check \
             or use restart_policy=immediate|manual"
        );
    }
    Ok(())
}

/// `true` when `restart_policy` is `signal*` and there is no enabled health probe.
///
/// Used by startup / `super check` warnings for legacy configs that predate the
/// create-time rejection. Invalid `restart_policy` strings return `false`.
pub fn signal_restart_missing_health_probe(
    artifact: Option<&ArtifactConfig>,
    health_check: Option<&HealthCheck>,
) -> bool {
    let Some(artifact) = artifact else {
        return false;
    };
    let Ok(policy) = parse_artifact_restart_policy(&artifact.restart_policy) else {
        return false;
    };
    if !matches!(policy, ArtifactRestartPolicy::Signal { .. }) {
        return false;
    }
    !health_check.is_some_and(|h| h.is_enabled())
}

/// `true` for exec health commands that always succeed and never verify a hot-reload.
///
/// Recognized (trim + ASCII lowercase): `true`, `:`, `/bin/true`, `/usr/bin/true`.
pub fn trivial_exec_health_probe(health_check: Option<&HealthCheck>) -> bool {
    match health_check {
        Some(HealthCheck::Exec { command, .. }) => matches!(
            command.trim().to_ascii_lowercase().as_str(),
            "true" | ":" | "/bin/true" | "/usr/bin/true"
        ),
        _ => false,
    }
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
    parse_artifact_restart_policy(&artifact.restart_policy)
        .map_err(|e| anyhow::anyhow!("artifact.restart_policy: {e}"))?;
    Ok(())
}

/// Parsed OTA restart policy after a successful binary swap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactRestartPolicy {
    /// SIGTERM restart (or spawn if stopped) + verification timer.
    Immediate,
    /// Commit the swap immediately; do not restart or verify.
    Manual,
    /// Deliver `signal` without marking restart_requested; keep WAL + verify timer.
    /// `signal` is a lowercase name: hup, int, term, quit, usr1, usr2.
    Signal { signal: &'static str },
}

/// Parse and validate `artifact.restart_policy`.
///
/// Accepted values:
/// - `""` / `immediate` → [`ArtifactRestartPolicy::Immediate`]
/// - `manual`
/// - `signal` (defaults to `hup`)
/// - `signal:<name>` where name ∈ hup|int|term|quit|usr1|usr2
pub fn parse_artifact_restart_policy(raw: &str) -> anyhow::Result<ArtifactRestartPolicy> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() || s == "immediate" {
        return Ok(ArtifactRestartPolicy::Immediate);
    }
    if s == "manual" {
        return Ok(ArtifactRestartPolicy::Manual);
    }
    if s == "signal" {
        return Ok(ArtifactRestartPolicy::Signal { signal: "hup" });
    }
    if let Some(rest) = s.strip_prefix("signal:") {
        let name = rest.trim();
        let signal = match name {
            "hup" | "sighup" => "hup",
            "int" | "sigint" => "int",
            "term" | "sigterm" => "term",
            "quit" | "sigquit" => "quit",
            "usr1" | "sigusr1" => "usr1",
            "usr2" | "sigusr2" => "usr2",
            _ => bail!(
                "unknown signal {name:?}; expected hup|int|term|quit|usr1|usr2 (or signal:<name>)"
            ),
        };
        return Ok(ArtifactRestartPolicy::Signal { signal });
    }
    bail!(
        "must be immediate, manual, signal, or signal:<hup|int|term|quit|usr1|usr2> (got {raw:?})"
    );
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
    use std::collections::HashMap;

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
            restart_policy: "immediate".into(),
        });
        let err = validate_create_program_request(&req, dir.path()).unwrap_err();
        assert!(err.to_string().contains("artifact.checksum"), "{err}");
    }

    #[test]
    fn create_rejects_bad_restart_policy() {
        let dir = tmp_logs();
        let mut req = minimal_create("/bin/true");
        req.artifact = Some(ArtifactConfig {
            source: "https://example.com/a".into(),
            checksum: "a".repeat(64),
            extract: false,
            destination: "/tmp/x".into(),
            restart_policy: "always".into(),
        });
        let err = validate_create_program_request(&req, dir.path()).unwrap_err();
        assert!(err.to_string().contains("artifact.restart_policy"), "{err}");
    }

    #[test]
    fn parse_restart_policy_variants() {
        use super::{ArtifactRestartPolicy, parse_artifact_restart_policy};
        assert_eq!(
            parse_artifact_restart_policy("").unwrap(),
            ArtifactRestartPolicy::Immediate
        );
        assert_eq!(
            parse_artifact_restart_policy("manual").unwrap(),
            ArtifactRestartPolicy::Manual
        );
        assert_eq!(
            parse_artifact_restart_policy("signal").unwrap(),
            ArtifactRestartPolicy::Signal { signal: "hup" }
        );
        assert_eq!(
            parse_artifact_restart_policy("signal:USR1").unwrap(),
            ArtifactRestartPolicy::Signal { signal: "usr1" }
        );
        assert!(parse_artifact_restart_policy("signal:kill").is_err());
    }

    #[test]
    fn create_rejects_signal_restart_without_health_check() {
        let dir = tmp_logs();
        let mut req = minimal_create("/bin/true");
        req.artifact = Some(ArtifactConfig {
            source: "https://example.com/a".into(),
            checksum: "a".repeat(64),
            extract: false,
            destination: "/tmp/x".into(),
            restart_policy: "signal:hup".into(),
        });
        let err = validate_create_program_request(&req, dir.path()).unwrap_err();
        assert!(
            err.to_string().contains("signal") && err.to_string().contains("health_check"),
            "{err}"
        );
    }

    #[test]
    fn create_allows_signal_restart_with_health_check() {
        let dir = tmp_logs();
        let mut req = minimal_create("/bin/true");
        req.health_check = Some(HealthCheck::Exec {
            command: "true".into(),
            interval_secs: 5,
            timeout_secs: 0,
            start_period_secs: 0,
            max_failures: 0,
        });
        req.artifact = Some(ArtifactConfig {
            source: "https://example.com/a".into(),
            checksum: "a".repeat(64),
            extract: false,
            destination: "/tmp/x".into(),
            restart_policy: "signal".into(),
        });
        validate_create_program_request(&req, dir.path()).unwrap();
    }

    #[test]
    fn signal_restart_missing_health_probe_detects_gap() {
        let art = ArtifactConfig {
            source: "https://example.com/a".into(),
            checksum: "a".repeat(64),
            extract: false,
            destination: "/tmp/x".into(),
            restart_policy: "signal:hup".into(),
        };
        assert!(signal_restart_missing_health_probe(Some(&art), None));
        assert!(signal_restart_missing_health_probe(
            Some(&art),
            Some(&HealthCheck::Disabled)
        ));
        let hc = HealthCheck::Tcp {
            host: "127.0.0.1".into(),
            port: 8080,
            interval_secs: 5,
            timeout_secs: 0,
            start_period_secs: 0,
            max_failures: 0,
        };
        assert!(!signal_restart_missing_health_probe(Some(&art), Some(&hc)));
        assert!(!signal_restart_missing_health_probe(None, None));
    }

    #[test]
    fn trivial_exec_health_probe_recognizes_always_true() {
        for cmd in ["true", "TRUE", " : ", "/bin/true", "/usr/bin/true"] {
            let hc = HealthCheck::Exec {
                command: cmd.into(),
                interval_secs: 5,
                timeout_secs: 0,
                start_period_secs: 0,
                max_failures: 0,
            };
            assert!(trivial_exec_health_probe(Some(&hc)), "cmd={cmd}");
        }
        let real = HealthCheck::Exec {
            command: "curl -f http://127.0.0.1/health".into(),
            interval_secs: 5,
            timeout_secs: 0,
            start_period_secs: 0,
            max_failures: 0,
        };
        assert!(!trivial_exec_health_probe(Some(&real)));
        assert!(!trivial_exec_health_probe(None));
        assert!(!trivial_exec_health_probe(Some(&HealthCheck::Tcp {
            host: "127.0.0.1".into(),
            port: 1,
            interval_secs: 5,
            timeout_secs: 0,
            start_period_secs: 0,
            max_failures: 0,
        })));
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

    #[test]
    fn parse_toml_stack_with_health_check() {
        let toml = r#"
[[services]]
name = "web"
command = "/usr/bin/python3"
args = ["-m", "http.server", "8080"]
env = { PORT = "8080", DEBUG = "1" }
autorestart = "true"
exitcodes = [0, 2]
health_check = { type = "http", url = "http://127.0.0.1:8080/health", interval_secs = 5 }

[[services]]
name = "worker"
command = "/bin/worker"
autorestart = "false"
"#;
        let stack = parse_stack_from_str(toml, Path::new("conf/conf.d/stack.toml")).unwrap();
        assert_eq!(stack.services.len(), 2);
        assert!(!stack.prune);

        let web = &stack.services[0];
        assert_eq!(web.name.as_deref(), Some("web"));
        assert_eq!(web.args, vec!["-m", "http.server", "8080"]);
        assert_eq!(web.env.get("PORT").map(String::as_str), Some("8080"));
        assert_eq!(web.autorestart, crate::AutorestartPolicy::True);
        assert_eq!(web.exitcodes, vec![0, 2]);
        match &web.health_check {
            Some(HealthCheck::Http {
                url, interval_secs, ..
            }) => {
                assert_eq!(url, "http://127.0.0.1:8080/health");
                assert_eq!(*interval_secs, 5);
            }
            other => panic!("unexpected health check: {other:?}"),
        }
        assert_eq!(
            stack.services[1].autorestart,
            crate::AutorestartPolicy::False
        );
    }

    #[test]
    fn parse_toml_stack_via_inline_table() {
        let toml = r#"
[[services]]
name = "db"
command = "/usr/bin/postgres"
health_check = { type = "tcp", host = "127.0.0.1", port = 5432 }
"#;
        let stack = parse_stack_from_str(toml, Path::new("db.toml")).unwrap();
        match &stack.services[0].health_check {
            Some(HealthCheck::Tcp { host, port, .. }) => {
                assert_eq!(host, "127.0.0.1");
                assert_eq!(*port, 5432);
            }
            other => panic!("unexpected health check: {other:?}"),
        }
    }

    #[test]
    fn parse_json_stack_still_supported() {
        let json = r#"{
            "prune": true,
            "services": [
                {"name": "web", "command": "/bin/true", "autorestart": "unexpected"}
            ]
        }"#;
        let stack = parse_stack_from_str(json, Path::new("conf/conf.d/legacy.json")).unwrap();
        assert!(stack.prune);
        assert_eq!(stack.services.len(), 1);
        assert_eq!(
            stack.services[0].autorestart,
            crate::AutorestartPolicy::Unexpected
        );
    }

    #[test]
    fn parse_no_extension_defaults_to_toml() {
        let toml = r#"
[[services]]
name = "cron"
command = "/bin/echo"
"#;
        let stack = parse_stack_from_str(toml, Path::new("conf/conf.d/stack")).unwrap();
        assert_eq!(stack.services.len(), 1);
        assert_eq!(stack.services[0].name.as_deref(), Some("cron"));
    }

    #[test]
    fn example_stack_all_toml_parses() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../example/resource/stack_all.toml");
        let content = std::fs::read_to_string(&path).unwrap();
        let stack = parse_stack_from_str(&content, &path).unwrap();
        assert_eq!(stack.services.len(), 4, "{stack:?}");

        let api = stack
            .services
            .iter()
            .find(|s| s.name.as_deref() == Some("web-api"))
            .expect("web-api present");
        assert!(matches!(api.health_check, Some(HealthCheck::Http { .. })));
        assert!(api.hooks.pre_start.is_some(), "hooks pre_start present");
        assert_eq!(api.depends_on, vec!["sys-db"]);
    }

    #[test]
    fn stack_toml_roundtrip_through_export() {
        // Mirrors `super export --format toml`: serialize to TOML, then confirm
        // the output re-parses identically (toml emits nested tables for
        // internally-tagged enums, which the parser accepts).
        let stack = StackApplyRequest {
            services: vec![CreateProgramRequest {
                name: Some("web".into()),
                command: "/bin/true".into(),
                env: HashMap::from([("PORT".into(), "8080".into())]),
                health_check: Some(HealthCheck::Http {
                    url: "http://127.0.0.1:8080/health".into(),
                    method: Some("GET".into()),
                    interval_secs: 5,
                    timeout_secs: 0,
                    start_period_secs: 0,
                    max_failures: 0,
                }),
                ..Default::default()
            }],
            prune: true,
        };
        let s = toml::to_string_pretty(&stack).unwrap();
        let parsed = parse_stack_from_str(&s, Path::new("exported.toml")).unwrap();
        assert_eq!(parsed.prune, stack.prune);
        let a = &parsed.services[0];
        let b = &stack.services[0];
        assert_eq!(a.name, b.name);
        assert_eq!(a.command, b.command);
        assert_eq!(a.env, b.env);
        match (&a.health_check, &b.health_check) {
            (
                Some(HealthCheck::Http {
                    url: au,
                    method: am,
                    interval_secs: ai,
                    ..
                }),
                Some(HealthCheck::Http {
                    url: bu,
                    method: bm,
                    interval_secs: bi,
                    ..
                }),
            ) => {
                assert_eq!(au, bu);
                assert_eq!(am, bm);
                assert_eq!(ai, bi);
            }
            other => panic!("health check mismatch after round-trip: {other:?}"),
        }
    }

    #[test]
    fn parse_toml_stack_via_nested_table() {
        // Single nested tables are accepted for internally-tagged enums
        // (only [[array.of.tables]] is rejected).
        let toml = r#"
[[services]]
name = "db"
command = "/usr/bin/postgres"

[services.health_check]
type = "tcp"
host = "127.0.0.1"
port = 5432
"#;
        let stack = parse_stack_from_str(toml, Path::new("db.toml")).unwrap();
        match &stack.services[0].health_check {
            Some(HealthCheck::Tcp { host, port, .. }) => {
                assert_eq!(host, "127.0.0.1");
                assert_eq!(*port, 5432);
            }
            other => panic!("unexpected health check: {other:?}"),
        }
    }

    #[test]
    fn toml_parse_error_carries_location() {
        let bad = "[[services]]\nname = \"web\"\ncommand = [1, 2]\n";
        let err = parse_stack_from_str(bad, Path::new("conf/conf.d/stack.toml")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.starts_with("conf/conf.d/stack.toml:"), "{msg}");
        assert!(msg.contains("expected"), "{msg}");
    }
}
