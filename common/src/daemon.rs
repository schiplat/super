//! Optional self-daemonize helpers shared by `superd` and `super doctor`.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Default pidfile relative to `SUPER_ROOT` when daemonizing without an override.
pub const DEFAULT_PIDFILE_REL: &str = "run/superd.pid";

/// True when the process appears to be started by systemd (service unit).
pub fn under_systemd() -> bool {
    env_nonempty("INVOCATION_ID") || env_nonempty("NOTIFY_SOCKET")
}

fn env_nonempty(key: &str) -> bool {
    std::env::var_os(key).is_some_and(|v| !v.is_empty())
}

/// Resolve a pidfile path: absolute as-is; relative joined under `root`.
pub fn resolve_pidfile_path(root: &Path, configured: Option<&Path>) -> PathBuf {
    match configured {
        Some(p) if p.is_absolute() => p.to_path_buf(),
        Some(p) => root.join(p),
        None => root.join(DEFAULT_PIDFILE_REL),
    }
}

/// Effective daemonize flag: `--foreground` > `--daemon` > config > false.
pub fn resolve_daemonize(foreground: bool, cli_daemon: bool, config_daemon: bool) -> bool {
    if foreground {
        return false;
    }
    if cli_daemon {
        return true;
    }
    config_daemon
}

/// Whether a pidfile should be written for this start.
/// Daemon mode always writes; foreground only when pidfile was explicitly set.
pub fn should_write_pidfile(daemonize: bool, explicit_pidfile: bool) -> bool {
    daemonize || explicit_pidfile
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PidfileStatus {
    Missing,
    /// File exists but contents are not a valid pid.
    Invalid,
    /// Pid in file is not running (stale).
    Stale {
        pid: i32,
    },
    /// Pid appears alive (or we lack permission to signal it — treat as in use).
    Alive {
        pid: i32,
    },
}

/// Read and classify an existing pidfile (does not create or remove).
pub fn inspect_pidfile(path: &Path) -> PidfileStatus {
    if !path.exists() {
        return PidfileStatus::Missing;
    }
    let Ok(mut f) = fs::File::open(path) else {
        return PidfileStatus::Invalid;
    };
    let mut buf = String::new();
    if f.read_to_string(&mut buf).is_err() {
        return PidfileStatus::Invalid;
    }
    let pid: i32 = match buf.trim().parse() {
        Ok(p) if p > 1 => p,
        _ => return PidfileStatus::Invalid,
    };
    if pid_is_alive(pid) {
        PidfileStatus::Alive { pid }
    } else {
        PidfileStatus::Stale { pid }
    }
}

/// Write `pid` to `path`, creating parent directories. Fails if another live process owns the file.
pub fn claim_pidfile(path: &Path, pid: i32) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    match inspect_pidfile(path) {
        PidfileStatus::Alive { pid: other } => {
            anyhow::bail!(
                "pidfile {} is held by running process {other}; refuse to start",
                path.display()
            );
        }
        PidfileStatus::Stale { .. } | PidfileStatus::Invalid | PidfileStatus::Missing => {}
    }
    let mut f = fs::File::create(path)?;
    writeln!(f, "{pid}")?;
    Ok(())
}

/// Remove pidfile only if it still contains `pid`.
pub fn release_pidfile(path: &Path, pid: i32) {
    match inspect_pidfile(path) {
        PidfileStatus::Alive { pid: other } | PidfileStatus::Stale { pid: other }
            if other == pid =>
        {
            let _ = fs::remove_file(path);
        }
        PidfileStatus::Invalid | PidfileStatus::Missing => {
            // If we wrote it and it became unreadable, try remove anyway when content matches.
            if let Ok(s) = fs::read_to_string(path)
                && s.trim().parse::<i32>().ok() == Some(pid)
            {
                let _ = fs::remove_file(path);
            }
        }
        _ => {}
    }
}

/// Parent directory of pidfile is missing or not writable.
pub fn pidfile_parent_unwritable(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    if !parent.exists() {
        // Will be created at claim time — check nearest existing ancestor.
        let mut p = parent.to_path_buf();
        while let Some(up) = p.parent() {
            if up.exists() {
                return fs::metadata(up)
                    .map(|m| m.permissions().readonly())
                    .unwrap_or(true);
            }
            if up.as_os_str().is_empty() {
                break;
            }
            p = up.to_path_buf();
        }
        return false;
    }
    // Probe writability by checking directory metadata; on Unix also try access.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match fs::metadata(parent) {
            Ok(m) => {
                let mode = m.permissions().mode();
                // Owner-write bit as a coarse check (doctor hint, not security boundary).
                mode & 0o200 == 0
            }
            Err(_) => true,
        }
    }
    #[cfg(not(unix))]
    {
        fs::metadata(parent)
            .map(|m| m.permissions().readonly())
            .unwrap_or(true)
    }
}

#[cfg(unix)]
pub fn pid_is_alive(pid: i32) -> bool {
    // signal 0: existence check; EPERM means process exists but we can't signal it.
    let rc = unsafe { libc::kill(pid, 0) };
    if rc == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
pub fn pid_is_alive(_pid: i32) -> bool {
    false
}

#[cfg(test)]
mod tests {
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
}
