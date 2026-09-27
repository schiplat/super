use super::*;
use std::sync::Mutex;

// Serialize env mutations across tests.
static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn resolve_pidfile_default_and_relative() {
    let root = Path::new("/opt/super");
    assert_eq!(
        resolve_pidfile_path(root, None),
        PathBuf::from("/opt/super/run/superd.pid")
    );
    assert_eq!(
        resolve_pidfile_path(root, Some(Path::new("run/custom.pid"))),
        PathBuf::from("/opt/super/run/custom.pid")
    );
    assert_eq!(
        resolve_pidfile_path(root, Some(Path::new("/var/run/superd.pid"))),
        PathBuf::from("/var/run/superd.pid")
    );
}

#[test]
fn daemonize_precedence() {
    assert!(!resolve_daemonize(true, true, true));
    assert!(resolve_daemonize(false, true, false));
    assert!(resolve_daemonize(false, false, true));
    assert!(!resolve_daemonize(false, false, false));
}

#[test]
fn write_pidfile_policy() {
    assert!(should_write_pidfile(true, false));
    assert!(should_write_pidfile(true, true));
    assert!(should_write_pidfile(false, true));
    assert!(!should_write_pidfile(false, false));
}

#[test]
fn under_systemd_reads_env() {
    let _g = ENV_LOCK.lock().unwrap();
    // SAFETY: single-threaded under mutex for this test process.
    unsafe {
        std::env::remove_var("INVOCATION_ID");
        std::env::remove_var("NOTIFY_SOCKET");
    }
    assert!(!under_systemd());
    unsafe {
        std::env::set_var("INVOCATION_ID", "abc");
    }
    assert!(under_systemd());
    unsafe {
        std::env::remove_var("INVOCATION_ID");
        std::env::set_var("NOTIFY_SOCKET", "/run/systemd/notify");
    }
    assert!(under_systemd());
    unsafe {
        std::env::remove_var("NOTIFY_SOCKET");
    }
}

#[test]
fn claim_and_release_pidfile() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run/superd.pid");
    let self_pid = std::process::id() as i32;
    claim_pidfile(&path, self_pid).unwrap();
    assert!(matches!(
        inspect_pidfile(&path),
        PidfileStatus::Alive { pid } if pid == self_pid
    ));
    release_pidfile(&path, self_pid);
    assert!(matches!(inspect_pidfile(&path), PidfileStatus::Missing));
}

#[test]
fn stale_pidfile_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("superd.pid");
    fs::write(&path, "999999\n").unwrap();
    assert!(matches!(
        inspect_pidfile(&path),
        PidfileStatus::Stale { pid: 999999 }
    ));
    let self_pid = std::process::id() as i32;
    claim_pidfile(&path, self_pid).unwrap();
    assert!(matches!(
        inspect_pidfile(&path),
        PidfileStatus::Alive { pid } if pid == self_pid
    ));
}

#[test]
fn claim_refuses_when_pidfile_held_by_live_process() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("superd.pid");
    let holder = std::process::id() as i32;
    claim_pidfile(&path, holder).unwrap();
    // Another would-be instance must not steal the pidfile.
    let err = claim_pidfile(&path, holder.wrapping_add(1).max(2)).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("held by running process") && msg.contains(&holder.to_string()),
        "unexpected error: {msg}"
    );
    assert!(matches!(
        inspect_pidfile(&path),
        PidfileStatus::Alive { pid } if pid == holder
    ));
    release_pidfile(&path, holder);
}
