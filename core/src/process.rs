use common::ProgramConfig;
#[cfg(unix)]
use nix::unistd::User;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::process::{Child, Command};

/// Build a Tokio Command from config and spawn the process.
pub fn spawn_process(
    config: &ProgramConfig,
    extra_envs: &HashMap<String, String>,
) -> anyhow::Result<Child> {
    let mut cmd = Command::new(&config.command);

    // 1. Arguments
    cmd.args(&config.args);

    // 2. Environment variables
    cmd.envs(&config.env);

    // Inject system env (SUPER_ID, SUPER_NAME, etc.)
    cmd.envs(extra_envs);

    // 3. Working directory
    if let Some(dir) = &config.cwd {
        cmd.current_dir(dir);
    }

    // 4. User switching
    if let Some(username) = &config.user {
        #[cfg(unix)]
        {
            if let Some(user) = User::from_name(username)? {
                use std::ffi::CString;
                let uid = user.uid.as_raw();
                let gid = user.gid.as_raw();
                let username_c = CString::new(username.clone())
                    .map_err(|_| anyhow::anyhow!("Invalid username string"))?;

                // SAFETY: `pre_exec` runs in the child after fork, before exec.
                // `username_c` is moved into the closure and outlives the call;
                // `initgroups` is async-signal-safe per POSIX and `c_user` is a
                // valid NUL-terminated pointer to the moved `CString`.
                unsafe {
                    cmd.pre_exec(move || {
                        let c_user = username_c.as_ptr();
                        if libc::initgroups(c_user, gid as _) != 0 {
                            return Err(std::io::Error::last_os_error());
                        }
                        Ok(())
                    });
                }
                cmd.gid(gid);
                cmd.uid(uid);
            } else {
                return Err(anyhow::anyhow!(
                    "User '{}' not found on this system",
                    username
                ));
            }
        }

        #[cfg(not(unix))]
        {
            tracing::warn!(
                "User switching (su) is not supported on non-Unix systems. Ignoring user='{}'.",
                username
            );
        }
    }

    // New process group: child and descendants share one PGID (equals child PID).
    // Windows does not support process_group.
    #[cfg(unix)]
    cmd.process_group(0);

    // Reset OOM score for managed children on Linux. superd lowers its own
    // `oom_score_adj` to -1000 at bootstrap (daemon self-protection); that value
    // is inherited across fork/exec, which would make every managed program
    // OOM-immune — the kernel then can never kill it when it exceeds
    // `resource_limits.memory_limit`, turning the hard cap into a livelock.
    // Writing 0 raises the score (-1000 → 0), which needs no capability.
    #[cfg(target_os = "linux")]
    unsafe {
        // SAFETY: `pre_exec` runs in the child after fork, before exec.
        // `open`/`write`/`close` are async-signal-safe per POSIX.
        cmd.pre_exec(|| {
            let path = c"/proc/self/oom_score_adj";
            let fd = libc::open(path.as_ptr(), libc::O_WRONLY);
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let buf = b"0\n";
            let written = libc::write(fd, buf.as_ptr().cast(), buf.len());
            libc::close(fd);
            if written != buf.len() as isize {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    // 5. Pipes
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    // 6. Daemon mode (optional)
    // cmd.kill_on_drop(false);

    let child = cmd.spawn()?;
    Ok(child)
}
