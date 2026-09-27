use crate::plugin::loader::{PluginRuntime, load_authorized_plugins};
use anyhow::Context;
use common::config::resolve_license_key;
use common::license::{
    LicenseClaims, LicenseExpiryStatus, check_superd_version, license_help_footer,
    licensed_version_scope, scan_plugin_stems, verify_license_for_superd,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

const LICENSE_BANNER: &str = "\
======================================================================\n\
LICENSE ERROR: verification failed — running OSS mode only.\n\
No paid plugins will be loaded. Fix [license].key in conf/super.toml\n\
Run: super check   or   super doctor\n\
(Set [license].strict = true to refuse startup instead of degrading.)\n\
======================================================================";

const LICENSE_REFUSAL_BANNER: &str = "\
======================================================================\n\
LICENSE ERROR: verification failed — startup refused.\n\
Fix [license].key or remove licensed-only config (plugins/, auth_secret).\n\
Run: super check   or   super doctor\n\
======================================================================";

/// Stderr banner when strict mode or deployment intent blocks OSS fallback.
pub fn emit_license_refusal_stderr(reason: &str, strict: bool, intent: bool) {
    eprintln!("{LICENSE_REFUSAL_BANNER}");
    eprintln!("License error: {reason}");
    if strict {
        eprintln!("Cause: [license].strict or SUPER_LICENSE_STRICT is enabled.");
    }
    if intent {
        eprintln!(
            "Cause: licensed deployment signals (plugins on disk, auth_secret, or non-loopback bind)."
        );
    }
    eprintln!("{}", license_help_footer());
}

/// Log license degradation after tracing is initialized (superd bootstrap).
pub fn log_license_degradation(reason: &str) {
    error!("{}", LICENSE_BANNER);
    error!("License error: {}", reason);
    error!(
        "Paid plugins are disabled until the key verifies. {}",
        license_help_footer()
    );
}

fn emit_license_degradation_stderr(reason: &str) {
    eprintln!("{LICENSE_BANNER}");
    eprintln!("License error: {reason}");
    eprintln!("Hint: run `super check` or `super doctor`. Paid plugins will not load.");
    eprintln!("{}", license_help_footer());
}

/// Whether superd is running with paid plugins active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Oss,
    Licensed,
}

/// Result of reading and validating the subscription license.
#[derive(Debug, Clone)]
pub enum LicenseOutcome {
    /// No license configured.
    Missing,
    /// Signature valid and major version compatible.
    Valid(LicenseClaims),
    /// Bad signature, parse error, or major mismatch.
    Invalid { reason: String },
}

/// Startup snapshot after scanning license + `plugins/*.so`.
pub struct PluginHost {
    pub mode: RunMode,
    pub claims: Option<LicenseClaims>,
    /// Plugin IDs authorized by license (empty in OSS mode).
    pub licensed_plugins: Vec<String>,
    /// `.so` stems found on disk (all files, including unauthorized).
    pub installed_plugins: Vec<String>,
    /// Plugin IDs loaded successfully via dlopen.
    pub loaded_plugins: Vec<String>,
    pub runtime: PluginRuntime,
    pub plugins_dir: PathBuf,
    /// Set when a license key was configured but verification failed (OSS fallback unless strict/intent refuses).
    pub license_degraded_reason: Option<String>,
}

impl PluginHost {
    /// Discover plugins under `{root}/plugins/` and validate license in `conf/super.toml`.
    ///
    /// `superd_version` is checked against signed major/minor policy in the license claims.
    pub fn discover(root: &Path, superd_version: &str) -> Self {
        let plugins_dir = root.join("plugins");
        let config_file = root.join("conf").join("super.toml");

        let license_outcome = resolve_license(&config_file);
        let installed_plugins = scan_plugin_stems(&plugins_dir);

        match &license_outcome {
            LicenseOutcome::Missing => {
                info!("No license found; running OSS edition.");
                if !installed_plugins.is_empty() {
                    for id in &installed_plugins {
                        warn!("Plugin '{}.so' present but no license; skipped.", id);
                    }
                }
                Self {
                    mode: RunMode::Oss,
                    claims: None,
                    licensed_plugins: Vec::new(),
                    installed_plugins,
                    loaded_plugins: Vec::new(),
                    runtime: PluginRuntime::empty(),
                    plugins_dir,
                    license_degraded_reason: None,
                }
            }
            LicenseOutcome::Invalid { reason } => {
                emit_license_degradation_stderr(reason);
                error!("{}", LICENSE_BANNER);
                error!("License error: {}", reason);
                if !installed_plugins.is_empty() {
                    for id in &installed_plugins {
                        warn!("Plugin '{}.so' present but license invalid; skipped.", id);
                    }
                }
                Self {
                    mode: RunMode::Oss,
                    claims: None,
                    licensed_plugins: Vec::new(),
                    installed_plugins,
                    loaded_plugins: Vec::new(),
                    runtime: PluginRuntime::empty(),
                    plugins_dir,
                    license_degraded_reason: Some(reason.clone()),
                }
            }
            LicenseOutcome::Valid(claims) => {
                if let Err(reason) = check_superd_version(claims, superd_version) {
                    emit_license_degradation_stderr(&reason);
                    error!("{}", LICENSE_BANNER);
                    error!("License error: {}", reason);
                    if !installed_plugins.is_empty() {
                        for id in &installed_plugins {
                            warn!("Plugin '{}.so' present but license invalid; skipped.", id);
                        }
                    }
                    return Self {
                        mode: RunMode::Oss,
                        claims: None,
                        licensed_plugins: Vec::new(),
                        installed_plugins,
                        loaded_plugins: Vec::new(),
                        runtime: PluginRuntime::empty(),
                        plugins_dir,
                        license_degraded_reason: Some(reason),
                    };
                }

                info!(
                    "License verified for '{}' (superd {}, grants: {:?})",
                    claims.issued_to,
                    licensed_version_scope(claims),
                    claims.grants
                );

                let licensed_set: HashSet<&str> =
                    claims.grants.iter().map(String::as_str).collect();
                let installed_set: HashSet<&str> =
                    installed_plugins.iter().map(String::as_str).collect();

                for id in &installed_plugins {
                    if !licensed_set.contains(id.as_str()) {
                        warn!("Plugin '{}' present but not licensed; skipped.", id);
                    }
                }

                for id in &claims.grants {
                    if !installed_set.contains(id.as_str()) {
                        warn!(
                            "Plugin '{}' licensed but not installed; feature unavailable.",
                            id
                        );
                    }
                }

                let to_load: Vec<String> = installed_plugins
                    .iter()
                    .filter(|id| licensed_set.contains(id.as_str()))
                    .cloned()
                    .collect();

                let runtime = load_authorized_plugins(&plugins_dir, &to_load);

                Self {
                    mode: RunMode::Licensed,
                    claims: Some(claims.clone()),
                    licensed_plugins: claims.grants.clone(),
                    installed_plugins,
                    loaded_plugins: runtime.loaded_ids.clone(),
                    runtime,
                    plugins_dir,
                    license_degraded_reason: None,
                }
            }
        }
    }

    pub fn is_licensed(&self) -> bool {
        self.mode == RunMode::Licensed
    }

    pub fn has_loaded_plugins(&self) -> bool {
        !self.loaded_plugins.is_empty()
    }
}

/// Licensed mode requires the bundled `security` plugin and a configured root secret.
pub fn validate_licensed_security(
    mode: RunMode,
    claims: Option<&LicenseClaims>,
    loaded_plugins: &[String],
    installed_plugins: &[String],
    plugins_dir: &Path,
) -> anyhow::Result<()> {
    if mode != RunMode::Licensed {
        return Ok(());
    }

    let claims = claims.context("licensed mode requires license claims")?;

    if !claims.grants.iter().any(|p| p == "security") {
        anyhow::bail!(
            "Licensed deployment requires the security plugin in your subscription key. \
             Re-issue or renew your license — security is included with every subscription."
        );
    }

    if !loaded_plugins.iter().any(|p| p == "security") {
        if installed_plugins.iter().any(|p| p == "security") {
            anyhow::bail!(
                "security plugin is present under {} but failed to load. \
                 Check superd logs for dlopen errors.",
                plugins_dir.display()
            );
        }
        anyhow::bail!(
            "Licensed deployment requires security.so (or security.dylib) under {}. \
             The security plugin is included with every subscription.",
            plugins_dir.display()
        );
    }

    Ok(())
}

/// Require `auth_secret` once the security plugin is loaded for a licensed deployment.
pub fn validate_licensed_auth_secret(
    mode: RunMode,
    loaded_plugins: &[String],
    auth_secret: Option<&str>,
) -> anyhow::Result<()> {
    if mode != RunMode::Licensed {
        return Ok(());
    }
    if !loaded_plugins.iter().any(|p| p == "security") {
        return Ok(());
    }
    if auth_secret.is_some_and(|s| !s.trim().is_empty()) {
        return Ok(());
    }
    anyhow::bail!(
        "Licensed deployment requires [server].auth_secret in conf/super.toml for the security plugin."
    );
}

/// Refuse startup when license verification failed but strict or licensed intent applies.
pub fn enforce_license_degradation_policy(
    reason: &str,
    config: &common::config::ServerConfig,
    installed_plugins: &[String],
    config_path: &Path,
) -> anyhow::Result<()> {
    let strict = common::resolve_license_strict(config_path)?;
    let intent = common::licensed_deployment_intent(config, installed_plugins);
    if common::should_refuse_license_degradation(config, installed_plugins, strict) {
        emit_license_refusal_stderr(reason, strict, intent);
        anyhow::bail!(common::license_degradation_refusal_message(
            reason, strict, intent
        ));
    }
    Ok(())
}

fn resolve_license(config_file: &Path) -> LicenseOutcome {
    let key = match resolve_license_key(config_file) {
        Ok(k) => k,
        Err(e) => {
            return LicenseOutcome::Invalid {
                reason: format!("Cannot read license from {:?}: {}", config_file, e),
            };
        }
    };

    let Some(key) = key else {
        return LicenseOutcome::Missing;
    };

    match verify_license_for_superd(&key) {
        Ok((claims, expiry)) => {
            if expiry == LicenseExpiryStatus::Expired {
                warn!(
                    "License subscription expired; licensed plugins remain available offline. {}",
                    license_help_footer()
                );
            }
            LicenseOutcome::Valid(claims)
        }
        Err(e) => LicenseOutcome::Invalid {
            reason: e.to_string(),
        },
    }
}

#[cfg(test)]
#[path = "../tests/host_tests.rs"]
mod tests;
