use super::*;

#[test]
fn legacy_top_level_auth_secret_detection() {
    assert!(legacy_top_level_auth_secret_present(
        "auth_secret = \"x\"\n[server]\nhost = \"127.0.0.1\"\n"
    ));
    assert!(!legacy_top_level_auth_secret_present(
        "[server]\nauth_secret = \"x\"\nhost = \"127.0.0.1\"\n"
    ));
    assert!(!legacy_top_level_auth_secret_present(
        "# auth_secret = \"x\"\n[server]\nhost = \"127.0.0.1\"\n"
    ));
}

#[test]
fn auth_secret_under_server_parses() {
    let cfg: ServerConfig = toml::from_str(
        r#"
            [server]
            auth_secret = "s3cret"
            host = "127.0.0.1"
            "#,
    )
    .unwrap();
    assert_eq!(cfg.server.auth_secret.as_deref(), Some("s3cret"));
}

#[test]
fn stale_ota_section_in_super_toml_is_ignored() {
    // No compatibility: leftover `[ota]` is not part of ServerConfig and
    // must not affect parsing (serde ignores unknown tables by default).
    let cfg: ServerConfig = toml::from_str(
        r#"
            [server]
            host = "127.0.0.1"
            port = 9002

            [ota]
            download_timeout = 1
            verify_timeout = 1
            "#,
    )
    .unwrap();
    assert_eq!(cfg.server.host, "127.0.0.1");
    assert_eq!(cfg.server.port, 9002);
}
