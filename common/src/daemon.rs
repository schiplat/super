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
///
/// The claim is atomic: the pidfile is opened with `O_CREAT|O_EXCL` and held
/// with an exclusive `flock` for the process lifetime. Two concurrent starts
/// can no longer both pass the old check-then-create window; a stale pidfile
/// is unlinked and recreated under the lock.
pub fn claim_pidfile(path: &Path, pid: i32) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // O_EXCL fails if another starter created the file in the check-then-create
    // race window; a leftover from an older (crashed) process is unlinked first.
    for attempt in 0..2 {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path);
        match file {
            Ok(file) => return write_pidfile_locked(file, path, pid),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempt == 0 => {
                // File exists: only unlink a stale/invalid one. A live holder
                // keeps its lock (flock survives unlink-by-another-process
                // only until the fd closes, so verify liveness first).
                match inspect_pidfile(path) {
                    PidfileStatus::Alive { pid: other } => anyhow::bail!(
                        "pidfile {} is held by running process {other}; refuse to start",
                        path.display()
                    ),
                    PidfileStatus::Stale { .. } | PidfileStatus::Invalid => {
                        let _ = fs::remove_file(path);
                        continue;
                    }
                    PidfileStatus::Missing => {
                        // Vanished between the O_EXCL error and the inspect;
                        // retry the exclusive create once.
                        continue;
                    }
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    // Second O_EXCL also hit an existing file: another starter won the race.
    anyhow::bail!(
        "pidfile {} was claimed concurrently by another process; refuse to start",
        path.display()
    );
}

/// Write the pid and keep an exclusive advisory lock on the pidfile fd for
/// the process lifetime. The lock (not the file contents) is the real claim:
/// a concurrent starter's O_EXCL fails, and legacy readers that only inspect
/// the pid still see a live process through [`inspect_pidfile`].
fn write_pidfile_locked(file: fs::File, path: &Path, pid: i32) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            let e = std::io::Error::last_os_error();
            anyhow::bail!(
                "pidfile {} is locked by another process ({e}); refuse to start",
                path.display()
            );
        }
    }
    let mut file = file;
    file.set_len(0)?;
    writeln!(file, "{pid}")?;
    file.sync_all()?;
    // Deliberately leak the File: the open fd keeps the flock held until the
    // process exits (or crashes), which is exactly the claim lifetime. The OS
    // closes leaked fds and releases the lock on exit.
    std::mem::forget(file);
    let _ = path;
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
#[path = "tests/daemon_tests.rs"]
mod tests;
