---
title: "CLI Reference"
weight: 4
description: "Command-line arguments for the 'super' client."
---

The `super` binary is the primary way to interact with the daemon. Commands are grouped by category below; run `super --help` for inline help.

**Edition legend** (same convention as [Config reference — Edition legend](/docs/06-internals/config-reference/#edition-legend)):

| Mark | Meaning |
| :--- | :--- |
| **💎 Subscription** | Requires a valid `[license].key` and the matching licensed plugin (`security` for auth, `isolation` for resource limits). OSS builds/deployments don't have these. |
| *(no mark)* | Available in OSS (with or without plugins). |

**Global Flags:**

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `--server <URL>` / `-s` | string | `http://127.0.0.1:9002` | Override the server endpoint. Accepts an HTTP(S) URL or a Unix socket (`unix:///path/to/superd.sock`). Relative socket paths resolve under `SUPER_ROOT`; auto-discovery may prefer the socket (see below) |
| `--token <TOKEN>` 💎 | string | — | API token for authenticated daemons (falls back to `SUPER_TOKEN`). Requires the licensed `security` plugin |
| `--yes` / `-y` | bool | `false` | Skip batch confirmation prompts |
| `--dry-run` | bool | `false` | Preview which programs a batch operation would affect, without executing |

**Endpoint selection** (how `super` picks a daemon to talk to):

1. `--server <URL>` — highest priority; used for this command only.
2. `~/.super/cli.json` `server_url` — persisted by `super login <secret> [--url <URL>]`; all subsequent commands follow it.
3. **Auto-discovery** (only when neither of the above applies, i.e. no `--server` and no `~/.super/cli.json`): if `$SUPER_ROOT/run/superd.sock` (or `./run/superd.sock` without `SUPER_ROOT`) exists and is a real socket file, the CLI uses `unix://` there; otherwise it falls back to the default `http://127.0.0.1:9002`.

When the daemon listens on both TCP and a Unix socket, an unconfigured CLI therefore prefers the Unix socket automatically. To force TCP, pass `--server http://host:port` or run `super login <secret> http://host:port` to persist it. Auto-discovery only recognizes the conventional `run/superd.sock` path — a custom socket path or a non-default TCP port still needs an explicit `--server` (or a persisted login).

## Starting `superd` (daemon binary)

`superd` is separate from the `super` CLI. It reads `$SUPER_ROOT/conf/super.toml` and listens for API/CLI traffic.

| Mode | Command / config | When to use |
| :--- | :--- | :--- |
| Foreground (default) | `superd` or `superd --foreground` | systemd (`Type=simple`), Docker, debugging |
| Self-daemonize (Unix) | `superd --daemon` or `[server] daemon = true` | Bare metal without systemd; writes `$SUPER_ROOT/run/superd.pid` by default |

CLI overrides: `--daemon` / `--foreground` / `--pidfile <PATH>`. See [Config reference](/docs/06-internals/config-reference/#server) and [Installation — Systemd](/docs/01-getting-started/installation/#method-3-systemd-vm--bare-metal). Program control (`start` / `stop` / `shutdown`) is unchanged either way.

## Command index

### Runtime & Monitoring

| Command | Alias | Description |
| :--- | :--- | :--- |
| [`super list`](#list) | `ls` | List all managed programs with status, CPU, RAM, uptime |
| [`super info <name\|id>`](#info) | — | Detailed config / state for one program |
| [`super events <name\|id>`](#events) | — | Persisted event history (filterable) + stats |
| [`super logs <name\|id>`](#logs) | `log` | Stream or read program logs |
| [`super top`](#top) | — | Full-screen real-time monitoring UI |

### Lifecycle

| Command | Alias | Description |
| :--- | :--- | :--- |
| [`super add <command>`](#add) | — | Register a new program |
| [`super update <name\|id>`](#update) | — | Change an existing program's config |
| [`super start <target>`](#start) | — | Start a stopped process |
| [`super stop <target>`](#stop) | — | Stop a running process |
| [`super restart <target>`](#restart) | `rs` | Restart a process |
| [`super signal <target>`](#signal) | — | Send a POSIX signal |
| [`super remove <name\|@group\|id\|all>`](#remove-alias-rm) | `rm` | Remove a program |

### Config & Stack

| Command | Alias | Description |
| :--- | :--- | :--- |
| [`super reload [--wait]`](#reload) | — | Reload `super.toml`, or SIGHUP a program |
| [`super apply <file>`](#apply) | — | Apply a declarative stack (TOML default, JSON compatible) |
| [`super export`](#export) | — | Export current state as a stack (TOML default, `--format json` available) |
| [`super check`](#check) | — | Validate `super.toml` without a running daemon |

### System & Daemon

| Command | Alias | Description |
| :--- | :--- | :--- |
| [`super shutdown`](#shutdown) | — | Gracefully shut down the daemon and all children |
| [`super doctor`](#doctor) | — | One-shot diagnostics (config, daemon, license) |
| [`super keyring [--json]`](#keyring) | — | List embedded license verifying keys |

### Security (plugin 💎)

| Command | Alias | Description |
| :--- | :--- | :--- |
| [`super login <secret>`](#security-requires-security-plugin) | — | Save credentials to `~/.super/cli.json` |
| [`super logout`](#security-requires-security-plugin) | — | Clear saved credentials |
| [`super token list\|create\|revoke`](#security-requires-security-plugin) | — | Manage API access tokens |

## Runtime & Monitoring

### `list`
List all managed programs and their status.

```bash
super list
```

### `info`
Show detailed JSON/Table information about a specific program.

```bash
super info <name|id>
```

### `events`
Show the persisted event history for a program — **all** lifecycle events are recorded (crashes, OOM kills, backoff retries, recoveries, exits, cron runs, system startup/shutdown), not just anomalies. Newest first. For storage, retention, and workflow guidance, see [Event History](/docs/03-orchestration/events/history).

```bash
super events <name|id>                    # all recorded events
super events <name|id> --limit 10         # last 10 events
super events <name|id> --type process_fatal   # only fatal events
super events <name|id> --exit-code 1      # only runs that exited with code 1
super events <name|id> --q "oom"          # free-text match on the message
super events <name|id> --from 1735689600 --to 1735776000   # time window (Unix seconds)
super events <name|id> --stats            # retention statistics instead of the list
```

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `--limit N` | int | all events | Show only the N most recent events |
| `--from TS` | int | — | Inclusive start of the time window (Unix seconds) |
| `--to TS` | int | — | Inclusive end of the time window (Unix seconds) |
| `--type NAME` | string | — | Exact event type (e.g. `process_fatal`, `cron_exit`, `health_restart`) |
| `--exit-code N` | int | — | Exact exit code |
| `--q TEXT` | string | — | Free-text match on the event message |
| `--stats` | flag | — | Show counts by event type and the retained time range |

Events are persisted by `superd` to a SQLite database (default `data/events.db`) — see [Event History](/docs/03-orchestration/events/history) for storage, retention (`events_keep_days`), and the full event catalog. Every event carries a **Unix timestamp** (`ts`, seconds) and a millisecond-precision `ts_ms`; the table's `Time` column renders it as local time. The `signal` column shows the terminating signal — `9` (`SIGKILL`) typically indicates a cgroup/kernel OOM kill under `resource_limits`. Backed by `GET /api/v1/programs/{id}/events` (plus `GET /api/v1/events` and `GET /api/v1/events/stats`).

### `logs`
Read historical lines from disk and/or stream live output via WebSocket.

```bash
super logs <name|id>              # live stream (WebSocket)
super logs <name|id> --tail 200   # last 200 lines from disk
super logs <name|id> --tail 50 --follow   # tail then follow live
```

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `--tail N` | int | — (live only) | Read last N lines from log files (`GET /api/v1/programs/{id}/logs`) |
| `--source` | enum | both | Log stream: `stdout` or `stderr` only |
| `--follow` | bool | `false` | After `--tail`, keep streaming via WebSocket |

### `top`
Real-time monitoring interface (like `htop`). Polls `/api/v1/programs` every second and renders a full-screen table; navigate with arrow keys, quit with `q`.

```bash
super top
```

## Lifecycle

### Target syntax

Control commands take a single target, written PM2-style as a union:

```bash
super <start|stop|restart|remove> <name|@group|id|all>
```

*   `<name>` — exact program name (like PM2's `app_name`).
*   `@<group>` — every program in that group (like PM2's `namespace`).
*   `<id>` — program UUID; unambiguous prefixes are accepted.
*   `all` — every managed program (PM2's `'all'`).

Super has no `json_conf` target — declarative batches go through a stack file instead (`super apply <file>`, TOML default / JSON compatible). A target that matches several programs is rejected as ambiguous, with the candidates listed.

Batch targets are protected by three safety knobs (global flags, accepted on any control command):

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `--yes` / `-y` | bool | `false` | Skip the interactive confirmation prompt. Batch operations (`@group` / `all`) ask for confirmation before touching more than one program; `--yes` bypasses it for scripts |
| `--dry-run` | bool | `false` | Show which programs the operation would affect (preview list) and exit without executing anything |

Without `--yes`, a batch operation on more than one program prints the affected program list and asks `[y/N]`; answering anything other than `y`/`yes` aborts. Single-target operations never prompt.

### `add`
Register a new program without a config file.

```bash
super add <COMMAND> [ARGS...] [FLAGS]
```

**Flags:**

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `--name <NAME>` / `-n` | string | binary name | Custom program name |
| `--autostart` | bool | `true` | Start the program when the daemon starts |
| `--cwd <DIR>` | string | daemon cwd | Working directory |
| `--env <KEY=VAL>` / `-e` | string[] | — | Set environment variables (repeatable) |
| `--env-file <PATH>` | path | — | Load environment variables from a file at spawn time |
| `--user <USER>` | string | — | Run as a specific user (requires root) |
| `--group <GROUP>` | string | — | Group name for organization (addressable as `@<group>`) |
| `--numprocs <N>` | int | `1` | Spawn N process instances |
| `--process-name <TPL>` | string | `{name}-{num}` | Process name template for multiple instances (e.g. `worker-{num}`) |
| `--autorestart <POLICY>` | enum | `unexpected` | Auto-restart policy: `unexpected`, `true`, or `false` |
| `--exitcodes <N,N>` | int[] | `0` | Comma-separated exit codes considered successful |
| `--startsecs <N>` | int | `10` | Seconds before exit counts as stable start |
| `--stopsecs <N>` | int | `10` | Seconds to wait after SIGTERM before SIGKILL (server `shutdown_timeout` when omitted) |
| `--cron <EXPR>` | string | — | Cron expression for scheduled tasks (see [Scheduled Tasks](/docs/02-essentials/scheduled-tasks)) |
| `--on-overlap <POLICY>` | enum | `skip` | Cron overlap policy when the previous run is still active: `skip` / `queue` / `kill` |
| `--catchup <POLICY>` | enum | `skip` | Policy for cron slots missed while the daemon was down: `skip` / `latest` / `all` |
| `--jitter <SECS>` | int | `0` | Max random delay (seconds) before each cron trigger to spread load |
| `--max-concurrent <N>` | int | `1` | Max overlapping cron runs allowed at once (1–64) |
| `--max-queued <N>` | int | `100` | Cap on queued cron firings when at `max_concurrent` (`0` means default) |
| `--cpu <CORES>` 💎 | float | — | CPU quota in cores (e.g. `1.5`; `isolation` plugin, Linux only) |
| `--memory <MB>` 💎 | int | — | Memory hard limit in **MB** (`isolation` plugin, Linux only) |
| `--memory-warn-percent <PCT>` 💎 | int | `80` | Pre-kill warning threshold as % of limit (`0` off; `isolation` plugin, Linux only) |
| `--memory-warn-headroom <MB>` 💎 | int | `0` | Warn within this many MB of the limit (`0` off) |
| `--memory-high <MB>` 💎 | int | `0` | Kernel soft limit in MB — throttles before OOM-kill (`0` off, opt-in) |

### `update`
Update configuration for an existing program.

```bash
super update <name|id> [FLAGS]
```

**Flags:**

Every flag is optional — omitted flags keep the program's current value.

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `--command <CMD>` | string | keep | New command to execute |
| `--args <ARG...>` | string[] | keep | New command arguments |
| `--cwd <DIR>` | string | keep | Working directory |
| `--user <USER>` | string | keep | Run as a specific user |
| `--group <GROUP>` | string | keep | Group name for organization |
| `--env <KEY=VAL>` / `-e` | string[] | keep | Replace environment variables |
| `--env-file <PATH>` | path | keep | Env file path (`""` clears) |
| `--autostart` | bool | keep | Auto-start on daemon start |
| `--retry-limit <N>` | int | keep | Auto-restart retry limit |
| `--autorestart <POLICY>` | enum | keep | Auto-restart policy: `unexpected` / `true` / `false` |
| `--exitcodes <N,N>` | int[] | keep | Comma-separated successful exit codes |
| `--startsecs <N>` | int | keep | Seconds before exit counts as stable start |
| `--stopsecs <N>` | int | keep | Seconds to wait after SIGTERM before SIGKILL |
| `--no-health-check` | bool | `false` | Remove the health check configuration |
| `--cron <EXPR>` | string | keep | Cron expression for scheduled tasks |
| `--on-overlap <POLICY>` | enum | keep | Overlap policy: `skip` / `queue` / `kill` |
| `--catchup <POLICY>` | enum | keep | Catchup policy: `skip` / `latest` / `all` |
| `--jitter <SECS>` | int | keep | Max random delay before each cron trigger |
| `--max-concurrent <N>` | int | keep | Max overlapping cron runs allowed at once |
| `--max-queued <N>` | int | keep | Cap on queued cron firings |
| `--artifact-url <URL>` | string | keep | OTA download URL (triggers transactional update when checksum changes) |
| `--artifact-sha256 <HEX>` | string | keep | Expected SHA256 digest of the downloaded bytes |
| `--artifact-destination <PATH>` | path | keep | **Absolute path** where the binary lives (e.g. `/usr/local/bin/my-app`); required on first OTA setup, omit if unchanged |
| `--artifact-extract` | bool | `false` | Unpack `.tar.gz` / `.tgz` / `.tar` / `.zip` before swap |
| `--artifact-restart-policy <POLICY>` | string | `immediate` | `immediate`, `manual`, `signal`, or `signal:<hup\|int\|term\|quit\|usr1\|usr2>`. **`signal*` requires the program to already have an enabled `health_check`.** |
| `--artifact-download-timeout <SECS>` | int | `60` | Max seconds for this OTA HTTP download. `0` disables the overall transfer deadline (connect still times out at 10s). |
| `--artifact-verify-timeout <SECS>` | int | `60` | Post-swap health window before auto-rollback. `0` disables. |
| `--cpu <CORES>` 💎 | float | keep | CPU quota in cores (`isolation` plugin, Linux only) |
| `--memory <MB>` 💎 | int | keep | Memory hard limit in MB (`isolation` plugin, Linux only) |
| `--memory-warn-percent <PCT>` 💎 | int | keep | Pre-kill warning threshold as % of limit (`0` off) |
| `--memory-warn-headroom <MB>` 💎 | int | keep | Warn within this many MB of the limit (`0` off) |
| `--memory-high <MB>` 💎 | int | keep | Kernel soft limit in MB (`0` off, opt-in) |

Full flow: [Atomic OTA Updates](/docs/03-orchestration/ota-updates). Artifact schema (same fields on API / stack / dashboard): [Config reference — `artifact`](/docs/06-internals/config-reference#artifact). Scheduled-task and licensed (`isolation` plugin) flags behave as in [`super add`](#add); licensed flags warn if the plugin is not loaded.

### `start`
Start a stopped process.

```bash
super start <name|@group|id|all> [--wait] [--wait-healthy] [--timeout N]
```

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `--wait` | bool | `false` | Poll until the program(s) reach `Running`/`Healthy` |
| `--wait-healthy` | bool | `false` | Poll until the program(s) reach `Healthy` (readiness check passed). Mutually exclusive with `--wait` |
| `--timeout N` | int | `5` | Wait timeout in seconds |
| `--yes` / `-y` | bool | `false` | Batch safety: skip confirmation (see above) |
| `--dry-run` | bool | `false` | Batch safety: preview only (see above) |

### `stop`
Stop a running process.

```bash
super stop <name|@group|id|all> [--wait] [--timeout N] [--force]
```

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `--wait` | bool | `false` | Poll until the program(s) reach `Stopped` |
| `--timeout N` | int | `5` | Wait timeout in seconds |
| `--force` | bool | `false` | Skip graceful shutdown and SIGKILL immediately |
| `--yes` / `-y` | bool | `false` | Batch safety: skip confirmation (see above) |
| `--dry-run` | bool | `false` | Batch safety: preview only (see above) |

### `restart`
Restart a process.

```bash
super restart <name|@group|id|all> [--wait] [--wait-healthy] [--timeout N]
```

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `--wait` | bool | `false` | Poll until the program(s) reach `Running`/`Healthy` |
| `--wait-healthy` | bool | `false` | Poll until the program(s) reach `Healthy` (readiness check passed). Mutually exclusive with `--wait` |
| `--timeout N` | int | `5` | Wait timeout in seconds |
| `--yes` / `-y` | bool | `false` | Batch safety: skip confirmation (see above) |
| `--dry-run` | bool | `false` | Batch safety: preview only (see above) |

### `signal`
Send a specific POSIX signal.

```bash
super signal <name|@group|id|all> --sig <SIGNAL>
```

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `--sig <SIGNAL>` | enum | `hup` | Signal to send: `hup`, `int`, `term`, `kill`, `quit`, `usr1`, `usr2` |

### `remove` (alias `rm`)
Remove a program configuration. It must be stopped first (see [Process Operations](#lifecycle)).

```bash
super remove <name|@group|id|all>
```

## Config & Stack

### `reload`
Reload system configuration from `super.toml` (logging, includes, event hooks), or send **SIGHUP** to a running program when a target is given.

```bash
super reload                              # reload super.toml (no program restart)
super reload --wait [--timeout N]         # reload and wait for affected programs to become Healthy
super reload <name|@group|id|all>         # SIGHUP to program(s) — e.g. nginx config reload
```

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `--wait` | bool | `false` | Readiness-aware reload: after applying the new config, wait for every affected program to pass its health checks before reporting success. Prints the affected programs and exits non-zero if any is not ready in time |
| `--timeout N` | int | `30` | Readiness wait timeout in seconds |
| `--yes` / `-y` | bool | `false` | Batch safety on the SIGHUP target: skip confirmation (see above) |
| `--dry-run` | bool | `false` | Batch safety on the SIGHUP target: preview only (see above) |

### `apply`
Apply a declarative stack configuration. Stack files are **TOML by default** (`.toml` or no extension); legacy JSON (`.json`) stacks keep working.

```bash
super apply <FILE>
```

### `export`
Export current state as a stack file. **Defaults to TOML** (the default stack format, round-trips cleanly with `super apply` / `[include]`); `--format json` keeps the legacy JSON shape for tooling that expects it.

```bash
super export [--format toml|json]
```

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `--format` | `toml` \| `json` | `toml` | Output format. `toml` produces the default stack format (inline/nested tables); `json` keeps the legacy shape for tooling that expects it. |

### `check`
Validate `super.toml` **without a running daemon**: TOML syntax, bind/port, log/data paths, licensed-mode requirements, `[include]` stacks (TOML default, JSON compatible — same program-body checks as create), and rejected leftovers (`[webhook]`, `[[program]]` tables that Super does not load). Include errors name the file and location: `path:line:col:` for TOML/JSON syntax, `path: services[i] (name=…): field:` for invalid services. Non-zero exit if any error is found. Use with [`super keyring`](#keyring) when diagnosing license verification failures.

```bash
super check
```

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `--file <PATH>` / `-f` | path | `./conf/super.toml` (or `/etc/super/super.toml`) | Path to the config file to validate |

## System & Daemon

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

### `keyring`
List Ed25519 verifying key ids (`kid`) compiled into this `super` binary — one row per key, so multiple rotation keys are all visible. Each `kid` uses the `k_<8hex>` convention (first four bytes of the public key as hex). Suggested when a license fails with an unknown signing key or to compare a local build with an official release. See [Troubleshooting license verification](/docs/05-advanced-management/authentication#troubleshooting-license-verification).

> [!NOTE]
> This command exists to help diagnose **license verification for licensed deployments**. Pure OSS builds have no license, so the keyring output is irrelevant unless you run a licensed instance.

```bash
super keyring
super keyring --json
```

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `--json` | bool | `false` | Output JSON (for scripts / monitoring) |

## Security (requires `security` plugin)

> 💎 **Subscription.** The `security` plugin is bundled with every license and gates API authentication — `super login` / `super logout` / `super token` and the `--token` global flag require it. OSS deployments have no auth and use `super list` directly on localhost.

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

| Flag | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `super login <secret> --url <URL>` 💎 | string | configured server | Server URL saved with the credentials; pins subsequent commands to that endpoint |
| `super token create --role <ROLE>` 💎 | enum | `operator` | Access token role: `viewer`, `operator`, or `admin` |

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

## Environment variables

`superd` and the `super` CLI read a small set of public environment variables (`SUPER_ROOT`, `SUPER_TOKEN`, `SUPER_LICENSE`, `SUPER_LICENSE_STRICT`) that override or configure daemon behavior without touching `super.toml`. See [Environment Variables](/docs/06-internals/environment-variables).
