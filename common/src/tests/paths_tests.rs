use super::*;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn resolve_storage_path_joins_relative_under_root() {
    let root = PathBuf::from("/tmp/super-demo");
    assert_eq!(
        resolve_storage_path(&root, Path::new("./logs")),
        PathBuf::from("/tmp/super-demo/logs")
    );
    assert_eq!(
        resolve_storage_path(&root, Path::new("data/events.db")),
        PathBuf::from("/tmp/super-demo/data/events.db")
    );
}

#[test]
fn resolve_storage_path_keeps_absolute() {
    let root = PathBuf::from("/tmp/super-demo");
    let abs = PathBuf::from("/var/log/super");
    assert_eq!(resolve_storage_path(&root, &abs), abs);
}

#[test]
fn infer_root_from_conf_super_toml() {
    let root = PathBuf::from("/tmp/super-demo");
    let config = root.join("conf/super.toml");
    assert_eq!(
        infer_super_root_from_config(&config).as_deref(),
        Some(root.as_path())
    );
}

#[test]
fn infer_root_from_root_super_toml() {
    let root = PathBuf::from("/opt/super");
    let config = root.join("super.toml");
    assert_eq!(
        infer_super_root_from_config(&config).as_deref(),
        Some(root.as_path())
    );
}

#[test]
fn resolve_for_config_prefers_super_root_env() {
    let _guard = env_lock().lock().unwrap();
    // SAFETY: serialized by env_lock; restored before unlock.
    unsafe {
        env::set_var("SUPER_ROOT", "/env/root");
    }
    let got = resolve_super_root_for_config(Path::new("/ignored/conf/super.toml"));
    unsafe {
        env::remove_var("SUPER_ROOT");
    }
    assert_eq!(got, PathBuf::from("/env/root"));
}
