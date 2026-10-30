//! Resolution of the effective `[server].auth_secret` at startup.
//!
//! Shipped defaults (Docker image, examples) carry a `CHANGE-ME-…` placeholder
//! so the daemon can start out of the box. A placeholder is public knowledge
//! and must never authenticate anything on a non-loopback TCP bind. On such
//! binds the effective secret becomes a generated ephemeral value, persisted
//! at `data/auth_secret.generated` (mode 0600, stable across restarts). On
//! loopback / socket-only runtimes the placeholder is kept (local exposure
//! only). This module does not log — callers report
//! [`EffectiveAuthSecret::PlaceholderLocal`] / [`EffectiveAuthSecret::Ephemeral`]
//! through their own subscriber.
//!
//! `superd` bootstrap and the `security` plugin both resolve through
//! [`effective_auth_secret`] (directly or via [`resolve_from_config`]) so the
//! core auth gate and the plugin always agree on the same value.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use rand::RngCore;

/// Prefix shared by every shipped placeholder secret (`CHANGE-ME-…`).
const PLACEHOLDER_PREFIX: &str = "change-me";

/// Outcome of auth-secret resolution. `core` maps this onto
/// `[server].auth_secret` before any consumer (auth middleware, security
/// plugin init payload, license validation) reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectiveAuthSecret {
    /// No auth configured (superd refuses non-loopback TCP binds).
    None,
    /// Real configured secret, used as-is.
    Configured(String),
    /// Shipped placeholder kept for a loopback / socket-only runtime.
    PlaceholderLocal(String),
    /// Shipped placeholder replaced by a generated, persisted ephemeral
    /// secret (non-loopback TCP bind).
    Ephemeral(String),
}

impl EffectiveAuthSecret {
    /// The string that authenticates requests, if any.
    pub fn as_secret(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Configured(s) | Self::PlaceholderLocal(s) | Self::Ephemeral(s) => Some(s),
        }
    }

    pub fn into_secret(self) -> Option<String> {
        self.as_secret().map(str::to_string)
    }
}

/// True when `secret` is one of the shipped placeholders (`CHANGE-ME-…`,
/// case-insensitive). Real secrets never start with that prefix.
pub fn is_placeholder_auth_secret(secret: &str) -> bool {
    let s = secret.trim();
    !s.is_empty() && s.to_ascii_lowercase().starts_with(PLACEHOLDER_PREFIX)
}

/// Path of the persisted ephemeral secret, under SUPER_ROOT.
pub fn ephemeral_auth_secret_path(super_root: &Path) -> PathBuf {
    super_root.join("data").join("auth_secret.generated")
}

fn generate_secret() -> String {
    let mut bytes = [0u8; 24];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Load the persisted ephemeral secret, creating it on first use.
///
/// The file is created with `create_new` and mode 0600 so the secret never
/// touches disk world-readable, even briefly. An existing empty/corrupt file
/// is rewritten in place.
pub fn load_or_generate_ephemeral_auth_secret(super_root: &Path) -> anyhow::Result<String> {
    let path = ephemeral_auth_secret_path(super_root);
    if path.exists() {
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
        let secret = generate_secret();
        std::fs::write(&path, &secret)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        return Ok(secret);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let secret = generate_secret();
    let mut file = OpenOptions::new().write(true).create_new(true).open(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(secret.as_bytes())?;
    file.sync_all()?;
    Ok(secret)
}

/// Resolve the effective auth secret for this runtime (see module docs).
pub fn effective_auth_secret(
    config_secret: Option<&str>,
    super_root: &Path,
    host: &str,
    socket_only: bool,
) -> anyhow::Result<EffectiveAuthSecret> {
    let Some(secret) = config_secret.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(EffectiveAuthSecret::None);
    };
    if !is_placeholder_auth_secret(secret) {
        return Ok(EffectiveAuthSecret::Configured(secret.to_string()));
    }

    let tcp_exposed = !socket_only && !crate::security::is_loopback_bind_host(host);
    if !tcp_exposed {
        return Ok(EffectiveAuthSecret::PlaceholderLocal(secret.to_string()));
    }
    Ok(EffectiveAuthSecret::Ephemeral(
        load_or_generate_ephemeral_auth_secret(super_root)?,
    ))
}

/// File-based entry point for callers that only hold SUPER_ROOT (the security
/// plugin): parses `[server]` from `config_path`, then applies
/// [`effective_auth_secret`]. superd bootstrap resolves from its in-memory
/// config with the same inputs, so both always agree.
pub fn resolve_from_config(
    config_path: &Path,
    super_root: &Path,
) -> anyhow::Result<EffectiveAuthSecret> {
    let content = std::fs::read_to_string(config_path)?;
    let cfg: crate::config::ServerConfig = toml::from_str(&content)?;
    effective_auth_secret(
        cfg.server.auth_secret.as_deref(),
        super_root,
        &cfg.server.host,
        cfg.server.socket_only,
    )
}

#[cfg(test)]
#[path = "tests/auth_secret_tests.rs"]
mod tests;
