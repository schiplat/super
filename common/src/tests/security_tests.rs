use super::*;

#[test]
fn secrets_equal_is_length_sensitive() {
    assert!(secrets_equal("abc", "abc"));
    assert!(!secrets_equal("abc", "abd"));
    assert!(!secrets_equal("abc", "ab"));
}

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
        validate_outbound_url("http://127.0.0.1:8080/health", FetchUrlPolicy::HealthCheck).is_ok()
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
