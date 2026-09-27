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
        server: crate::config::ServerSection {
            auth_secret: Some("secret".into()),
            ..Default::default()
        },
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
