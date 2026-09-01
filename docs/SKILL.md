# Project Super — AI Skill Reference

A compact reference so an AI assistant can help configure, operate, and
troubleshoot **Project Super** (a lightweight, API-first process manager in
Rust). Drop this file into your project (e.g. reference it from `CLAUDE.md`,
`.cursor/rules`, or paste it into your AI context).

> **Authoritative sources** — this file is a quick-start; when in doubt, defer
> to the [online docs](https://super.docs.sconts.com/docs/) or run
> `super --help` / `super check`. Command reference: [CLI Reference](https://super.docs.sconts.com/docs/06-internals/cli-reference/). Config schema: [Config Reference](https://super.docs.sconts.com/docs/06-internals/config-reference/).

---

## What Super Is

Super (`superd` daemon + `super` CLI) runs and supervises long-running services —
any language: Node, Python, Go, Rust, shell. Key traits:

- **Declarative programs** declared in **JSON stack files** (`conf/conf.d/*.json`),
  applied at daemon start and on `super reload`. NOT in `super.toml`.
- **API-first**: everything the CLI does is a REST call to `superd`
  (default `http://127.0.0.1:9002`, optional Unix socket).
- **Fail-closed security**: refuses non-loopback bind unless explicitly opted in
  or the licensed `security` plugin is active; licensed startup requires a valid
  `[license].key`, the `security` plugin, and `auth_secret`.
- **Two restart concepts** that are independent:
  - `autostart` — start when `superd` boots.
  - `autorestart` — restart **after exit** (`unexpected` / `true` / `false`).

---

## Quick Start

```bash
# Install (Linux / macOS / FreeBSD)
curl -fsSL https://raw.githubusercontent.com/schiplat/super/master/install.sh | sh

# Instance root: conf/, data/, logs/, run/, plugins/
export SUPER_ROOT=/opt/super

# Start daemon (foreground; systemd/Docker friendly)
superd

# Register a program (no config file needed)
super add ./api-server --name api --port 8080

# Every-day ops
super list              # alias: ls
super logs api --tail 50 --follow
super restart api       # alias: rs
super info api          # detail + status
super top               # htop-style TUI
```

---

## Everyday Commands

| Task | Command |
| :--- | :--- |
| List all programs | `super list` (`ls`) |
| One program detail | `super info <name\|id>` |
| Stream / read logs | `super logs <name> [--tail N] [--follow]` |
| Register a program | `super add <COMMAND> [ARGS...] [--name N] [--env K=V]...` |
| Change config | `super update <name> [FLAGS]` (omitted flags keep current value) |
| Start / stop / restart | `super start\|stop\|restart <name\|@group\|id\|all>` |
| Send a signal | `super signal <target> --sig hup\|int\|term\|kill\|quit\|usr1\|usr2` |
| Remove | `super remove <target>` (`rm`) — must be stopped first |
| Config reload | `super reload` (system config) or `super reload <target>` (SIGHUP) |
| Validate config offline | `super check` |
| Declarative stack | `super apply <file>` / `super export` |
| Shutdown daemon + children | `super shutdown` |
| Diagnostics | `super doctor` |

**Targets**: `<name>` exact name · `@<group>` every program in a group · `<id>`
UUID (unambiguous prefix ok) · `all` every program. A match on several programs
is rejected as ambiguous.

**Batch safety**: operations touching >1 program (`@group` / `all`) prompt for
confirmation. `--yes` / `-y` skips it; `--dry-run` prints the affected list and
exits without executing.

**Waiting**: `super start|restart --wait` polls until `Running`/`Healthy`;
`--wait-healthy` requires `Healthy` (health check passed). `--timeout N` seconds.

---

## Config Model

### `super.toml` — daemon settings only

```toml
[server]
host = "127.0.0.1"        # fail-closed: non-loopback needs explicit opt-in
port = 9002
# socket = "run/superd.sock"   # optional Unix socket (default 0600, owner-only)
# socket_only = true           # disable TCP entirely

[include]
files = ["conf/conf.d/*.json"]   # program stacks load from here

[child_logging]
driver = "file"        # file | stdout
timestamp = "local"    # local | utc | none
```

### Program stacks — `conf/conf.d/*.json`

```json
{
  "services": [
    {
      "name": "api",
      "command": "./api-server",
      "args": ["--port", "8080"],
      "cwd": "/srv/api",
      "env": { "NODE_ENV": "production" },
      "env_file": "/etc/secrets/prod.env",
      "numprocs": 1,
      "process_name": "{name}-{num}",
      "autostart": true,
      "autorestart": "unexpected",
      "exitcodes": [0],
      "startsecs": 10,
      "retry_limit": 3,
      "stopsecs": 10,
      "depends_on": ["db"]
    }
  ]
}
```

Key fields:

- **Identity & execution**: `name` (required), `command` (required), `args`,
  `env`, `env_file`, `cwd`, `user` (root only), `group` (batch target `@group`),
  `numprocs`, `process_name` (template, `{num}`).
- **Restart**: `autostart`, `autorestart` (`unexpected`|`true`|`false`),
  `exitcodes`, `retry_limit` (max consecutive crashes before `Fatal`),
  `startsecs` (stable uptime resets the retry counter), `stopsecs`
  (grace after SIGTERM, then SIGKILL; TOML alias `stopwaitsecs`), `priority`.
- **Orchestration**: `depends_on` (must be **Healthy** first), cron fields below.
- **Hooks**: `hooks.pre_start` / `post_start` / `pre_stop` / `post_stop` (shell).
- **Health check**: `health_check` block, see below.
- **Licensed only** 💎: `resource_limits.cpu_quota` (cores) / `memory_limit`
  (MB) / `memory_warn_percent` / `memory_warn_headroom` / `memory_high`
  (`isolation` plugin, Linux cgroups v2).

### Cron scheduling

```json
{
  "name": "backup",
  "command": "./backup.sh",
  "cron": "0 3 * * * *",
  "on_overlap": "skip",     // skip | queue | kill
  "catchup": "skip",        // skip | latest | all
  "jitter_sec": 0,
  "max_concurrent": 1,      // up to 64
  "max_queued": 100         // cap on queue; full → dropped + queue_full event
}
```

### Health checks

```json
"health_check": {
  "type": "http",                    // tcp | http | exec
  "url": "http://127.0.0.1:8080/health",
  "method": "GET",                   // http only
  "interval_secs": 5,
  "timeout_secs": 5,
  "start_period_secs": 1,            // grace before first probe
  "max_failures": 3                  // consecutive failures → auto-restart
}
```

`max_failures` consecutive failures trigger an automatic restart (a
`health_restart` event); the counter resets once healthy, and `retry_limit`
bounds total health restarts before `Fatal`.

---

## Super Pro (licensed plugins) 💎

**Super Pro** is not a separate binary or fork — the OSS `superd` / `super`
binaries stay identical. Optional **signed plugin libraries** are loaded at
runtime after license verification to enable commercial features. See the
[Feature Matrix](https://super.docs.sconts.com/docs/07-editions/feature-matrix/).

### Enabling Pro

Same binaries, just add three things:

```toml
# 1. A valid signed subscription key
[license]
key = "eyJjbGFpbXMiOnsiaXNzdWVkX3RvIjoi..."
# optional: fail startup loudly on invalid key
strict = true
```

```toml
# 2. Root-level auth_secret (required for licensed startup)
auth_secret = "change-me-before-bootstrap"
```

```
# 3. Plugin libraries in $SUPER_ROOT/plugins/ (no "lib" prefix)
$SUPER_ROOT/plugins/security.dylib
$SUPER_ROOT/plugins/notify.dylib
$SUPER_ROOT/plugins/isolation.dylib
$SUPER_ROOT/plugins/ui.dylib
```

**Licensed startup hard-requires** the `security` plugin + `auth_secret` + a
valid `[license].key` — otherwise `superd` refuses to start. Container
injection: `SUPER_LICENSE` env var + `SUPER_LICENSE_STRICT=1` instead of
`[license].key` on disk.

### What each plugin provides

| Plugin | Feature | Notes |
| :--- | :--- | :--- |
| `security` | API auth (tokens), RBAC roles, audit log | **Required** for licensed startup; included with every subscription |
| `ui` | Web dashboard at `/` | Requires `security` for licensed startup |
| `notify` | IM/Webhook notifications, `conf/notify.toml` | Hot-reloadable channels; distinct from OSS `[[event_hooks]]` |
| `isolation` | cgroups v2 CPU/memory limits (`resource_limits`) | **Linux only**, privileged |

### Pro-only CLI & API surface

- `super login <secret>` / `super logout` / `super token list|create|revoke`
  (bootstrap with `auth_secret`, day-to-day with `sk-...` tokens).
- Global `--token <TOKEN>` / `SUPER_TOKEN` for authenticated requests.
- `super add|update --cpu --memory --memory-warn-percent --memory-warn-headroom
  --memory-high` and stack `resource_limits` (requires `isolation`, Linux).
- API auth: `Authorization: Bearer <auth_secret>` (bootstrap) or
  `Authorization: Bearer sk-...` (tokens).

### What stays OSS (no plugin needed)

Cron scheduling (`cron` / `on_overlap` / `catchup` / `jitter_sec` /
`max_concurrent` / `max_queued`), health checks & tuning, OTA updates, event
hooks (`[[event_hooks]]` command/webhook), `super top`, `super logs`,
dependencies (`depends_on`), `numprocs` scaling.

---

## Environment Variables

| Variable | Purpose |
| :--- | :--- |
| `SUPER_ROOT` | Instance root (`conf/`, `data/`, `logs/`, `run/`, `plugins/`) |
| `SUPER_TOKEN` | CLI auth token (equivalent to `--token`) |
| `SUPER_LICENSE` | Base64 signed license key (overrides `[license].key`) |
| `SUPER_LICENSE_STRICT` | `1` = strict license verify; invalid key refuses startup |

**Injected into children/hooks** (set by the daemon, do not set yourself):
`SUPER_ID`, `SUPER_NAME`, `SUPER_HOSTNAME`, `SUPER_GROUP`, `SUPER_PID`,
`SUPER_EXIT_CODE`, `SUPER_UPTIME_SECS`, `SUPER_PROCESS_NUM`, `SUPER_PROCESS_TOTAL`,
and for event hooks `SUPER_EVENT`, `SUPER_USAGE_BYTES`, `SUPER_LIMIT_BYTES`,
`SUPER_WARN_BYTES`, `SUPER_RETRY_COUNT`.

---

## Common Operations (do it right)

### Logs

```bash
super logs api                      # live stream (WebSocket)
super logs api --tail 200           # last 200 lines from disk
super logs api --tail 50 --follow   # tail then follow
super logs api --source stderr      # stdout | stderr | both
```

### Rolling update with health gate

```bash
super reload --wait [--timeout 30]          # reload super.toml, wait for healthy
super restart api --wait-healthy --timeout 30
```

### Zero-downtime SIGHUP (e.g. nginx)

```bash
super reload nginx
```

### Scheduled task overlap policies

- `skip` (default): drop the tick while the previous run is active.
- `queue`: start the run as soon as the current instance exits.
- `kill`: terminate the running instance, then start the new run.
- `max_concurrent` raises the ceiling before `on_overlap` applies.

### OTA (self-update a program)

```bash
super update api \
  --artifact-url https://example.com/api.tar.gz \
  --artifact-sha256 <hex> \
  --artifact-destination /usr/local/bin/api \
  [--artifact-extract]
```

On a bad new version (fails health within `[server].ota_verify_timeout`),
the daemon rolls back automatically.

---

## Troubleshooting

1. **`super` can't reach the daemon**
   - Default endpoint `http://127.0.0.1:9002`. Check the socket: the CLI prefers
     `$SUPER_ROOT/run/superd.sock` when present — force TCP with
     `super --server http://127.0.0.1:9002 ...` or pin via
     `super login <secret> --url <URL>`.
   - Daemon alive? `super doctor`; check `$SUPER_ROOT/logs/app.log`.

2. **Daemon refuses to start on a non-loopback bind**
   - OSS: explicitly set `allow_insecure_public_bind = true` (or load the
     `security` plugin). This is by design (fail-closed).
   - Licensed: startup requires the `security` plugin + `auth_secret` + valid
     `[license].key`. See `super doctor` / `super keyring` for verify issues.

3. **License verification fails**
   - `super doctor` prints the verifying-key summary; `super keyring` lists
     `kid`s compiled into the binary. The license's `kid` must match.
   - Containers: inject via `SUPER_LICENSE` instead of `[license].key`.
   - Set `SUPER_LICENSE_STRICT=1` to make invalid keys fail loudly instead of
     silently degrading to OSS.
   - Licensed startup needs **all three**: valid `[license].key`, the `security`
     plugin in `$SUPER_ROOT/plugins/`, and `auth_secret` in `super.toml`.

3b. **API returns 401 / RBAC denies**
   - Licensed daemon: bootstrap with `super login <auth_secret>`, then create an
     access token (`super token create <name> --role admin|operator|viewer`) and
     use `sk-...` / `SUPER_TOKEN` day-to-day.
   - OSS daemon: no auth by default — a 401 means you hit a licensed daemon.

3c. **Web dashboard / notifications / resource limits not working**
   - Confirm the plugin file exists in `$SUPER_ROOT/plugins/` (no `lib` prefix),
     the license grants the plugin id, and `super doctor` reports it loaded.
   - `notify` reads `conf/notify.toml`; `isolation` is **Linux-only** cgroups v2.
   - OSS builds ignore unknown licensed fields — a missing feature usually means
     the plugin did not load.

4. **Program keeps restarting (restart loop)**
   - `super events <name>` shows `process_fatal` / backoff / OOM history
     (capped at 100 events, oldest dropped).
   - Check `autorestart` + `exitcodes`: an exit code not in `exitcodes` (and not
     `0`) counts as a crash under `unexpected`.
   - `retry_limit` consecutive crashes → `Fatal` (no more auto-restarts).
   - `[server] flapping_window` / `flapping_threshold` guard the daemon side.

5. **Health check restarts unexpectedly**
   - `max_failures` consecutive probe failures → auto-restart. Tune
     `timeout_secs` (probe may be too short), `start_period_secs` (slow start),
     `interval_secs` (probe cadence). `max_failures = 0` disables auto-restart.

6. **Cron jobs not firing**
   - Cron programs skip boot-time autostart by design.
   - `on_overlap = "skip"` drops ticks while the previous run is active; check
     for long-running instances (`super list`).
   - `catchup = "skip"` drops slots missed while the daemon was down.
   - `queue_full` events mean `max_queued` was exceeded.

7. **Secrets exposure**
   - Prefer `env_file` (`chmod 600`) over `-e KEY=VAL`: values never touch
     `snapshot.json`. Note that all env values reach the child via `execve`
     env and are visible in `/proc/<pid>/environ` to same-UID processes —
     display masking (API/CLI) is a UI layer only. See `SECURITY.md`.

---

## Key Semantic Traps (AI, pay attention)

- **`autostart` ≠ `autorestart`.** Boot start vs crash recovery are independent.
  `autostart = false` + `autorestart = "true"` = manually started, self-healing.
- **Programs are NOT in `super.toml`.** `[[programs]]` tables are ignored (and
  rejected by `super check`). Use stack JSON files, `super add`, or the API.
- **`super check` is offline** — validates `super.toml` + included stacks without
  a running daemon. `--file` overrides the config path.
- **SIGKILL (`signal: 9`) in events usually means a cgroup/OOM kill** under
  `resource_limits`, not a manual kill.
- **`--wait` vs `--wait-healthy`**: the former accepts `Running`, the latter
  requires `Healthy`. Mutually exclusive.
- **Health types**: `tcp` needs `host`+`port`; `http` needs `url` (+`method`);
  `exec` needs `command`. Wrong shape = probe never runs.
- **Pro plugins are runtime-loaded, not compiled in.** Missing features usually
  mean the plugin didn't load (wrong filename / missing license grant / missing
  `auth_secret`) — not a missing binary. `super doctor` is the first stop.
- **`security` is mandatory in licensed mode.** You cannot run licensed startup
  with `notify`/`ui`/`isolation` alone; `security.so` + `auth_secret` are always
  required.

---

## Docs map

- [Getting started](https://super.docs.sconts.com/docs/01-getting-started/)
- [Configuration](https://super.docs.sconts.com/docs/02-essentials/configuration/) · [Environment & Secrets](https://super.docs.sconts.com/docs/02-essentials/environment-secrets/)
- [Health checks](https://super.docs.sconts.com/docs/03-orchestration/health-checks/) · [Scheduled tasks](https://super.docs.sconts.com/docs/02-essentials/scheduled-tasks/) · [OTA updates](https://super.docs.sconts.com/docs/03-orchestration/ota-updates)
- [System events](https://super.docs.sconts.com/docs/03-orchestration/system-events/) · [Lifecycle hooks](https://super.docs.sconts.com/docs/03-orchestration/lifecycle-hooks/)
- [CLI reference](https://super.docs.sconts.com/docs/06-internals/cli-reference/) · [Config reference](https://super.docs.sconts.com/docs/06-internals/config-reference/) · [Environment variables](https://super.docs.sconts.com/docs/06-internals/environment-variables/)
- [Feature matrix](https://super.docs.sconts.com/docs/07-editions/feature-matrix/) · [Changelog](https://super.docs.sconts.com/docs/08-changelog/)
- **Super Pro**: [Authentication](https://super.docs.sconts.com/docs/05-advanced-management/authentication/) · [Access Control (RBAC)](https://super.docs.sconts.com/docs/05-advanced-management/access-control/) · [Operation Audit](https://super.docs.sconts.com/docs/05-advanced-management/operation-audit/) · [Resource Isolation](https://super.docs.sconts.com/docs/05-advanced-management/resource-isolation/) · [Web UI](https://super.docs.sconts.com/docs/05-advanced-management/web-ui/) · [Event Notifications](https://super.docs.sconts.com/docs/05-advanced-management/event-notifications/)
