use crate::config::ServerConfig;
use crate::is_loopback_bind_host;
use std::path::Path;

/// `SUPER_LICENSE_STRICT=1` / `true` / `yes` forces hard-fail when the key does not verify.
pub fn env_license_strict() -> bool {
    match std::env::var("SUPER_LICENSE_STRICT") {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

/// Read `[license].strict` from `conf/super.toml` (default `false`).
pub fn read_license_strict(config_path: &Path) -> anyhow::Result<bool> {
    if !config_path.exists() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(config_path)?;
    let value: toml::Value = toml::from_str(&content)?;
    Ok(value
        .get("license")
        .and_then(|v| v.get("strict"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false))
}

/// Effective strict flag: env overrides config file.
pub fn resolve_license_strict(config_path: &Path) -> anyhow::Result<bool> {
    if env_license_strict() {
        return Ok(true);
    }
    read_license_strict(config_path)
}

/// Plugin library stems under `plugins/` (`.so` / `.dylib`).
pub fn scan_plugin_stems(plugins_dir: &Path) -> Vec<String> {
    let mut ids = Vec::new();
    let entries = match std::fs::read_dir(plugins_dir) {
        Ok(e) => e,
        Err(_) => return ids,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let is_plugin_lib = path
            .extension()
            .is_some_and(|ext| ext == "so" || ext == "dylib");
        if is_plugin_lib && let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            ids.push(stem.to_string());
        }
    }

    ids.sort();
    ids.dedup();
    ids
}

/// Signals that the deployment expects a valid licensed startup (not casual OSS).
pub fn licensed_deployment_intent(config: &ServerConfig, installed_plugins: &[String]) -> bool {
    if !installed_plugins.is_empty() {
        return true;
    }
    if config
        .auth_secret
        .as_ref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        return true;
    }
    !is_loopback_bind_host(&config.server.host)
}

/// When true, superd must exit instead of degrading to OSS on license verification failure.
pub fn should_refuse_license_degradation(
    config: &ServerConfig,
    installed_plugins: &[String],
    strict: bool,
) -> bool {
    strict || licensed_deployment_intent(config, installed_plugins)
}

pub fn license_degradation_refusal_message(reason: &str, strict: bool, intent: bool) -> String {
    let mut triggers = Vec::new();
    if strict {
        triggers.push("[license].strict (or SUPER_LICENSE_STRICT) is enabled");
    }
    if intent {
        triggers.push(
            "licensed deployment signals detected (plugins on disk, auth_secret set, or non-loopback bind)",
        );
    }
    let trigger_line = triggers.join("; ");
    format!(
        "License verification failed and startup was refused ({trigger_line}). \
         Reason: {reason}. \
         Fix [license].key, renew your subscription, or remove licensed-only configuration \
         (plugins/, auth_secret, public bind) to run in OSS mode. \
         Run `super check` or `super doctor`. {}",
        crate::license::license_help_footer()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServerConfig;
    use tempfile::TempDir;

    #[test]
    fn intent_when_plugins_present() {
        let config = ServerConfig::default();
        assert!(licensed_deployment_intent(&config, &["security".into()]));
    }

    #[test]
    fn intent_when_auth_secret_set() {
        let config = ServerConfig {
            auth_secret: Some("secret".into()),
            ..Default::default()
        };
        assert!(licensed_deployment_intent(&config, &[]));
    }

    #[test]
    fn intent_when_public_bind() {
        let config = ServerConfig {
            server: crate::config::ServerSection {
                host: "0.0.0.0".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(licensed_deployment_intent(&config, &[]));
    }

    #[test]
    fn no_intent_on_loopback_oss() {
        let config = ServerConfig::default();
        assert!(!licensed_deployment_intent(&config, &[]));
    }

    #[test]
    fn strict_from_toml() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("super.toml");
        std::fs::write(&path, "[license]\nstrict = true\nkey = \"x\"\n").unwrap();
        assert!(read_license_strict(&path).unwrap());
    }

    #[test]
    fn scan_plugin_stems_finds_libs() {
        let tmp = TempDir::new().unwrap();
        let plugins = tmp.path().join("plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::write(plugins.join("security.so"), b"x").unwrap();
        assert_eq!(scan_plugin_stems(&plugins), vec!["security"]);
    }
}
