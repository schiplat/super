//! Shared security helpers (env masking, outbound URL policy, plugin id format).

use anyhow::{Context, bail};
use std::collections::HashMap;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};

/// Maximum accepted Base64 license string length.
pub const MAX_LICENSE_B64_LEN: usize = 64 * 1024;

/// Maximum decoded license JSON payload size.
pub const MAX_LICENSE_JSON_LEN: usize = 64 * 1024;

/// Mask inline env values that look like secrets (API/CLI display).
pub fn mask_secret_value(key: &str, value: &str) -> String {
    let k = key.to_ascii_uppercase();
    if k.contains("SECRET")
        || k.contains("PASSWORD")
        || k.contains("TOKEN")
        || k.contains("KEY")
        || k.contains("CREDENTIAL")
    {
        "********".to_string()
    } else {
        value.to_string()
    }
}

/// Return a copy of `env` with sensitive values masked.
pub fn mask_env_map(env: &HashMap<String, String>) -> HashMap<String, String> {
    env.iter()
        .map(|(k, v)| (k.clone(), mask_secret_value(k, v)))
        .collect()
}

/// Plugin library stem allowed in signed license claims and on disk.
pub fn is_valid_plugin_id(id: &str) -> bool {
    let Some(first) = id.chars().next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// Reject grant ids that do not match the public catalog naming rules.
pub fn validate_license_grant_ids(grants: &[String]) -> anyhow::Result<()> {
    for id in grants {
        if !is_valid_plugin_id(id) {
            bail!("Invalid grant id in license: '{id}'");
        }
    }
    Ok(())
}

/// Policy for outbound HTTP(S) fetches initiated by superd.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchUrlPolicy {
    /// Health probes — loopback/LAN targets are expected.
    HealthCheck,
    /// OTA artifact downloads — HTTPS for remote hosts; loopback HTTP allowed for dev/tests.
    OtaArtifact,
}

/// Validate a URL before superd performs an outbound HTTP request.
pub fn validate_outbound_url(url: &str, policy: FetchUrlPolicy) -> anyhow::Result<()> {
    let url = url.trim();
    if url.is_empty() {
        bail!("URL must not be empty");
    }

    let (scheme, rest) = url
        .split_once("://")
        .context("URL must include a scheme (http:// or https://)")?;
    let scheme = scheme.to_ascii_lowercase();

    let host = rest
        .split(&['/', '?', '#'][..])
        .next()
        .and_then(|authority| authority.rsplit('@').next())
        .and_then(|hostport| hostport.split(':').next())
        .unwrap_or("")
        .trim_matches(|c| c == '[' || c == ']');

    if host.is_empty() {
        bail!("URL missing host");
    }

    if is_blocked_metadata_host(host) {
        bail!("URL host is not allowed: {host}");
    }

    let loopback = is_loopback_host(host);
    match (policy, scheme.as_str(), loopback) {
        (FetchUrlPolicy::OtaArtifact, "https", _) => {}
        (FetchUrlPolicy::OtaArtifact, "http", true) => {}
        (FetchUrlPolicy::HealthCheck, "http" | "https", _) => {}
        _ => bail!("URL scheme is not allowed for this operation"),
    }

    if policy == FetchUrlPolicy::OtaArtifact && !loopback && host_is_private_or_link_local(host)? {
        bail!("OTA artifact URL must not target private, link-local, or loopback addresses");
    }

    Ok(())
}

fn is_blocked_metadata_host(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    h == "metadata.google.internal"
        || h.ends_with(".metadata.google.internal")
        || h == "169.254.169.254"
}

fn is_loopback_host(host: &str) -> bool {
    let h = host.trim();
    if h.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if let Ok(ip) = h.parse::<IpAddr>() {
        return ip.is_loopback();
    }
    h.to_ascii_lowercase().ends_with(".localhost")
}

/// True when binding to this address keeps the API on loopback only.
pub fn is_loopback_bind_host(host: &str) -> bool {
    is_loopback_host(host)
}

/// Resolve a custom program log path and confine it under `log_dir`.
pub fn resolve_confined_log_path(log_dir: &Path, custom: &str) -> anyhow::Result<PathBuf> {
    let custom = custom.trim();
    if custom.is_empty() {
        bail!("custom log path must not be empty");
    }
    if custom.contains('\0') {
        bail!("invalid log path");
    }

    let canonical_log_dir = canonicalize_or_create_dir(log_dir)?;

    let raw = Path::new(custom);
    let resolved = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        canonical_log_dir.join(raw)
    };

    let canonical = if resolved.exists() {
        std::fs::canonicalize(&resolved).context("canonicalize log path")?
    } else {
        let file_name = resolved
            .file_name()
            .context("log path must include a file name")?;
        let parent = resolved
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(&canonical_log_dir);
        let canonical_parent = if parent.exists() {
            std::fs::canonicalize(parent).context("canonicalize log path parent")?
        } else {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create log path parent {}", parent.display()))?;
            std::fs::canonicalize(parent).context("canonicalize log path parent")?
        };
        canonical_parent.join(file_name)
    };

    if !canonical.starts_with(&canonical_log_dir) {
        bail!(
            "custom log path must be under log_dir ({})",
            canonical_log_dir.display()
        );
    }
    Ok(canonical)
}

fn canonicalize_or_create_dir(dir: &Path) -> anyhow::Result<PathBuf> {
    if dir.exists() {
        std::fs::canonicalize(dir)
            .with_context(|| format!("canonicalize log_dir {}", dir.display()))
    } else {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("create log_dir {}", dir.display()))?;
        std::fs::canonicalize(dir)
            .with_context(|| format!("canonicalize log_dir {}", dir.display()))
    }
}

fn host_is_private_or_link_local(host: &str) -> anyhow::Result<bool> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(is_non_public_ip(&ip));
    }

    // Best-effort: resolve hostname once; ignore resolution failures (handled at connect time).
    if let Ok(mut addrs) = (host, 0).to_socket_addrs()
        && let Some(addr) = addrs.next()
    {
        return Ok(is_non_public_ip(&addr.ip()));
    }
    Ok(false)
}

fn is_non_public_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private() || v4.is_link_local() || v4.is_loopback() || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xfe00) == 0xfc00 // unique local
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // link-local
        }
    }
}

/// Resolve a plugin library path and ensure it stays under `plugins_dir`.
pub fn resolve_plugin_library(plugins_dir: &Path, id: &str) -> Option<PathBuf> {
    if !is_valid_plugin_id(id) {
        return None;
    }

    #[cfg(target_os = "macos")]
    let extensions = ["dylib", "so"];
    #[cfg(not(target_os = "macos"))]
    let extensions = ["so", "dylib"];

    let canonical_dir = std::fs::canonicalize(plugins_dir).ok()?;

    for ext in extensions {
        let candidate = plugins_dir.join(format!("{id}.{ext}"));
        if !candidate.is_file() {
            continue;
        }
        let canonical_path = std::fs::canonicalize(&candidate).ok()?;
        if canonical_path.starts_with(&canonical_dir) {
            return Some(canonical_path);
        }
    }
    None
}

/// Default Unix socket permission mode: owner read/write only.
pub const DEFAULT_SOCKET_MODE: u32 = 0o600;

/// Parse and validate a Unix socket permission mode (octal string like `"0600"`).
///
/// Returns the raw mode bits. Empty input falls back to the 0600 default.
/// Refuses world-writable modes (`0o002` set): a world-writable socket would let
/// any local user connect to the control API, defeating the point of a Unix
/// socket transport. Group/other read bits are harmless (connecting a Unix
/// socket requires write permission).
pub fn parse_socket_mode(raw: &str) -> anyhow::Result<u32> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(DEFAULT_SOCKET_MODE);
    }
    let digits = raw.strip_prefix("0o").unwrap_or(raw);
    if digits.is_empty() || !digits.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        anyhow::bail!("socket_mode must be an octal string like \"0600\" (got {raw:?})");
    }
    if digits.len() > 4 {
        anyhow::bail!("socket_mode must be at most 4 octal digits (got {raw:?})");
    }
    let mode = u32::from_str_radix(digits, 8).map_err(|_| {
        anyhow::anyhow!("socket_mode must be an octal string like \"0600\" (got {raw:?})")
    })?;
    if mode > 0o777 {
        anyhow::bail!("socket_mode must be an octal mode between 000 and 0777 (got {raw:?})");
    }
    if mode == 0 {
        anyhow::bail!(
            "socket_mode must not be 000 — no local user would be able to connect; \
             use 0600 (owner only) or 0660/0640 to grant group access"
        );
    }
    if mode & 0o002 != 0 {
        anyhow::bail!(
            "socket_mode must not be world-writable (mode {raw:?} has the 'other' write bit set); \
             use 0600 (owner only) or 0660/0640 to grant group access"
        );
    }
    Ok(mode)
}

/// Reject UI asset paths with traversal or absolute segments.
pub fn sanitize_ui_asset_path(path: &str) -> Option<String> {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return Some("index.html".to_string());
    }
    if path.contains("..") || path.contains('\\') || path.contains('\0') {
        return None;
    }
    if path.split('/').any(|seg| seg.is_empty() || seg == ".") {
        return None;
    }
    Some(path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_secrets_in_env() {
        assert_eq!(mask_secret_value("DB_PASSWORD", "x"), "********");
        assert_eq!(mask_secret_value("PORT", "8080"), "8080");
    }

    #[test]
    fn plugin_id_rules() {
        assert!(is_valid_plugin_id("security"));
        assert!(is_valid_plugin_id("ui"));
        assert!(!is_valid_plugin_id("../ui"));
        assert!(!is_valid_plugin_id("Security"));
    }

    #[test]
    fn ota_blocks_metadata_and_http() {
        assert!(
            validate_outbound_url("http://cdn.example/app.tar.gz", FetchUrlPolicy::OtaArtifact)
                .is_err()
        );
        assert!(
            validate_outbound_url(
                "https://169.254.169.254/latest/meta-data",
                FetchUrlPolicy::OtaArtifact
            )
            .is_err()
        );
        assert!(
            validate_outbound_url(
                "https://releases.example.com/app.tar.gz",
                FetchUrlPolicy::OtaArtifact
            )
            .is_ok()
        );
    }

    #[test]
    fn health_allows_loopback() {
        assert!(
            validate_outbound_url("http://127.0.0.1:8080/health", FetchUrlPolicy::HealthCheck)
                .is_ok()
        );
    }

    #[test]
    fn ui_path_rejects_traversal() {
        assert!(sanitize_ui_asset_path("/../etc/passwd").is_none());
        assert_eq!(
            sanitize_ui_asset_path("assets/app.js").as_deref(),
            Some("assets/app.js")
        );
    }

    #[test]
    fn loopback_bind_hosts() {
        assert!(is_loopback_bind_host("127.0.0.1"));
        assert!(is_loopback_bind_host("localhost"));
        assert!(is_loopback_bind_host("::1"));
        assert!(!is_loopback_bind_host("0.0.0.0"));
        assert!(!is_loopback_bind_host("192.168.1.1"));
    }

    #[test]
    fn socket_mode_parsing() {
        assert_eq!(parse_socket_mode("").unwrap(), 0o600);
        assert_eq!(parse_socket_mode("0600").unwrap(), 0o600);
        assert_eq!(parse_socket_mode("0o660").unwrap(), 0o660);
        assert_eq!(parse_socket_mode("640").unwrap(), 0o640);
        assert!(
            parse_socket_mode("0666").is_err(),
            "world-writable must be refused"
        );
        assert!(parse_socket_mode("0662").is_err());
        assert!(parse_socket_mode("888").is_err());
        assert!(parse_socket_mode("abc").is_err());
        assert!(parse_socket_mode("06000").is_err());
        assert!(
            parse_socket_mode("0").is_err(),
            "empty-ish 0 is not an octal mode string"
        );
    }

    #[test]
    fn confined_log_path_stays_under_log_dir() {
        let dir = tempfile::tempdir().unwrap();
        let log_dir = dir.path().join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();

        let nested = log_dir.join("apps").join("out.log");
        std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
        std::fs::write(&nested, "").unwrap();

        let resolved = resolve_confined_log_path(&log_dir, "apps/out.log").expect("relative path");
        assert!(resolved.starts_with(std::fs::canonicalize(&log_dir).unwrap()));

        assert!(resolve_confined_log_path(&log_dir, "../outside.log").is_err());
        assert!(resolve_confined_log_path(&log_dir, "/etc/passwd").is_err());
    }
}
