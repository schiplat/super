//! Shared security helpers (env masking, outbound URL policy, plugin id format).

use anyhow::{Context, bail};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv6Addr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use subtle::ConstantTimeEq;

/// Maximum accepted Base64 license string length.
pub const MAX_LICENSE_B64_LEN: usize = 64 * 1024;

/// Maximum decoded license JSON payload size.
pub const MAX_LICENSE_JSON_LEN: usize = 64 * 1024;

/// Constant-time equality for Bearer secrets / token hashes.
///
/// Length mismatch returns `false` immediately (length is not secret for our
/// fixed-format hex hashes; for variable-length admin secrets it only leaks
/// length, not content).
pub fn secrets_equal(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

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
    /// Notification webhooks — http(s) allowed, but the daemon's own loopback
    /// API and cloud metadata endpoints are rejected so an Operator-level
    /// account cannot aim webhook delivery at superd itself or harvest cloud
    /// credentials. Private LAN ranges stay operator-configured (self-hosted
    /// gateways legitimately live there).
    Webhook,
}

/// Validate a URL before superd performs an outbound HTTP request.
pub fn validate_outbound_url(url: &str, policy: FetchUrlPolicy) -> anyhow::Result<()> {
    let url = url.trim();
    if url.is_empty() {
        bail!("URL must not be empty");
    }

    // Parse with the WHATWG URL parser — the same rules the HTTP client
    // applies when it connects — so host checks see the canonical address:
    // `2130706433`, `0x7f000001`, and `0177.0.0.1` all normalize to 127.0.0.1
    // here, and IPv6 literals arrive typed instead of hand-split strings.
    let parsed = url::Url::parse(url).context("URL must be a valid absolute http(s) URL")?;
    let scheme = parsed.scheme();
    let host = parsed.host().context("URL missing host")?;

    // Hostname denylist applies to every policy.
    if let url::Host::Domain(domain) = host
        && is_blocked_metadata_host(domain)
    {
        bail!("URL host is not allowed: {host}");
    }

    let loopback = host_is_loopback(&host);
    match (policy, scheme, loopback) {
        (FetchUrlPolicy::OtaArtifact, "https", _) => {}
        (FetchUrlPolicy::OtaArtifact, "http", true) => {}
        (FetchUrlPolicy::HealthCheck, "http" | "https", _) => {}
        (FetchUrlPolicy::Webhook, "http" | "https", _) => {}
        _ => bail!("URL scheme is not allowed for this operation"),
    }

    if policy == FetchUrlPolicy::OtaArtifact && !loopback && host_is_private_or_link_local(&host)? {
        bail!("OTA artifact URL must not target private, link-local, or loopback addresses");
    }

    if policy == FetchUrlPolicy::Webhook && (loopback || webhook_targets_blocked_address(&host)) {
        bail!("webhook URL must not target loopback or cloud metadata addresses (got {host})");
    }

    Ok(())
}

fn is_blocked_metadata_host(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    h == "metadata.google.internal"
        || h.ends_with(".metadata.google.internal")
        || h == "169.254.169.254"
}

/// Typed-host counterpart of [`is_loopback_host`] for parsed URLs: IPv6
/// literals (including IPv4-mapped spellings) are judged numerically.
fn host_is_loopback(host: &url::Host<&str>) -> bool {
    match host {
        url::Host::Ipv4(v4) => v4.is_loopback(),
        url::Host::Ipv6(v6) => {
            v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
        url::Host::Domain(d) => {
            let d = d.trim_end_matches('.');
            d.eq_ignore_ascii_case("localhost") || d.to_ascii_lowercase().ends_with(".localhost")
        }
    }
}

/// Alibaba Cloud metadata server (outside link-local).
const ALIBABA_METADATA_V4: std::net::Ipv4Addr = std::net::Ipv4Addr::new(100, 100, 100, 200);

/// GCP's IPv6 metadata server (`fd00:ec2::254`), the one metadata endpoint
/// that is neither loopback nor IPv4 link-local.
const GCP_METADATA_V6: Ipv6Addr = Ipv6Addr::new(0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254);

/// Webhook targets get a narrower denylist than OTA (private LAN stays
/// operator-configurable): loopback, unspecified (`0.0.0.0` reaches the local
/// host), IPv4/IPv6 link-local (169.254.0.0/16 carries AWS/GCP/Azure/DO
/// metadata and ECS task credentials), and the Alibaba/GCP metadata endpoints.
/// Hostnames are resolved best-effort so a name pointing at a blocked address
/// cannot slip past the literal checks; resolution failures are left to
/// connect time, as in the OTA check.
fn webhook_targets_blocked_address(host: &url::Host<&str>) -> bool {
    match host {
        url::Host::Ipv4(v4) => is_webhook_blocked_ip(&IpAddr::V4(*v4)),
        url::Host::Ipv6(v6) => is_webhook_blocked_ip(&IpAddr::V6(*v6)),
        url::Host::Domain(d) => match (*d, 0).to_socket_addrs() {
            Ok(addrs) => addrs.map(|a| a.ip()).any(|ip| is_webhook_blocked_ip(&ip)),
            Err(_) => false,
        },
    }
}

fn is_webhook_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            *v4 == ALIBABA_METADATA_V4
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // link-local
                || *v6 == GCP_METADATA_V6
                || v6.to_ipv4_mapped().is_some_and(|v4| {
                    v4.is_loopback() || v4.is_link_local() || v4.is_unspecified()
                })
        }
    }
}

/// True when binding to this address keeps the API on loopback only.
pub fn is_loopback_bind_host(host: &str) -> bool {
    is_loopback_host(host)
}

/// String-form loopback check for bind hosts and config values.
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

fn host_is_private_or_link_local(host: &url::Host<&str>) -> anyhow::Result<bool> {
    match host {
        url::Host::Ipv4(v4) => Ok(is_non_public_ip(&IpAddr::V4(*v4))),
        url::Host::Ipv6(v6) => Ok(is_non_public_ip(&IpAddr::V6(*v6))),
        // Best-effort: resolve hostname once; ignore resolution failures (handled at connect time).
        url::Host::Domain(d) => {
            if let Ok(mut addrs) = (*d, 0).to_socket_addrs()
                && let Some(addr) = addrs.next()
            {
                return Ok(is_non_public_ip(&addr.ip()));
            }
            Ok(false)
        }
    }
}

fn is_non_public_ip(ip: &IpAddr) -> bool {
    // IPv4-mapped v6 (::ffff:a.b.c.d) is checked as the v4 it really is.
    let v4 = match ip {
        IpAddr::V4(v4) => *v4,
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => v4,
            None => {
                return v6.is_loopback()
                    || v6.is_unspecified()
                    || (v6.segments()[0] & 0xfe00) == 0xfc00 // unique local
                    || (v6.segments()[0] & 0xffc0) == 0xfe80; // link-local
            }
        },
    };
    v4.is_private() || v4.is_link_local() || v4.is_loopback() || v4.is_unspecified()
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
#[path = "tests/security_tests.rs"]
mod tests;
