use super_core::config::ServerConfig;
// use common::ProgramConfig; // Fixed: Removed unused import

#[test]
fn test_default_config() {
    let toml_str = r#"
        [server]
        port = 8080
    "#;

    let config: ServerConfig = toml::from_str(toml_str).expect("Failed to parse");

    assert_eq!(config.server.port, 8080);
    assert_eq!(config.server.host, "127.0.0.1"); // Default host (localhost only, avoids accidentally exposing the API)
    assert!(!config.server.allow_insecure_public_bind);
    assert_eq!(config.logging.log_level, "info"); // Default log level
    assert_eq!(config.child_logging.max_size_mb, 10); // Default child process log size
}

#[test]
fn test_legacy_webhook_section_rejected_by_check_helper() {
    use common::config::legacy_webhook_section_present;
    assert!(legacy_webhook_section_present(
        "[webhook]\nurl = \"http://localhost:9999\"\n"
    ));
}

#[test]
fn test_auth_secret_config() {
    let toml_str = r#"
        auth_secret = "root-secret"

        [server]
        port = 9002
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.auth_secret.as_deref(), Some("root-secret"));
}

#[test]
fn test_license_config() {
    let toml_str = r#"
        [license]
        key = "test-key"

        [server]
        port = 9002
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(
        config.license.as_ref().unwrap().key.as_deref(),
        Some("test-key")
    );
}

#[test]
fn test_event_hooks_config() {
    let toml_str = r#"
        [[event_hooks]]
        id = "on-fatal"
        command = "/opt/hook.sh"
        events = ["process_fatal"]
        programs = ["web-server"]
        async = true
        timeout_secs = 15
    "#;
    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.event_hooks.len(), 1);
    let hook = &config.event_hooks[0];
    assert_eq!(hook.id.as_deref(), Some("on-fatal"));
    assert_eq!(hook.command, "/opt/hook.sh");
    assert_eq!(hook.events, vec!["process_fatal"]);
    assert_eq!(hook.programs, vec!["web-server"]);
    assert!(hook.r#async);
    assert_eq!(hook.timeout_secs, 15);
}
