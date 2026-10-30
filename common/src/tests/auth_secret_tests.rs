use super::*;

fn tmp_root(tag: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("super-auth-secret-{tag}-"))
        .tempdir()
        .unwrap()
}

#[test]
fn detects_shipped_placeholders() {
    assert!(is_placeholder_auth_secret("CHANGE-ME-BEFORE-EXPOSE"));
    assert!(is_placeholder_auth_secret("CHANGE-ME-strong-random-secret"));
    assert!(is_placeholder_auth_secret("change-me-anything"));
    assert!(is_placeholder_auth_secret("  Change-Me-Please  "));
    assert!(!is_placeholder_auth_secret(""));
    assert!(!is_placeholder_auth_secret("   "));
    assert!(!is_placeholder_auth_secret("correct-horse-battery"));
    assert!(!is_placeholder_auth_secret("change"));
}

#[test]
fn ephemeral_secret_created_once_with_mode_600() {
    let root = tmp_root("create");
    let path = ephemeral_auth_secret_path(root.path());

    let first = load_or_generate_ephemeral_auth_secret(root.path()).unwrap();
    assert_eq!(first.len(), 48, "24 random bytes as hex");
    assert!(first.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(path.is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    // Second call returns the same secret (stable across restarts).
    let second = load_or_generate_ephemeral_auth_secret(root.path()).unwrap();
    assert_eq!(first, second);
}

#[test]
fn empty_existing_file_is_rewritten() {
    let root = tmp_root("empty");
    let path = ephemeral_auth_secret_path(root.path());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "  \n").unwrap();

    let secret = load_or_generate_ephemeral_auth_secret(root.path()).unwrap();
    assert_eq!(secret.len(), 48);
    assert_eq!(std::fs::read_to_string(&path).unwrap().trim(), secret);
}

#[test]
fn none_config_stays_none() {
    let root = tmp_root("none");
    let resolved = effective_auth_secret(None, root.path(), "0.0.0.0", false).unwrap();
    assert_eq!(resolved, EffectiveAuthSecret::None);
    assert!(resolved.as_secret().is_none());
    assert!(!ephemeral_auth_secret_path(root.path()).exists());
}

#[test]
fn real_secret_wins_over_placeholder_logic() {
    let root = tmp_root("real");
    let resolved = effective_auth_secret(
        Some("  my-real-long-secret  "),
        root.path(),
        "0.0.0.0",
        false,
    )
    .unwrap();
    assert_eq!(
        resolved,
        EffectiveAuthSecret::Configured("my-real-long-secret".into())
    );
    assert!(!ephemeral_auth_secret_path(root.path()).exists());
}

#[test]
fn placeholder_kept_on_loopback_and_socket_only() {
    let root = tmp_root("loopback");
    let resolved = effective_auth_secret(
        Some("CHANGE-ME-BEFORE-EXPOSE"),
        root.path(),
        "127.0.0.1",
        false,
    )
    .unwrap();
    assert_eq!(
        resolved,
        EffectiveAuthSecret::PlaceholderLocal("CHANGE-ME-BEFORE-EXPOSE".into())
    );
    assert!(!ephemeral_auth_secret_path(root.path()).exists());

    let root = tmp_root("socket-only");
    let resolved = effective_auth_secret(
        Some("CHANGE-ME-BEFORE-EXPOSE"),
        root.path(),
        "0.0.0.0",
        true,
    )
    .unwrap();
    assert_eq!(
        resolved,
        EffectiveAuthSecret::PlaceholderLocal("CHANGE-ME-BEFORE-EXPOSE".into())
    );
    assert!(!ephemeral_auth_secret_path(root.path()).exists());
}

#[test]
fn placeholder_on_non_loopback_generates_ephemeral_secret() {
    let root = tmp_root("generate");
    let resolved = effective_auth_secret(
        Some("CHANGE-ME-BEFORE-EXPOSE"),
        root.path(),
        "0.0.0.0",
        false,
    )
    .unwrap();
    let EffectiveAuthSecret::Ephemeral(secret) = resolved else {
        panic!("expected ephemeral secret, got {resolved:?}");
    };
    assert_ne!(secret, "CHANGE-ME-BEFORE-EXPOSE");
    assert_eq!(secret.len(), 48);
    assert_eq!(
        std::fs::read_to_string(ephemeral_auth_secret_path(root.path()))
            .unwrap()
            .trim(),
        secret
    );
}
