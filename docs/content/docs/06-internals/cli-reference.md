---
title: "CLI Reference"
weight: 4
description: "Command-line arguments for the 'super' client."
---

The `super` binary is the primary way to interact with the daemon.

**Global Flags:**
*   `--server <URL>`: Override the server URL (default: `http://127.0.0.1:9002`).

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
*   `--name <NAME>`: Custom name (defaults to binary name).
*   `--autostart`: Enable autostart (default: true).
*   `--cwd <DIR>`: Working directory.
*   `--env <KEY=VAL>`: Set environment variables (can be used multiple times).
*   `--env-file <PATH>`: Load environment variables from a file at spawn time.
*   `--user <USER>`: Run as specific user.
*   `--numprocs <N>`: Spawn N instances.

### `update`
Update configuration for an existing program.

```bash
super update <TARGET> [FLAGS]
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
*   Licensed (`isolation` plugin): `--cpu`, `--memory` (Linux only; warns if plugin not loaded).

### `rm` (or `remove`)
Remove a program configuration. It must be stopped first (see [Process Operations](#process-operations)).

```bash
super rm <name|@group|id|all>
```

## Process Operations

Control commands take a single target, written PM2-style as a union:

```bash
super <start|stop|restart|rm> <name|@group|id|all>
```

*   `<name>` — exact program name (like PM2's `app_name`).
*   `@<group>` — every program in that group (like PM2's `namespace`).
*   `<id>` — program UUID; unambiguous prefixes are accepted.
*   `all` — every managed program (PM2's `'all'`).

Super has no `json_conf` target — declarative batches go through a stack file instead (`super apply <file>`). A target that matches several programs is rejected as ambiguous, with the candidates listed.

### `start`
Start a stopped process.

```bash
super start <name|@group|id|all> [--wait] [--timeout N]
```

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

### `restart`
Restart a process.

```bash
super restart <name|@group|id|all> [--wait] [--timeout N]
```

### `signal`
Send a specific POSIX signal.

```bash
super signal <TARGET> --sig <SIGNAL>
```
*   **Signals**: `hup`, `int`, `term`, `kill`, `quit`, `usr1`, `usr2`.

## Observability

### `info`
Show detailed JSON/Table information about a specific program.

```bash
super info <TARGET>
```

### `logs`
Read historical lines from disk and/or stream live output via WebSocket.

```bash
super logs <TARGET>              # live stream (WebSocket)
super logs <TARGET> --tail 200   # last 200 lines from disk
super logs <TARGET> --tail 50 --follow   # tail then follow live
```

| Flag | Description |
| :--- | :--- |
| `--tail N` | Read last N lines from log files (`GET /api/v1/programs/{id}/logs`) |
| `--source` | `stdout` or `stderr` only |
| `--follow` | After `--tail`, keep streaming via WebSocket |

## System

### `reload`
Reload system configuration from `super.toml` (logging, includes, event hooks), or send **SIGHUP** to a running program when a target is given.

```bash
super reload              # reload super.toml (no program restart)
super reload <TARGET>     # SIGHUP to program(s) — e.g. nginx config reload
```

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

> **Super Pro / subscription only:** this command exists to help diagnose **license verification for licensed deployments**. Pure OSS builds have no license, so the keyring output is irrelevant unless you run a Super Pro instance.

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
