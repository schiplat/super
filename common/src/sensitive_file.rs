//! Permission audit for files that carry credentials or process state.
//!
//! Two consumers share these rules: `superd` warns at startup when
//! `conf/super.toml` holds a live credential, and `super doctor` prints the
//! full report. Keeping the bit tests and the `chmod` hints in one place means
//! the daemon and the CLI never disagree about what "too open" means.
//!
//! This module only *reports*. Nothing here changes permissions: a daemon
//! quietly rewriting modes on files it did not create is how deployments
//! break, and an audit tool that fixes what it finds hides the problem from
//! the operator who owns the host.

use std::path::{Path, PathBuf};

use crate::config::ServerConfig;
use crate::paths::auth_db_path;

/// What kind of exposure matters for a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// Credentials, session state, or config that can grant access.
    /// Group/other *read* is a leak; group/other *write* is a takeover.
    Secret,
    /// Executable code, sockets, and directories whose contents can be
    /// swapped out. Read access is normal; write access by anyone but the
    /// owner is the risk.
    Writable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PermissionSeverity {
    /// Confidentiality exposure (someone can read it). Reported, not fatal.
    Warning,
    /// Integrity exposure (someone can replace it). Fails `super doctor`.
    Error,
}

/// One path that is more permissive than it should be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionFinding {
    pub path: PathBuf,
    pub mode: u32,
    pub severity: PermissionSeverity,
    /// Short reason, e.g. `"group-readable/world-readable"`.
    pub reason: String,
    /// Exact remediation, e.g. `"chmod 600 /srv/super/conf/super.toml"`.
    pub hint: String,
}

/// Result of auditing one instance root.
#[derive(Debug, Clone, Default)]
pub struct PermissionAudit {
    /// Paths that exist and were actually inspected.
    pub checked: usize,
    pub findings: Vec<PermissionFinding>,
}

impl PermissionAudit {
    pub fn errors(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == PermissionSeverity::Error)
            .count()
    }

    pub fn warnings(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == PermissionSeverity::Warning)
            .count()
    }
}

/// Inspect one path. `Ok(None)` means the path does not exist (or the
/// platform has no POSIX mode bits to inspect), so there is nothing to report.
#[cfg(unix)]
pub fn inspect(path: &Path, kind: PathKind) -> std::io::Result<Option<PermissionFinding>> {
    use std::os::unix::fs::PermissionsExt;

    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mode = meta.permissions().mode() & 0o777;
    let Some((reason, severity)) = describe(mode, kind) else {
        return Ok(None);
    };
    let suggested = suggested_mode(mode, kind);
    Ok(Some(PermissionFinding {
        path: path.to_path_buf(),
        mode,
        severity,
        reason,
        hint: format!("chmod {suggested:03o} {}", path.display()),
    }))
}

#[cfg(not(unix))]
pub fn inspect(_path: &Path, _kind: PathKind) -> std::io::Result<Option<PermissionFinding>> {
    Ok(None)
}

/// Force a data file to owner-only mode. Missing files are not an error.
///
/// SQLite derives the mode of `-wal`/`-shm` sidecars from the umask, so a
/// 0600 database can end up with 0644 sidecars holding the same rows. Stores
/// call this after opening (and after migrations, which is when the sidecars
/// first appear) instead of trusting the default.
#[cfg(unix)]
pub fn ensure_owner_only(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(not(unix))]
pub fn ensure_owner_only(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Tighten a SQLite database plus its `-wal`/`-shm` sidecars.
///
/// Returns the paths whose chmod failed; callers log and keep going (an
/// unopenable store would be worse than a wide mode that the next
/// `super doctor` run reports).
pub fn ensure_sqlite_owner_only(db_path: &Path) -> Vec<(PathBuf, std::io::Error)> {
    let mut failures = Vec::new();
    for suffix in ["", "-wal", "-shm"] {
        let mut name = db_path.as_os_str().to_owned();
        name.push(suffix);
        let path = PathBuf::from(name);
        if let Err(e) = ensure_owner_only(&path) {
            failures.push((path, e));
        }
    }
    failures
}

/// Write `bytes` to `path`, owner-only from the moment the file exists.
///
/// `File::create` + `chmod` leaves a window where a umask-derived 0644 file
/// already contains the secret. This creates with mode 0600, re-tightens an
/// existing file before writing, and fsyncs. Not atomic (no temp+rename):
/// callers that need atomic replacement keep their own tmp-file dance.
#[cfg(unix)]
pub fn write_owner_only(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    // `truncate(true)` keeps the mode of an existing file; tighten it before
    // any new bytes land in it.
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(not(unix))]
pub fn write_owner_only(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}

#[cfg(unix)]
fn describe(mode: u32, kind: PathKind) -> Option<(String, PermissionSeverity)> {
    let mut labels: Vec<&str> = Vec::new();
    match kind {
        PathKind::Secret => {
            if mode & 0o040 != 0 {
                labels.push("group-readable");
            }
            if mode & 0o004 != 0 {
                labels.push("world-readable");
            }
            if mode & 0o020 != 0 {
                labels.push("group-writable");
            }
            if mode & 0o002 != 0 {
                labels.push("world-writable");
            }
            if mode & 0o010 != 0 {
                labels.push("group-executable");
            }
            if mode & 0o001 != 0 {
                labels.push("world-executable");
            }
        }
        PathKind::Writable => {
            if mode & 0o020 != 0 {
                labels.push("group-writable");
            }
            if mode & 0o002 != 0 {
                labels.push("world-writable");
            }
        }
    }
    if labels.is_empty() {
        return None;
    }
    let severity = if mode & 0o002 != 0 {
        PermissionSeverity::Error
    } else {
        PermissionSeverity::Warning
    };
    Some((labels.join("/"), severity))
}

/// Strip the offending bits so the printed hint is a mode that would pass.
#[cfg(unix)]
fn suggested_mode(mode: u32, kind: PathKind) -> u32 {
    match kind {
        PathKind::Secret => match mode & 0o700 {
            0 => 0o600,
            m => m,
        },
        PathKind::Writable => match mode & !0o022 {
            0 => 0o755,
            m => m,
        },
    }
}

/// Audit every path that belongs to one instance root.
///
/// Absent files are skipped: a fresh install with no SSO state and no plugins
/// must report a clean bill of health rather than a list of missing paths.
pub fn audit_instance(root: &Path, cfg: &ServerConfig) -> PermissionAudit {
    let mut targets: Vec<(PathBuf, PathKind)> = Vec::new();

    push_target(&mut targets, root.join("conf"), PathKind::Writable);
    let has_live_secret = cfg.server.auth_secret.as_deref().is_some_and(|s| {
        !s.trim().is_empty() && !crate::auth_secret::is_placeholder_auth_secret(s)
    });
    if has_live_secret {
        push_target(&mut targets, root.join("conf/super.toml"), PathKind::Secret);
    }

    let storage = cfg.storage.resolve_under_root(root);
    let data_dir = storage
        .data_file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join("data"));

    push_target(&mut targets, data_dir.clone(), PathKind::Writable);
    push_target(&mut targets, storage.data_file.clone(), PathKind::Secret);
    push_target(&mut targets, storage.events_file.clone(), PathKind::Secret);
    for suffix in ["-wal", "-shm"] {
        let mut name = storage.events_file.clone().into_os_string();
        name.push(suffix);
        push_target(&mut targets, PathBuf::from(name), PathKind::Secret);
    }
    // SSO configuration, client secret and sessions share one store; the
    // doctor reports the database and its sidecars together.
    push_target(
        &mut targets,
        auth_db_path(root, &storage.data_file),
        PathKind::Secret,
    );
    // OSS mode can persist a generated admin secret for restart stability.
    push_target(
        &mut targets,
        crate::auth_secret::ephemeral_auth_secret_path(root),
        PathKind::Secret,
    );

    let plugins_dir = root.join("plugins");
    push_target(&mut targets, plugins_dir.clone(), PathKind::Writable);
    if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_plugin = path.is_file()
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e, "so" | "dylib" | "dll"));
            if is_plugin {
                push_target(&mut targets, path, PathKind::Writable);
            }
        }
    }

    let pidfile = crate::daemon::resolve_pidfile_path(root, cfg.server.pidfile.as_deref());
    if let Some(parent) = pidfile.parent() {
        push_target(&mut targets, parent.to_path_buf(), PathKind::Writable);
    }
    push_target(&mut targets, pidfile, PathKind::Writable);

    if let Some(socket) = cfg.server.socket.as_deref() {
        let socket = crate::resolve_storage_path(root, socket);
        if let Some(parent) = socket.parent() {
            push_target(&mut targets, parent.to_path_buf(), PathKind::Writable);
        }
        push_target(&mut targets, socket, PathKind::Writable);
    }

    let mut audit = PermissionAudit::default();
    for (path, kind) in targets {
        // `Ok(None)` is a missing path; `Err` is a mode we cannot stat (a
        // parent directory denies us). Neither supports an assertion.
        if let Ok(Some(finding)) = inspect(&path, kind) {
            audit.checked += 1;
            audit.findings.push(finding);
        }
    }
    audit.findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.path.as_os_str().cmp(b.path.as_os_str()))
    });
    audit
}

fn push_target(targets: &mut Vec<(PathBuf, PathKind)>, path: PathBuf, kind: PathKind) {
    if !targets.iter().any(|(p, k)| p == &path && *k == kind) {
        targets.push((path, kind));
    }
}

#[cfg(test)]
#[path = "tests/sensitive_file_tests.rs"]
mod tests;
