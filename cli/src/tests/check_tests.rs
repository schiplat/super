use super::*;
use std::fs;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn resolve_config_prefers_super_root_conf() {
    let _guard = env_lock().lock().unwrap();
    let dir = std::env::temp_dir().join(format!(
        "super-check-root-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(dir.join("conf")).unwrap();
    let conf = dir.join("conf/super.toml");
    fs::write(&conf, "[server]\nhost = \"127.0.0.1\"\nport = 9002\n").unwrap();

    // SAFETY: serialized by env_lock; restored before unlock.
    unsafe {
        std::env::set_var("SUPER_ROOT", &dir);
    }
    let got = resolve_config_path(None).unwrap();
    unsafe {
        std::env::remove_var("SUPER_ROOT");
    }
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(got, conf);
}

#[test]
fn licensed_ok_when_security_plugin_and_auth_present() {
    let dir = std::env::temp_dir().join(format!(
        "super-check-ok-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let plugins = dir.join("plugins");
    fs::create_dir_all(&plugins).unwrap();
    fs::write(plugins.join("security.dylib"), b"fake").unwrap();

    let errors =
        licensed_requirement_errors(&["security".into(), "ui".into()], &plugins, Some("secret"));
    assert!(errors.is_empty(), "{errors:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn licensed_errors_without_security_plugin_or_auth() {
    let dir = std::env::temp_dir().join(format!(
        "super-check-err-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let plugins = dir.join("plugins");
    fs::create_dir_all(&plugins).unwrap();

    let errors = licensed_requirement_errors(&["ui".into()], &plugins, Some("  "));
    assert!(
        errors
            .iter()
            .any(|e| e.contains("security' in license claims")),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|e| e.contains("security.so")),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|e| e.contains("auth_secret")),
        "{errors:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stray_program_tables_detected() {
    assert!(stray_program_tables_in_toml("[[program]]\nname = \"x\"\n"));
    assert!(stray_program_tables_in_toml("[[programs]]\n"));
    assert!(!stray_program_tables_in_toml("# [[program]]\n[server]\n"));
    assert!(!stray_program_tables_in_toml("[include]\nfiles = []\n"));
}

#[test]
fn include_json_syntax_error_is_reported() {
    let dir = std::env::temp_dir().join(format!(
        "super-check-inc-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let confd = dir.join("conf").join("conf.d");
    fs::create_dir_all(&confd).unwrap();
    fs::write(confd.join("bad.json"), "{ not json ").unwrap();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    check_include_stacks(
        &dir,
        &["conf/conf.d/*.json".into()],
        &dir.join("logs"),
        &mut errors,
        &mut warnings,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("bad.json:") && e.contains(":1:")),
        "{errors:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn include_json_valid_stack_ok() {
    let dir = std::env::temp_dir().join(format!(
        "super-check-inc-ok-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let confd = dir.join("conf").join("conf.d");
    fs::create_dir_all(&confd).unwrap();
    fs::write(
        confd.join("ok.json"),
        r#"{"services":[{"name":"a","command":"/bin/true"}]}"#,
    )
    .unwrap();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    check_include_stacks(
        &dir,
        &["conf/conf.d/*.json".into()],
        &dir.join("logs"),
        &mut errors,
        &mut warnings,
    );
    assert!(errors.is_empty(), "{errors:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn include_json_empty_command_is_reported() {
    let dir = std::env::temp_dir().join(format!(
        "super-check-inc-empty-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let confd = dir.join("conf").join("conf.d");
    fs::create_dir_all(&confd).unwrap();
    fs::write(
        confd.join("empty.json"),
        r#"{"services":[{"name":"a","command":"  "}]}"#,
    )
    .unwrap();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    check_include_stacks(
        &dir,
        &["conf/conf.d/*.json".into()],
        &dir.join("logs"),
        &mut errors,
        &mut warnings,
    );
    assert!(
        errors.iter().any(|e| {
            e.contains("services[0] (name=a)") && e.contains("command: must not be empty")
        }),
        "{errors:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}
