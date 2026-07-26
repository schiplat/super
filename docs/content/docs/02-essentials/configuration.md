---
title: "Configuration"
weight: 2
description: "Understanding the super.toml configuration file format."
---

Super uses TOML (Tom's Obvious, Minimal Language) for configuration. By default, it looks for `super.toml` in the current directory, `/etc/super/`, or `~/.super/`.

## Server Configuration

The `[server]` section controls the `superd` daemon itself.

```toml
[server]
# The IP and port for the API and Web UI
host = "127.0.0.1"
port = 9002

# OSS has no API auth. superd refuses non-loopback bind unless you opt in here
# or load the security plugin. Shipped example configs set this to false explicitly.
allow_insecure_public_bind = false

# Graceful shutdown timeout (seconds)
shutdown_timeout = 10

# Flapping detection (e.g., max 5 restarts in 60 seconds)
flapping_window = 60
flapping_threshold = 5

# Self-daemonize (Unix only). Keep false under systemd/Docker.
# CLI overrides: --daemon / --foreground. See Config reference.
# daemon = false
# pidfile = "run/superd.pid"   # default when daemonizing; relative to SUPER_ROOT

[logging]
# Daemon's own log level (debug, info, warn, error)
log_level = "info"
```

> **OSS security:** OSS builds ship with `host = "127.0.0.1"` and `allow_insecure_public_bind = false`. To bind on `0.0.0.0` or another non-loopback address you must either set `allow_insecure_public_bind = true` (acknowledging that the API is open to the network) or load the **`security` plugin** for token-based auth. Protect the port with a firewall or reverse proxy in either case.

> **Process model:** Default is **foreground** (correct for systemd `Type=simple` and containers). Optional `[server] daemon = true` / `superd --daemon` detaches without systemd; do not combine with a systemd unit. Full keys: [Config reference — `[server]`](/docs/06-internals/config-reference/#server).

### OSS security defaults (fail-closed)

Super defaults to **restrictive, fail-closed** behaviour in OSS. You can opt into broader exposure, but the daemon will not silently widen the attack surface:

| Area | Default behaviour | How to change (if you accept the risk) |
| :--- | :--- | :--- |
| **API bind** | Refuses non-loopback `host` unless auth is active | OSS: `allow_insecure_public_bind = true`, or load **`security`** (N/A for licensed — security is mandatory) |
| **Custom log paths** | `stdout_logfile` / `stderr_logfile` must resolve under `storage.log_dir` | Use paths inside `log_dir` (relative paths are joined there) |
| **OTA downloads** | Remote URLs must be **HTTPS**; cloud metadata endpoints blocked | Use HTTPS release URLs; loopback HTTP allowed for local dev only |
| **Health HTTP probes** | `http://` and `https://` only; no file or exotic schemes | Point probes at your service URLs |
| **Plugin libraries** | Loaded only from `$SUPER_ROOT/plugins/` after license verification | Ship authorized `.so` / `.dylib` from your subscription package |
| **Include stacks** | `[include].files` globs outside `SUPER_ROOT` are skipped | Keep stack JSON under your install root |
| **API responses** | Env keys matching `SECRET`, `PASSWORD`, `TOKEN`, `KEY`, `CREDENTIAL` are masked | See [Environment & Secrets](/docs/02-essentials/environment-secrets) |
| **Swagger UI** | Off by default (`enable_docs = false`); when on, served at `/api/docs` | Set `enable_docs = true` only on trusted localhost setups |

See [Authentication](/docs/05-advanced-management/authentication#licensed-deployments-require-security) and [SECURITY.md](https://github.com/hzbd/super/blob/master/SECURITY.md) for the full OSS security model.

## Program Configuration

You define managed processes using `[[programs]]` blocks. You can have as many as you like.

### Basic Example

```toml
[[programs]]
name = "my-worker"
command = "/usr/local/bin/worker"
args = ["--config", "/etc/worker.conf"]
cwd = "/tmp"
autostart = true
```

### Environment Variables

You can inject environment variables into the process.

```toml
[programs.env]
NODE_ENV = "production"
DB_HOST = "10.0.0.5"
```

> **Note**: Super automatically injects metadata variables like `SUPER_ID`, `SUPER_NAME`, and `SUPER_HOSTNAME` into the child process.

### User & Group

If running as root, you can drop privileges to a specific user.

```toml
[[programs]]
name = "safe-service"
command = "./app"
user = "www-data"
# group = "www-data" # Optional, defaults to user's primary group
```

### Advanced Settings (OSS)

Dependency orchestration in `super.toml`:

```toml
[[programs]]
name = "heavy-job"
command = "./processor"
depends_on = ["database", "redis"]
```

### Plugin-only blocks 💎

The following require **subscription plugins** (resource limits on Linux). OSS accepts `resource_limits` in the API schema but does not enforce them without the matching plugin.

```toml
[[programs]]
name = "heavy-job"
command = "./processor"

[programs.resource_limits]
memory_limit = 536870912  # 512 MB
cpu_quota = 50.0          # 50% of one core
```

### Scheduled tasks

Cron scheduling is built into OSS `superd`. See [Scheduled Tasks](/docs/02-essentials/scheduled-tasks).

```toml
[[programs]]
name = "nightly-backup"
command = "/scripts/backup.sh"
cron = "0 0 2 * * *"   # see Scheduled Tasks doc
```

See [Resource Isolation](/docs/05-advanced-management/resource-isolation) and [Scheduled Tasks](/docs/02-essentials/scheduled-tasks).

### Restart & stop behaviour

Supervisor-compatible restart and stop settings:

```toml
[[programs]]
name = "api-server"
command = "/usr/local/bin/api"
autostart = true
autorestart = "unexpected"   # restart on unexpected exit only
exitcodes = [0]
retry_limit = 3
startsecs = 10               # stable run resets retry counter
stopsecs = 30                # SIGTERM grace before SIGKILL (optional)
priority = 100               # lower = starts earlier on boot
```

For a full list of options, see the [Config Reference](/docs/06-internals/config-reference). Cron scheduling: [Scheduled Tasks](/docs/02-essentials/scheduled-tasks).
