use crate::plugin::loader::{PluginRuntime, load_authorized_plugins};
use anyhow::Context;
use common::config::resolve_license_key;
use common::license::{
    LICENSE_UPGRADE_URL, LicenseClaims, LicenseExpiryStatus, check_superd_version,
    licensed_version_scope, verify_license_for_superd,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

const LICENSE_BANNER: &str = "\
====================================================================\n\
LICENSE ERROR: verification failed. Running OSS mode only.\n\
No paid plugins will be loaded. Check [license].key in conf/super.toml\n\
====================================================================";

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
}

impl PluginHost {
    /// Discover plugins under `{root}/plugins/` and validate license in `conf/super.toml`.
    ///
    /// `superd_version` is checked against signed major/minor policy in the license claims.
    pub fn discover(root: &Path, superd_version: &str) -> Self {
        let plugins_dir = root.join("plugins");
        let config_file = root.join("conf").join("super.toml");

        let license_outcome = resolve_license(&config_file);
        let installed_plugins = scan_plugin_files(&plugins_dir);

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
                }
            }
            LicenseOutcome::Invalid { reason } => {
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
                }
            }
            LicenseOutcome::Valid(claims) => {
                if let Err(reason) = check_superd_version(claims, superd_version) {
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
        "Licensed deployment requires auth_secret in conf/super.toml for the security plugin."
    );
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
                    "License subscription expired; licensed plugins remain available offline. \
                     Renew for newer superd releases: {LICENSE_UPGRADE_URL}"
                );
            }
            LicenseOutcome::Valid(claims)
        }
        Err(e) => LicenseOutcome::Invalid {
            reason: e.to_string(),
        },
    }
}

/// List plugin library stems under `plugins/` (`.so` / `.dylib`).
fn scan_plugin_files(plugins_dir: &Path) -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn scan_finds_so_files() {
        let tmp = TempDir::new().unwrap();
        let plugins = tmp.path().join("plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::write(plugins.join("security.so"), b"fake").unwrap();
        std::fs::write(plugins.join("readme.txt"), b"x").unwrap();

        let ids = scan_plugin_files(&plugins);
        assert_eq!(ids, vec!["security"]);
    }

    #[test]
    fn scan_finds_dylib_files() {
        let tmp = TempDir::new().unwrap();
        let plugins = tmp.path().join("plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::write(plugins.join("isolation.dylib"), b"fake").unwrap();

        let ids = scan_plugin_files(&plugins);
        assert_eq!(ids, vec!["isolation"]);
    }

    #[test]
    fn missing_license_is_oss() {
        let tmp = TempDir::new().unwrap();
        let host = PluginHost::discover(tmp.path(), "1.1.9");
        assert_eq!(host.mode, RunMode::Oss);
        assert!(host.licensed_plugins.is_empty());
    }

    #[test]
    fn invalid_license_rejects_plugins() {
        let tmp = TempDir::new().unwrap();
        let conf = tmp.path().join("conf");
        std::fs::create_dir_all(&conf).unwrap();
        std::fs::write(
            conf.join("super.toml"),
            "[license]\nkey = \"not-a-license\"\n",
        )
        .unwrap();

        let plugins = tmp.path().join("plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::write(plugins.join("notify.so"), b"fake").unwrap();

        let host = PluginHost::discover(tmp.path(), "1.1.9");
        assert_eq!(host.mode, RunMode::Oss);
        assert!(host.loaded_plugins.is_empty());
    }

    #[test]
    fn licensed_requires_security_in_claims() {
        let claims = LicenseClaims {
            product_id: None,
            kid: None,
            issued_to: "acme".into(),
            issued_at: 0,
            major_version: 1,
            minor_version: None,
            max_super_minor: None,
            minor_ahead: None,
            issued_super_version: None,
            grants: vec!["ui".into()],
            expires_at: None,
            retain_grants_after_expiry: None,
            license_id: None,
        };
        let err = validate_licensed_security(
            RunMode::Licensed,
            Some(&claims),
            &[],
            &[],
            Path::new("/tmp/plugins"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("security plugin"));
    }

    #[test]
    fn licensed_requires_security_on_disk() {
        let claims = LicenseClaims {
            product_id: None,
            kid: None,
            issued_to: "acme".into(),
            issued_at: 0,
            major_version: 1,
            minor_version: None,
            max_super_minor: None,
            minor_ahead: None,
            issued_super_version: None,
            grants: vec!["security".into(), "ui".into()],
            expires_at: None,
            retain_grants_after_expiry: None,
            license_id: None,
        };
        let err = validate_licensed_security(
            RunMode::Licensed,
            Some(&claims),
            &[],
            &[],
            Path::new("/tmp/plugins"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("security.so"));
    }

    #[test]
    fn licensed_requires_auth_secret() {
        let err = validate_licensed_auth_secret(RunMode::Licensed, &["security".into()], None)
            .unwrap_err();
        assert!(err.to_string().contains("auth_secret"));
    }

    #[test]
    fn licensed_reports_security_dlopen_failure() {
        let claims = LicenseClaims {
            product_id: None,
            kid: None,
            issued_to: "acme".into(),
            issued_at: 0,
            major_version: 1,
            minor_version: None,
            max_super_minor: None,
            minor_ahead: None,
            issued_super_version: None,
            grants: vec!["security".into()],
            expires_at: None,
            retain_grants_after_expiry: None,
            license_id: None,
        };
        let err = validate_licensed_security(
            RunMode::Licensed,
            Some(&claims),
            &[],
            &["security".into()],
            Path::new("/tmp/plugins"),
        )
        .unwrap_err();
        assert!(err.to_string().contains("failed to load"));
    }

    #[test]
    fn oss_skips_licensed_security_checks() {
        validate_licensed_security(RunMode::Oss, None, &[], &[], Path::new(".")).unwrap();
        validate_licensed_auth_secret(RunMode::Oss, &[], None).unwrap();
    }
}
