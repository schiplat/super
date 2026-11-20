use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn write_mode(dir: &Path, name: &str, mode: u32) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, b"x").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    path
}

fn test_config(auth_secret: Option<&str>, socket: Option<&str>) -> ServerConfig {
    let mut toml = String::from("[server]\n");
    if let Some(secret) = auth_secret {
        toml.push_str(&format!("auth_secret = \"{secret}\"\n"));
    }
    if let Some(socket) = socket {
        toml.push_str(&format!("socket = \"{socket}\"\n"));
    }
    toml.push_str(
        "[storage]\ndata_file = \"./data/snapshot.json\"\nevents_file = \"./data/events.db\"\n",
    );
    toml::from_str(&toml).unwrap()
}

#[test]
fn owner_only_secret_is_clean() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_mode(tmp.path(), "secret.key", 0o600);
    assert_eq!(inspect(&path, PathKind::Secret).unwrap(), None);
}

#[test]
fn group_readable_secret_warns_with_chmod_hint() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_mode(tmp.path(), "secret.key", 0o640);
    let finding = inspect(&path, PathKind::Secret).unwrap().unwrap();
    assert_eq!(finding.mode, 0o640);
    assert_eq!(finding.severity, PermissionSeverity::Warning);
    assert!(finding.reason.contains("group-readable"), "{finding:?}");
    assert!(finding.hint.contains("chmod 600"), "{finding:?}");
}

#[test]
fn world_writable_secret_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_mode(tmp.path(), "secret.key", 0o666);
    let finding = inspect(&path, PathKind::Secret).unwrap().unwrap();
    assert_eq!(finding.severity, PermissionSeverity::Error);
    assert_eq!(finding.hint, format!("chmod 600 {}", path.display()));
}

#[test]
fn read_only_secret_keeps_its_restrictive_mode_in_the_hint() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_mode(tmp.path(), "secret.key", 0o444);
    let finding = inspect(&path, PathKind::Secret).unwrap().unwrap();
    assert!(finding.reason.contains("world-readable"), "{finding:?}");
    assert!(finding.hint.contains("chmod 400"), "{finding:?}");
}

#[test]
fn readable_executable_is_clean_but_group_writable_is_not() {
    let tmp = tempfile::tempdir().unwrap();
    let clean = write_mode(tmp.path(), "a.so", 0o755);
    assert_eq!(inspect(&clean, PathKind::Writable).unwrap(), None);

    let group = write_mode(tmp.path(), "b.so", 0o775);
    let finding = inspect(&group, PathKind::Writable).unwrap().unwrap();
    assert_eq!(finding.severity, PermissionSeverity::Warning);
    assert_eq!(finding.reason, "group-writable");
    assert!(finding.hint.contains("chmod 755"), "{finding:?}");
}

#[test]
fn world_writable_executable_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = write_mode(tmp.path(), "a.dylib", 0o777);
    let finding = inspect(&path, PathKind::Writable).unwrap().unwrap();
    assert_eq!(finding.severity, PermissionSeverity::Error);
    assert!(finding.reason.contains("world-writable"), "{finding:?}");
}

#[test]
fn missing_path_is_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("nope");
    assert_eq!(inspect(&path, PathKind::Secret).unwrap(), None);
}

#[test]
fn audit_instance_only_reports_paths_that_exist() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    // conf/super.toml holds a live secret and is world-readable.
    write_mode(root, "conf/super.toml", 0o644);
    // Snapshot is correctly owner-only.
    write_mode(root, "data/snapshot.json", 0o600);
    // Event store was created by an umask that left it group-readable.
    write_mode(root, "data/events.db", 0o640);
    // Forward-looking SSO paths: only the ones present are reported.
    write_mode(root, "data/auth.db", 0o644);
    // A plugin dropped into the deploy dir world-writable.
    write_mode(root, "plugins/security.dylib", 0o777);
    // Unix socket any local user can talk to.
    write_mode(root, "run/superd.sock", 0o777);

    let cfg = test_config(Some("s3cret"), Some("./run/superd.sock"));
    let audit = audit_instance(root, &cfg);

    let find = |name: &str| {
        audit
            .findings
            .iter()
            .find(|f| f.path.ends_with(name))
            .map(|f| (f.severity, f.reason.clone()))
    };

    assert_eq!(
        find("conf/super.toml").map(|f| f.0),
        Some(PermissionSeverity::Warning)
    );
    assert_eq!(
        find("events.db").map(|f| f.0),
        Some(PermissionSeverity::Warning)
    );
    assert_eq!(
        find("auth.db").map(|f| f.0),
        Some(PermissionSeverity::Warning)
    );
    assert_eq!(
        find("security.dylib").map(|f| f.0),
        Some(PermissionSeverity::Error)
    );
    assert_eq!(
        find("superd.sock").map(|f| f.0),
        Some(PermissionSeverity::Error)
    );
    // Correctly-moded and absent paths stay out of the report.
    assert_eq!(find("snapshot.json"), None);

    assert_eq!(audit.errors(), 2);
    assert!(audit.warnings() >= 3);
    // errors sort first
    assert_eq!(audit.findings[0].severity, PermissionSeverity::Error);
}

#[test]
fn config_file_is_skipped_without_a_live_auth_secret() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_mode(root, "conf/super.toml", 0o644);

    let audit = audit_instance(root, &test_config(None, None));
    assert!(
        !audit
            .findings
            .iter()
            .any(|f| f.path.ends_with("super.toml")),
        "0644 super.toml without auth_secret is not a secret: {:?}",
        audit.findings
    );

    // The placeholder written by installers must not trigger the check either.
    write_mode(root, "conf/super.toml", 0o644);
    let audit = audit_instance(root, &test_config(Some("CHANGE-ME-replace-me"), None));
    assert!(
        !audit
            .findings
            .iter()
            .any(|f| f.path.ends_with("super.toml")),
        "installer placeholder is not a live credential: {:?}",
        audit.findings
    );
}

#[test]
fn write_owner_only_creates_and_retightens() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("sso_client.secret");

    write_owner_only(&path, b"first").unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );

    // A file that was loosened by hand must be tightened before the next
    // write lands, not after.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    write_owner_only(&path, b"second").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "second");
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn ensure_sqlite_owner_only_covers_sidecars_and_ignores_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let db = write_mode(tmp.path(), "events.db", 0o644);
    let wal = write_mode(tmp.path(), "events.db-wal", 0o644);
    // events.db-shm does not exist yet.

    let failures = ensure_sqlite_owner_only(&db);
    assert!(failures.is_empty(), "{failures:?}");
    for path in [&db, &wal] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600,
            "{}",
            path.display()
        );
    }
}
