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

    let ids = scan_plugin_stems(&plugins);
    assert_eq!(ids, vec!["security"]);
}

#[test]
fn scan_finds_dylib_files() {
    let tmp = TempDir::new().unwrap();
    let plugins = tmp.path().join("plugins");
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(plugins.join("isolation.dylib"), b"fake").unwrap();

    let ids = scan_plugin_stems(&plugins);
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
fn invalid_license_with_plugins_refuses_when_enforced() {
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
    let reason = host.license_degraded_reason.as_deref().unwrap();
    let config = common::config::ServerConfig::default();
    assert!(
        enforce_license_degradation_policy(
            reason,
            &config,
            &host.installed_plugins,
            &conf.join("super.toml"),
        )
        .is_err()
    );
}

#[test]
fn invalid_license_loopback_dev_allows_degrade() {
    let tmp = TempDir::new().unwrap();
    let conf = tmp.path().join("conf");
    std::fs::create_dir_all(&conf).unwrap();
    std::fs::write(
        conf.join("super.toml"),
        "[license]\nkey = \"not-a-license\"\n",
    )
    .unwrap();

    let host = PluginHost::discover(tmp.path(), "1.1.9");
    let reason = host.license_degraded_reason.as_deref().unwrap();
    let config = common::config::ServerConfig::default();
    enforce_license_degradation_policy(
        reason,
        &config,
        &host.installed_plugins,
        &conf.join("super.toml"),
    )
    .unwrap();
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
    let err =
        validate_licensed_auth_secret(RunMode::Licensed, &["security".into()], None).unwrap_err();
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
