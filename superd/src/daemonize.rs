//! Unix self-daemonize (double-fork + setsid). Call before starting Tokio.

use std::fs::OpenOptions;
use std::path::Path;

use nix::unistd::{ForkResult, dup2_stderr, dup2_stdin, dup2_stdout, fork, setsid};

/// Detach from the controlling terminal. Parent processes exit 0.
///
/// After return, the caller is the daemon grandchild with a new session.
pub fn daemonize() -> anyhow::Result<()> {
    match unsafe { fork() }? {
        ForkResult::Parent { .. } => {
            // First parent exits so the child can become session leader.
            std::process::exit(0);
        }
        ForkResult::Child => {}
    }

    setsid()?;

    match unsafe { fork() }? {
        ForkResult::Parent { .. } => {
            std::process::exit(0);
        }
        ForkResult::Child => {}
    }

    redirect_stdio_to_devnull()?;
    Ok(())
}

fn redirect_stdio_to_devnull() -> anyhow::Result<()> {
    let devnull = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/null")?;
    dup2_stdin(&devnull)?;
    dup2_stdout(&devnull)?;
    dup2_stderr(&devnull)?;
    Ok(())
}

/// True when running as container/init PID 1 (daemonize would break the supervisor).
pub fn is_pid1() -> bool {
    std::process::id() == 1
}

/// Preflight before fork: refuse if pidfile is held by a live process.
pub fn preflight_pidfile(path: &Path) -> anyhow::Result<()> {
    use common::PidfileStatus;
    use common::inspect_pidfile;
    match inspect_pidfile(path) {
        PidfileStatus::Alive { pid } => anyhow::bail!(
            "pidfile {} is held by running process {pid}; refuse to start",
            path.display()
        ),
        PidfileStatus::Stale { .. } | PidfileStatus::Invalid | PidfileStatus::Missing => Ok(()),
    }
}
