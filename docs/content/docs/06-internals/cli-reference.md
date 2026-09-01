---
title: "CLI Reference"
weight: 4
description: "Command-line arguments for the 'super' client."
---

The `super` binary is the primary way to interact with the daemon.

**Global Flags:**
*   `--server <URL>`: Override the server endpoint. Accepts an HTTP(S) URL (`http://127.0.0.1:9002`, default) or a Unix socket (`unix:///path/to/superd.sock`). Relative socket paths resolve under `SUPER_ROOT`.

**Endpoint selection** (how `super` picks a daemon to talk to):

1. `--server <URL>` — highest priority; used for this command only.
2. `~/.super/cli.json` `server_url` — persisted by `super login <secret> [url]`; all subsequent commands follow it.
3. **Auto-discovery** (only when neither of the above applies, i.e. no `--server` and no `~/.super/cli.json`): if `$SUPER_ROOT/run/superd.sock` (or `./run/superd.sock` without `SUPER_ROOT`) exists and is a real socket file, the CLI uses `unix://` there; otherwise it falls back to the default `http://127.0.0.1:9002`.

When the daemon listens on both TCP and a Unix socket, an unconfigured CLI therefore prefers the Unix socket automatically. To force TCP, pass `--server http://host:port` or run `super login <secret> http://host:port` to persist it. Auto-discovery only recognizes the conventional `run/superd.sock` path — a custom socket path or a non-default TCP port still needs an explicit `--server` (or a persisted login).

## Starting `superd` (daemon binary)

`superd` is separate from the `super` CLI. It reads `$SUPER_ROOT/conf/super.toml` and listens for API/CLI traffic.

| Mode | Command / config | When to use |
| :--- | :--- | :--- |
| Foreground (default) | `superd` or `superd --foreground` | systemd (`Type=simple`), Docker, debugging |
| Self-daemonize (Unix) | `superd --daemon` or `[server] daemon = true` | Bare metal without systemd; writes `$SUPER_ROOT/run/superd.pid` by default |

CLI overrides: `--daemon` / `--foreground` / `--pidfile <PATH>`. See [Config reference](/docs/06-internals/config-reference/#server) and [Installation — Systemd](/docs/01-getting-started/installation/#method-3-systemd-vm--bare-metal). Program control (`start` / `stop` / `shutdown`) is unchanged either way.

## Core Management

### `list`
List all managed programs and their status.

```bash
super list
```

### `add`
Register a new program without a config file.

```bash
super add <COMMAND> [ARGS...] [FLAGS]
```

**Flags:**
*   `--name <NAME>` / `-n`: Custom name (defaults to binary name).
*   `--autostart`: Enable autostart (default: true).
*   `--cwd <DIR>`: Working directory.
*   `--env <KEY=VAL>` / `-e`: Set environment variables (can be used multiple times).
*   `--env-file <PATH>`: Load environment variables from a file at spawn time.
*   `--user <USER>`: Run as specific user.
*   `--group <GROUP>`: Group name for organization (addressable as `@<group>`).
*   `--numprocs <N>`: Spawn N instances.
*   `--process-name <TPL>`: Process name template for multiple instances (e.g. `worker-{num}`; default `{name}-{num}`).
*   `--autorestart <POLICY>`: `unexpected` (default), `true`, or `false`.
*   `--exitcodes <N,N>`: Comma-separated exit codes considered successful (default: `0`).
*   `--startsecs <N>`: Seconds before exit counts as stable start (default: `10`).
*   `--stopsecs <N>`: Seconds to wait after SIGTERM before SIGKILL (default: server `shutdown_timeout`).
*   `--cron <EXPR>`: Cron expression for scheduled tasks (see [Scheduled Tasks](/docs/02-essentials/scheduled-tasks)).
*   `--cpu <CORES>`: CPU quota in cores (e.g. `1.5`; `isolation` plugin, Linux only).
*   `--memory <MB>`: Memory hard limit in **MB** (`isolation` plugin, Linux only).
*   `--memory-warn-percent <PCT>`: Pre-kill warning threshold as % of limit (default `80`; `0` off; `isolation` plugin, Linux only).
*   `--memory-warn-headroom <MB>`: Warn within this many MB of the limit (`0` off).
*   `--memory-high <MB>`: Kernel soft limit in MB — throttles before OOM-kill (`0` off, opt-in).

### `update`
Update configuration for an existing program.

```bash
super update <name|id> [FLAGS]
```

**Flags:**
*   `--command`, `--args`, `--cwd`, `--user`, `--group`: Execution settings.
*   `--env <KEY=VAL>`, `--env-file <PATH>`: Environment (`--env-file ""` clears).
*   `--autostart`, `--retry-limit`, `--autorestart`, `--exitcodes`, `--startsecs`, `--stopsecs`.
*   `--no-health-check`: Disable health check.
*   `--artifact-url`, `--artifact-sha256`: OTA download URL and expected SHA256 checksum.
*   `--artifact-destination`: **Absolute path** on the host filesystem where the binary lives (e.g. `/usr/local/bin/my-app`). Required on first OTA setup if the program has no existing `artifact`; omit on later updates if unchanged.
*   `--artifact-extract`: Extract archive before swap (default: `false`).
*   Full flow: [Atomic OTA Updates](/docs/03-orchestration/ota-updates).
*   Scheduled tasks: `--cron` (see [Scheduled Tasks](/docs/02-essentials/scheduled-tasks)).
*   Licensed (`isolation` plugin): `--cpu`, `--memory`, `--memory-warn-percent`, `--memory-warn-headroom`, `--memory-high` (Linux only; warns if plugin not loaded).

### `remove` (alias `rm`)
Remove a program configuration. It must be stopped first (see [Process Operations](#process-operations)).

```bash
super remove <name|@group|id|all>
```

## Process Operations

Control commands take a single target, written PM2-style as a union:

```bash
super <start|stop|restart|remove> <name|@group|id|all>
```

*   `<name>` — exact program name (like PM2's `app_name`).
*   `@<group>` — every program in that group (like PM2's `namespace`).
*   `<id>` — program UUID; unambiguous prefixes are accepted.
*   `all` — every managed program (PM2's `'all'`).

Super has no `json_conf` target — declarative batches go through a stack file instead (`super apply <file>`). A target that matches several programs is rejected as ambiguous, with the candidates listed.

Batch targets are protected by three safety knobs (global flags, accepted on any control command):

| Flag | Description |
| :--- | :--- |
| `--yes` / `-y` | Skip the interactive confirmation prompt. Batch operations (`@group` / `all`) ask for confirmation before touching more than one program; `--yes` bypasses it for scripts |
| `--dry-run` | Show which programs the operation would affect (preview list) and exit without executing anything |

Without `--yes`, a batch operation on more than one program prints the affected program list and asks `[y/N]`; answering anything other than `y`/`yes` aborts. Single-target operations never prompt.

### `start`
Start a stopped process.

```bash
super start <name|@group|id|all> [--wait] [--wait-healthy] [--timeout N]
```

| Flag | Description |
| :--- | :--- |
| `--wait` | Poll until the program(s) reach `Running`/`Healthy` |
| `--wait-healthy` | Poll until the program(s) reach `Healthy` (readiness check passed). Mutually exclusive with `--wait` |
| `--timeout N` | Wait timeout in seconds (default: `5`) |
| `--yes` / `--dry-run` | Batch safety: skip confirmation / preview only (see above) |

### `stop`
Stop a running process.

```bash
super stop <name|@group|id|all> [--wait] [--timeout N] [--force]
```

| Flag | Description |
| :--- | :--- |
| `--wait` | Poll until the program(s) reach `Stopped` |
| `--timeout N` | Wait timeout in seconds (default: `5`) |
| `--force` | Skip graceful shutdown and SIGKILL immediately |
| `--yes` / `--dry-run` | Batch safety: skip confirmation / preview only (see above) |

### `restart`
Restart a process.

```bash
super restart <name|@group|id|all> [--wait] [--wait-healthy] [--timeout N]
```

| Flag | Description |
| :--- | :--- |
| `--wait` | Poll until the program(s) reach `Running`/`Healthy` |
| `--wait-healthy` | Poll until the program(s) reach `Healthy` (readiness check passed). Mutually exclusive with `--wait` |
| `--timeout N` | Wait timeout in seconds (default: `5`) |
| `--yes` / `--dry-run` | Batch safety: skip confirmation / preview only (see above) |

### `signal`
Send a specific POSIX signal.

```bash
super signal <name|@group|id|all> --sig <SIGNAL>
```
*   **Signals**: `hup`, `int`, `term`, `kill`, `quit`, `usr1`, `usr2`.

## Observability

### `info`
Show detailed JSON/Table information about a specific program.

```bash
super info <name|id>
```

### `events`
Show the persisted lifecycle/exception event history for a program (crashes, OOM kills, backoff retries, recoveries, unexpected exits). Newest first.

```bash
super events <name|id>          # all recorded events
super events <name|id> --limit 10   # last 10 events
```

| Flag | Description |
| :--- | :--- |
| `--limit N` | Show only the N most recent events |

Events are recorded by `superd` to `data/events.json`. Every event carries a **Unix timestamp** (seconds, `ts` field in the API/JSON) — the table's `Time` column renders it as local time. Events are kept **newest per program, capped at 100 per program, oldest dropped** (across `superd` restarts). The `signal` column shows the terminating signal — `9` (`SIGKILL`) typically indicates a cgroup/kernel OOM kill under `resource_limits`. Backed by `GET /api/v1/programs/{id}/events`.

### `logs`
Read historical lines from disk and/or stream live output via WebSocket.

```bash
super logs <name|id>              # live stream (WebSocket)
super logs <name|id> --tail 200   # last 200 lines from disk
super logs <name|id> --tail 50 --follow   # tail then follow live
```

| Flag | Description |
| :--- | :--- |
| `--tail N` | Read last N lines from log files (`GET /api/v1/programs/{id}/logs`) |
| `--source` | `stdout` or `stderr` only |
| `--follow` | After `--tail`, keep streaming via WebSocket |

### `top`
Real-time monitoring interface (like `htop`). Polls `/api/v1/programs` every second and renders a full-screen table; navigate with arrow keys, quit with `q`.

```bash
super top
```

## System

### `reload`
Reload system configuration from `super.toml` (logging, includes, event hooks), or send **SIGHUP** to a running program when a target is given.

```bash
super reload                              # reload super.toml (no program restart)
super reload --wait [--timeout N]         # reload and wait for affected programs to become Healthy
super reload <name|@group|id|all>         # SIGHUP to program(s) — e.g. nginx config reload
```

| Flag | Description |
| :--- | :--- |
| `--wait` | Readiness-aware reload: after applying the new config, wait for every affected program to pass its health checks before reporting success. Prints the affected programs and exits non-zero if any is not ready in time |
| `--timeout N` | Readiness wait timeout in seconds (default: `30`) |
| `--yes` / `--dry-run` | Batch safety on the SIGHUP target: skip confirmation / preview only (see above) |

### `apply`
Apply a declarative stack configuration (JSON).

```bash
super apply <FILE>
```

### `export`
Export current state as a stack JSON.

```bash
super export
```

### `shutdown`
Gracefully shut down the Super daemon and all child processes (works for foreground and `--daemon` instances).

```bash
super shutdown
```

### `doctor`
One-shot diagnostics: config check (`super check` output), daemon health, license status, a **Verifying keys** summary line (same ids as [`super keyring`](#keyring)), and local `[server].daemon` / pidfile hints (systemd conflict, stale pidfile). See [Troubleshooting license verification](/docs/05-advanced-management/authentication#troubleshooting-license-verification).

```bash
super doctor
```

### `check`
Validate `super.toml` **without a running daemon**: TOML syntax, bind/port, log/data paths, licensed-mode requirements, `[include]` JSON stacks (`StackApplyRequest`, including the same program-body checks as create), and rejected leftovers (`[webhook]`, `[[program]]` tables that Super does not load). Include errors name the file and location: `path:line:col:` for JSON syntax, `path: services[i] (name=…): field:` for invalid services. Non-zero exit if any error is found. Use with [`super keyring`](#keyring) when diagnosing license verification failures.

```bash
super check
```

### `keyring`
List Ed25519 verifying key ids (`kid`) compiled into this `super` binary — one row per key, so multiple rotation keys are all visible. Each `kid` uses the `k_<8hex>` convention (first four bytes of the public key as hex). Suggested when a license fails with an unknown signing key or to compare a local build with an official release. See [Troubleshooting license verification](/docs/05-advanced-management/authentication#troubleshooting-license-verification).

> [!NOTE]
> This command exists to help diagnose **license verification for licensed deployments**. Pure OSS builds have no license, so the keyring output is irrelevant unless you run a Super Pro instance.

```bash
super keyring
super keyring --json
```

## Security (requires `security` plugin 💎)

When the `security` plugin is loaded, use the same `super` CLI:

```bash
# Bootstrap only (no Access Tokens yet), or after all tokens were revoked:
super login <auth_secret>          # save credentials to ~/.super/cli.json

# Day-to-day: use a generated token
super login sk-...
super logout                        # clear saved credentials (~/.super/cli.json)
super token list
super token create ci-bot --role operator
super token revoke <id>

# or pass token per invocation:
super --token sk-... list
export SUPER_TOKEN=sk-...
```

`auth_secret` stays usable by default; Admins may explicitly disable it after creating an Admin Access Token. See [Authentication](/docs/05-advanced-management/authentication#optional-disable-auth_secret).

Without the plugin, `super login` fails (404 on `/api/v1/auth/login`). OSS deployments without auth can use `super list` directly on localhost.

Alternative via curl:

```bash
# Bootstrap (login with auth_secret only when no tokens exist yet):
curl -X POST http://127.0.0.1:9002/api/v1/auth/login \
  -H "Authorization: Bearer <auth_secret>"

curl -X POST http://127.0.0.1:9002/api/v1/auth/tokens \
  -H "Authorization: Bearer <auth_secret>" \
  -H "Content-Type: application/json" \
  -d '{"name":"ci-bot","role":"operator"}'

curl -H "Authorization: Bearer sk-..." http://127.0.0.1:9002/api/v1/programs
```

See [Authentication](/docs/05-advanced-management/authentication) for details.
