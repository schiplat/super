---
title: "Configuration"
weight: 2
description: "Understanding the super.toml configuration file format."
---

Super uses TOML (Tom's Obvious, Minimal Language) for configuration. The daemon reads `$SUPER_ROOT/conf/super.toml`; `SUPER_ROOT` defaults to the directory containing the `bin/` next to the `superd` executable, or the current working directory (see [Config Reference — Instance layout](/docs/06-internals/config-reference#instance-layout-super_root) and [Environment Variables](/docs/06-internals/environment-variables#super_root)). The CLI's offline tools (`super check`) additionally probe `super.toml`, `conf/super.toml`, and `/etc/super/super.toml`.

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

> [!CAUTION]
> OSS builds ship with `host = "127.0.0.1"` and `allow_insecure_public_bind = false`. To bind on `0.0.0.0` or another non-loopback address you must either set `allow_insecure_public_bind = true` (acknowledging that the API is open to the network) or load the **`security` plugin** for token-based auth. Protect the port with a firewall or reverse proxy in either case.

> [!NOTE]
> Default is **foreground** (correct for systemd `Type=simple` and containers). Optional `[server] daemon = true` / `superd --daemon` detaches without systemd; do not combine with a systemd unit. Full keys: [Config reference — `[server]`](/docs/06-internals/config-reference/#server).

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

See [Authentication](/docs/05-advanced-management/authentication#licensed-deployments-require-security) and [SECURITY.md](https://github.com/schiplat/super/blob/master/SECURITY.md) for the full OSS security model.

## Program Configuration

Programs are **not** declared in `super.toml`. Define them via JSON stack files, the API, or the CLI — Super persists them to `data/snapshot.json` and reloads them on start.

> [!WARNING]
> `[[programs]]` / `[[program]]` tables in `super.toml` are **ignored**. `super check` flags them as an error. Programs load only from `[include]` JSON stacks (`conf/conf.d/*.json`), the API, and `data/snapshot.json`.

| Source | How | Persisted to |
| :--- | :--- | :--- |
| JSON stack files | `conf/conf.d/*.json`, matched by `[include].files` | `data/snapshot.json` |
| CLI | `super add --name my-worker --autostart /usr/local/bin/worker` | `data/snapshot.json` |
| HTTP API | `POST /api/v1/programs` | `data/snapshot.json` |

### JSON stack files (declarative)

Point `super.toml` at your stack directory once with `[include].files` (a glob list of JSON stack files):

```toml
[include]
files = ["conf/conf.d/*.json"]
```

Then create files under `conf/conf.d/` — each file is a **stack** applied on daemon start and on `super reload`:

```json
{
  "prune": false,
  "services": [
    {
      "name": "my-worker",
      "command": "/usr/local/bin/worker",
      "args": ["--config", "/etc/worker.conf"],
      "cwd": "/tmp",
      "autostart": true
    }
  ]
}
```

`prune: true` removes programs that are not listed in the stack.

### Environment Variables

You can inject environment variables into the process.

```json
{
  "services": [
    {
      "name": "my-worker",
      "command": "./app",
      "env": {
        "NODE_ENV": "production",
        "DB_HOST": "10.0.0.5"
      }
    }
  ]
}
```

> [!NOTE]
> Super automatically injects metadata variables like `SUPER_ID`, `SUPER_NAME`, and `SUPER_HOSTNAME` into the child process.

### User & Group

If running as root, you can drop privileges to a specific user.

```json
{
  "services": [
    {
      "name": "safe-service",
      "command": "./app",
      "user": "www-data"
    }
  ]
}
```

### Advanced Settings (OSS)

Dependency orchestration — the `depends_on` array names programs that must be **Healthy** first:

```json
{
  "services": [
    { "name": "database", "command": "./db", "health_check": { "type": "tcp", "port": 5432 } },
    { "name": "heavy-job", "command": "./processor", "depends_on": ["database", "redis"] }
  ]
}
```

### Plugin-only blocks 💎

The following require **subscription plugins** (resource limits on Linux). OSS accepts `resource_limits` in the API schema but does not enforce them without the matching plugin.

```json
{
  "services": [
    {
      "name": "heavy-job",
      "command": "./processor",
      "resource_limits": {
        "memory_limit": 512,
        "cpu_quota": 0.5
      }
    }
  ]
}
```

### Scheduled tasks

Cron scheduling is built into OSS `superd`. See [Scheduled Tasks](/docs/02-essentials/scheduled-tasks).

```json
{
  "services": [
    {
      "name": "nightly-backup",
      "command": "/scripts/backup.sh",
      "cron": "0 0 2 * * *"
    }
  ]
}
```

See [Resource Isolation](/docs/05-advanced-management/resource-isolation) and [Scheduled Tasks](/docs/02-essentials/scheduled-tasks).

### Restart & stop behaviour

Supervisor-compatible restart and stop settings:

```json
{
  "services": [
    {
      "name": "api-server",
      "command": "/usr/local/bin/api",
      "autostart": true,
      "autorestart": "unexpected",
      "exitcodes": [0],
      "retry_limit": 3,
      "startsecs": 10,
      "stopsecs": 30,
      "priority": 100
    }
  ]
}
```

For a full list of options, see the [Config Reference](/docs/06-internals/config-reference). Cron scheduling: [Scheduled Tasks](/docs/02-essentials/scheduled-tasks).
