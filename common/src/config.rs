use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn default_star() -> Vec<String> {
    vec!["*".to_string()]
}

fn default_hook_timeout() -> u64 {
    30
}

/// OSS: global event hook (Supervisor `[eventlistener]` equivalent).
/// Runs a local command with JSON on stdin when a system event fires.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventHookConfig {
    pub command: String,
    #[serde(default = "default_star")]
    pub events: Vec<String>,
    #[serde(default = "default_star")]
    pub programs: Vec<String>,
    #[serde(default = "default_true")]
    pub r#async: bool,
    #[serde(default = "default_hook_timeout")]
    pub timeout_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Signed subscription key in `conf/super.toml` (`[license] key = "..."`).
#[derive(Debug, Deserialize, Clone, Default)]
pub struct LicenseSection {
    pub key: Option<String>,
    /// When true, invalid or incompatible license keys refuse startup (no OSS fallback).
    #[serde(default)]
    pub strict: bool,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ServerConfig {
    /// Root API secret for the security plugin (subscription).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_secret: Option<String>,

    #[serde(default)]
    pub license: Option<LicenseSection>,

    #[serde(default)]
    pub server: ServerSection,
    #[serde(default)]
    pub storage: StorageSection,
    #[serde(default)]
    pub logging: LoggingSection,
    // Child process log settings
    #[serde(default)]
    pub child_logging: ChildLoggingSection,

    #[serde(default)]
    pub include: IncludeSection,

    /// Global event hooks: run shell commands on system events (JSON on stdin).
    #[serde(default)]
    pub event_hooks: Vec<EventHookConfig>,
}

/// Detect a legacy `[webhook]` table in raw `super.toml` content (removed from schema).
pub fn legacy_webhook_section_present(content: &str) -> bool {
    content.lines().any(|line| {
        let header = line.split('#').next().unwrap_or("").trim();
        header == "[webhook]"
    })
}

/// User-facing message when `[webhook]` is still present in `super.toml`.
pub const LEGACY_WEBHOOK_SECTION_MSG: &str = "[webhook] in super.toml is not supported — use [[event_hooks]] for local scripts or conf/notify.toml with the notify plugin for HTTP/IM alerts";

#[derive(Debug, Deserialize, Clone, Default)]
pub struct IncludeSection {
    // Glob patterns, e.g. ["/etc/super/conf.d/*.json"]
    pub files: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerSection {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout: u64,

    #[serde(default = "default_download_timeout")]
    pub download_timeout: u64,

    // Flapping detection
    // Window length in seconds (default 60)
    #[serde(default = "default_flapping_window")]
    pub flapping_window: u64,

    // Max restarts allowed within the window (default 5)
    #[serde(default = "default_flapping_threshold")]
    pub flapping_threshold: usize,

    // Enable API docs
    #[serde(default = "default_enable_docs")] // dev: true, production: false
    pub enable_docs: bool,

    /// Allow binding to a non-loopback address without the security plugin (OSS only).
    #[serde(default)]
    pub allow_insecure_public_bind: bool,

    /// Self-daemonize after startup (Unix only). Default false — use foreground under
    /// systemd/Docker. Conflicts with systemd service environments.
    #[serde(default)]
    pub daemon: bool,

    /// Optional pidfile path. Relative paths resolve under `SUPER_ROOT`.
    /// When daemonizing and unset, defaults to `run/superd.pid`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pidfile: Option<PathBuf>,
}

// Manual Default impl using helper functions above
impl Default for ServerSection {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            shutdown_timeout: default_shutdown_timeout(),
            download_timeout: default_download_timeout(),
            flapping_window: default_flapping_window(),
            flapping_threshold: default_flapping_threshold(),
            enable_docs: default_enable_docs(),
            allow_insecure_public_bind: false,
            daemon: false,
            pidfile: None,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct StorageSection {
    #[serde(default = "default_data_file")]
    pub data_file: PathBuf,
    #[serde(default = "default_log_dir")]
    pub log_dir: PathBuf,
}

impl Default for StorageSection {
    fn default() -> Self {
        Self {
            data_file: default_data_file(),
            log_dir: default_log_dir(),
        }
    }
}

// Log driver enum
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum LogDriver {
    #[default]
    File,
    Stdout,
}

// Default File for backward compatibility

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingSection {
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_log_max_mb")]
    pub log_max_mb: u64,
    #[serde(default = "default_log_backups")]
    pub log_backups: u32,
}

impl Default for LoggingSection {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            log_max_mb: default_log_max_mb(),
            log_backups: default_log_backups(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ChildLoggingSection {
    // Child log driver: file or stdout
    #[serde(default)]
    pub driver: LogDriver,

    #[serde(default = "default_child_max_mb")]
    pub max_size_mb: u64,
    #[serde(default = "default_child_backups")]
    pub max_backups: u32,

    // Max single-line log length (KB), default 16KB
    #[serde(default = "default_child_line_max_kb")]
    pub max_line_size_kb: u64,
}

impl Default for ChildLoggingSection {
    fn default() -> Self {
        Self {
            driver: LogDriver::default(),
            max_size_mb: default_child_max_mb(),
            max_backups: default_child_backups(),
            max_line_size_kb: default_child_line_max_kb(),
        }
    }
}

// Defaults helper functions
fn default_host() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    9002
}
fn default_shutdown_timeout() -> u64 {
    10
}
fn default_download_timeout() -> u64 {
    86400
} // default download timeout: 24 hours
fn default_data_file() -> PathBuf {
    "./data/snapshot.json".into()
}
fn default_log_dir() -> PathBuf {
    "./logs".into()
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_log_max_mb() -> u64 {
    50
}
fn default_log_backups() -> u32 {
    5
}
fn default_child_max_mb() -> u64 {
    10
} // default 10MB
fn default_child_backups() -> u32 {
    5
} // default 5 backups
fn default_child_line_max_kb() -> u64 {
    16
} // default 16KB per line
fn default_flapping_window() -> u64 {
    60
}
fn default_flapping_threshold() -> usize {
    5
}
fn default_enable_docs() -> bool {
    false
} // OSS default off; enable for onboarding

/// Read `auth_secret` from `conf/super.toml`.
pub fn read_auth_secret(config_path: &Path) -> anyhow::Result<Option<String>> {
    if !config_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(config_path)?;
    let config: ServerConfig = toml::from_str(&content)?;
    Ok(config
        .auth_secret
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty()))
}

/// Read `[license].key` from `conf/super.toml`.
pub fn read_license_key(config_path: &Path) -> anyhow::Result<Option<String>> {
    if !config_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(config_path)?;
    let value: toml::Value = toml::from_str(&content)?;
    Ok(value
        .get("license")
        .and_then(|v| v.get("key"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string))
}

/// Resolve license key: `SUPER_LICENSE` env overrides `[license].key` in super.toml.
pub fn resolve_license_key(config_path: &Path) -> anyhow::Result<Option<String>> {
    if let Ok(env_key) = std::env::var("SUPER_LICENSE") {
        let trimmed = env_key.trim();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed.to_string()));
        }
    }
    read_license_key(config_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_webhook_section_detection() {
        assert!(legacy_webhook_section_present(
            "[webhook]\nurl = \"http://x\"\n"
        ));
        assert!(!legacy_webhook_section_present(
            "[[event_hooks]]\ncommand = \"./h.sh\"\n"
        ));
        assert!(!legacy_webhook_section_present(
            "# [webhook] legacy example\n"
        ));
    }
}
