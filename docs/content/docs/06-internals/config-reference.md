---
title: "Config Reference"
weight: 3
description: "Complete schema for super.toml."
---

## Edition legend

| Mark | Meaning |
| :--- | :--- |
| **💎 Subscription** | Requires valid `[license].key` in `conf/super.toml` and matching authorized plugin libraries. OSS ignores unknown subscription-only fields. |
| *(no mark)* | Available in OSS (with or without plugins). |

> [!TIP] Free 1-month beta trial
> Pro plugins are available with a **free 1-month trial license** ([request via GitHub Issue](https://github.com/schiplat/super/issues/new?template=pro-trial.yml)). We recommend staging and non-critical workloads until GA; see the [feature matrix](/docs/07-editions/feature-matrix/). Plugins, licensing, and trial details are covered in [Advanced Management](/docs/05-advanced-management/).

**Licensed-plugin fields in this reference** (quick index):

| Location | Keys / file |
| :--- | :--- |
| Root (`super.toml`) | `auth_secret` 💎 |
| `[license]` | `key` 💎 — cryptographically signed subscription token from your vendor |
| `conf/conf.d/*.json` *(program stacks)* | `services[].resource_limits` (`cpu_quota`, `memory_limit`, `memory_warn_percent`, `memory_warn_headroom`, `memory_high`) 💎 |
| `conf/notify.toml` *(separate file)* | `[[channels]]` 💎 — see [Event Notifications](/docs/05-advanced-management/event-notifications) |

> [!NOTE]
> See [Configuration — OSS security defaults](/docs/02-essentials/configuration#oss-security-defaults-fail-closed) for fail-closed bind, log path confinement, and other defensive defaults.

## Instance layout (`SUPER_ROOT`)

The instance root is resolved from `SUPER_ROOT` first, then the binary's directory layout, then the working directory — see [Environment Variables](/docs/06-internals/environment-variables#super_root).

| Path | Purpose |
| :--- | :--- |
| `conf/super.toml` | Daemon settings (this file) |
| `conf/notify.toml` | Licensed notify plugin config (optional) |
| `data/` | Persisted registry / auth state |
| `logs/` | Daemon (`app.log`) and child process logs |
| `run/` | Runtime files; default pidfile `run/superd.pid` when self-daemonizing |
| `plugins/` | Licensed `.so` / `.dylib` libraries |

## `[server]`

Global settings for the daemon.

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `host` | string | `127.0.0.1` | Bind address for API/Web UI. |
| `port` | int | `9002` | Bind port. |
| `allow_insecure_public_bind` | bool | `false` | Explicit opt-in to bind on a non-loopback address without the `security` plugin. OSS **refuses startup** when `host` is not loopback and this is `false`. **Licensed deployments always load `security`** — this flag applies to OSS only. |
| `shutdown_timeout` | int | `10` | Seconds to wait for SIGTERM before SIGKILL during shutdown. |
| `ota_verify_timeout` | int | `60` | OTA verification window (seconds). After an OTA update restarts a program, the new version must pass its health checks within this window; on timeout the daemon rolls back to the previous version automatically. `0` disables the timeout (no automatic rollback). |
| `flapping_window` | int | `60` | Time window (seconds) to detect restart loops. |
| `flapping_threshold` | int | `5` | Max restarts allowed within the window. |
| `enable_docs` | bool | `false` | Enable Swagger UI (`/api/docs`) when the binary is built with the docs feature. |
| `daemon` | bool | `false` | Unix only: self-daemonize after start (like nginx `daemon on`). Keep **`false`** under systemd/Docker (`Type=simple` / PID 1). CLI: `--daemon` / `--foreground` override this. |
| `pidfile` | string | — | Optional pidfile path. Relative paths resolve under `SUPER_ROOT`. When daemonizing and unset, defaults to `run/superd.pid`. Foreground writes a pidfile only when this (or `--pidfile`) is set. CLI: `--pidfile`. |
| `socket` | string | — | Optional Unix socket endpoint for the API, e.g. `socket = "run/superd.sock"`. Relative paths resolve under `SUPER_ROOT`; absolute paths are used as-is. Unix only (macOS/Linux). When set, the daemon listens on **both** TCP and the socket unless `socket_only = true`. The socket file is created with `socket_mode` permissions (default `0600`, owner only) and removed on clean shutdown. |
| `socket_mode` | string | `0600` | Unix socket file permission mode as an octal string (`0600` = owner read/write, `0660` = owner + group, `0640` = owner read/write + group read). **World-writable modes (`0666`) are refused** — a world-writable socket would let any local user drive the control API. |
| `socket_only` | bool | `false` | When `true`, bind **only** the Unix socket and skip the TCP listener entirely (zero network exposure). Requires `socket` to be set. The non-loopback bind check no longer applies because no TCP port is opened. |

```toml
[server]
# Optional self-daemonize when not using systemd/Docker:
# daemon = true
# pidfile = "run/superd.pid"   # default when daemonizing

# Optional Unix socket endpoint (default 0600, owner-only):
# socket = "run/superd.sock"
# socket_mode = "0600"   # "0600" | "0640" | "0660" — never world-writable
# socket_only = true     # disable the TCP listener entirely

# CLI: super --server unix:///path/to/superd.sock list
```

## Root keys (Licensed 💎)

Top-level fields in `super.toml` (sibling to `[server]`, not inside it):

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `auth_secret` 💎 | string | — | **Plugin only** (`security`). Required for licensed startup. Root Admin Bearer for bootstrap; Admins may explicitly disable login with it after creating an Admin Access Token. See [Authentication](/docs/05-advanced-management/authentication). |

## `[license]` — subscription key (Licensed 💎)

Optional section in `conf/super.toml`. When present and valid, `superd` loads authorized plugins from `plugins/` and **requires the bundled `security` plugin** (`security.so` + `auth_secret`) or refuses startup. See [Licensed deployments require security](/docs/05-advanced-management/authentication#licensed-deployments-require-security).

When a key is present but **does not verify**, behavior depends on deployment signals:

| Condition | Invalid key behavior |
| :--- | :--- |
| Loopback OSS dev (no plugins, no `auth_secret`) | **Degrade** to OSS with stderr/tracing warnings |
| **Licensed intent** (plugins on disk, `auth_secret` set, or non-loopback bind) | **Refuse startup** |
| `[license].strict = true` (or `SUPER_LICENSE_STRICT=1`) | **Refuse startup** always |

The `SUPER_LICENSE` and `SUPER_LICENSE_STRICT` overrides are documented in [Environment Variables](/docs/06-internals/environment-variables#license-licensed-deployments).

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `key` 💎 | string | — | Base64-encoded signed subscription key. Obtain from your subscription vendor. Override: `SUPER_LICENSE` env (same format). Every valid license includes a signing key id (`kid`, e.g. `k_0ac64a3f`) that must match a verifying key embedded in your `superd` build — see [Troubleshooting license verification](/docs/05-advanced-management/authentication#troubleshooting-license-verification). |
| `strict` 💎 | bool | `false` | When `true`, invalid or incompatible keys refuse startup instead of degrading to OSS. Recommended for production licensed deployments. Override: `SUPER_LICENSE_STRICT=1`. |

```toml
[license]
strict = true
key = "eyJjbGFpbXMiOnsiaXNzdWVkX3RvIjoi..."
```

## `[storage]` / `[logging]` / `[child_logging]`

See [Configuration](/docs/02-essentials/configuration) and [Logging](/docs/02-essentials/logging) for examples. Keys mirror `ServerConfig` in `common/src/config.rs`.

### `[child_logging]` — child process log capture

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `driver` | `file` \| `stdout` | `file` | Where captured child `stdout`/`stderr` lines go. `stdout` prints them (prefixed with `[name:source]`) to the daemon's own stdout; `file` writes to `{log_dir}/{id}.out` / `.err`. |
| `max_size_mb` | int | `10` | Max size per log file before rotation (MB). |
| `max_backups` | int | `5` | Number of rotated backups to keep. |
| `max_line_size_kb` | int | `16` | Max single-line length; longer lines are truncated. |
| `timestamp` | `local` \| `utc` \| `none` | `local` | Prefix each captured line with a timestamp. `local` → `[YYYY-MM-DD HH:MM:SS]`, `utc` → `[YYYY-MM-DDTHH:MM:SSZ]`, `none` → raw line. Applied when the daemon consumes the line; the WebSocket stream always carries the raw line. |

```toml
[child_logging]
driver = "file"
max_size_mb = 10
max_backups = 5
max_line_size_kb = 16
timestamp = "local"
```

## `[include]` — program stack files

Programs are declared in **JSON stack files**, not in `super.toml`. `[include].files` lists glob patterns for those files; `superd` parses every match as a stack and applies it on daemon start and on `super reload`.

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `files` | list of string | `[]` | Glob patterns for program stack JSON files, e.g. `["conf/conf.d/*.json"]`. Relative patterns resolve under `SUPER_ROOT`; patterns that resolve outside `SUPER_ROOT` are skipped. |

```toml
[include]
files = ["conf/conf.d/*.json"]
```

Stack file schema and per-program keys: the *Program stacks* section below.

## Program stacks — `conf/conf.d/*.json`

Programs are **not** declared in `super.toml` — `[[programs]]` / `[[program]]` tables there are ignored (`super check` reports them as an error). Programs load from **JSON stack files** matched by `[include].files` (applied on daemon start and `super reload`), the API (`POST /api/v1/programs`), or the CLI (`super add`); all persist to `data/snapshot.json`.

The keys below describe a **single program entry** — one item in a stack file's `services[]` array, or a create/update API payload. Multiple `services[]` entries are allowed per stack.

> [!NOTE]
> Keys such as `autostart`, `autorestart`, `exitcodes`, `startsecs`, and `stopsecs` align with [Supervisor](/docs/04-production-scenarios/migrations/vs-supervisor) for migration. Newer keys (`retry_limit`, `health_check`, `depends_on`, …) use snake_case. In stack JSON / API payloads, `stopwaitsecs` is accepted as an alias for `stopsecs`.

### Identity & execution

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `name` | string | — | **Required.** Unique program name. |
| `command` | string | — | **Required.** Path to the executable. |
| `args` | list | `[]` | Command-line arguments. |
| `env` | dict | `{}` | Inline environment variables (`KEY = "VAL"`). |
| `env_file` | string | — | Path to a `.env` file loaded at spawn time. |
| `cwd` | string | — | Working directory. |
| `user` | string | — | Run as this user (requires root). |
| `group` | string | — | Logical group for batch control (e.g. `@backend`). |
| `numprocs` | int | `1` | Spawn N process instances (CLI: `super add --numprocs`). |
| `process_name` | string | `{name}-{num}` | Process name template for multiple instances (e.g. `worker-{num}`). |

### Restart & stop behaviour

`autostart` and `autorestart` are **independent**:

* **`autostart`** — start the program when `superd` boots.
* **`autorestart`** — restart the program **after it exits** (crash recovery).

Example: `autostart = false` with `autorestart = "true"` gives a manually started service that still recovers from crashes.

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `autostart` | bool | `true` | Start on daemon boot. Cron programs skip boot-time start regardless. |
| `autorestart` | string | `unexpected` | `unexpected` — restart unless exit code is in `exitcodes`; `true` — always restart; `false` — never restart on exit. |
| `exitcodes` | list | `[0]` | Exit codes treated as success when `autorestart = "unexpected"`. |
| `retry_limit` | int | `3` | Max consecutive crash restarts before status becomes `Fatal`. |
| `startsecs` | int | `10` | Seconds of stable uptime before an exit resets the retry counter. |
| `stopsecs` | int | `[server].shutdown_timeout` | Per-program seconds to wait after SIGTERM before SIGKILL. Omit to use `[server].shutdown_timeout` (default `10`). TOML alias: `stopwaitsecs`. |
| `priority` | int | `999` | Boot-time autostart order; lower values start first. |

### Logging

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `stdout_logfile` | string | `{log_dir}/{uuid}.out` | Custom stdout log path (must resolve under `storage.log_dir`). |
| `stderr_logfile` | string | `{log_dir}/{uuid}.err` | Custom stderr log path (must resolve under `storage.log_dir`). |

### Orchestration

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `depends_on` | list | `[]` | Program names that must be **Healthy** before this one starts. |
| `cron` | string | — | Cron expression (e.g. `0 0 * * * *`). See [Scheduled Tasks](/docs/02-essentials/scheduled-tasks). |
| `on_overlap` | string | `skip` | Cron overlap policy: `skip` (drop tick while previous run is active), `queue` (run after the current instance exits), or `kill` (terminate the running instance, then run). |
| `catchup` | string | `skip` | Cron catch-up policy for slots missed while the daemon was down: `skip` (drop), `latest` (backfill the most recent slot once), or `all` (backfill every missed slot, capped at 10). |
| `jitter_sec` | int | `0` | Max random delay in **seconds** added to each cron trigger to spread load. `0` disables jitter. |
| `max_concurrent` | int | `1` | Max overlapping cron runs allowed at once (1–64; `0` means the default). See [Scheduled Tasks](/docs/02-essentials/scheduled-tasks). |
| `max_queued` | int | `100` | Cap on queued cron firings when `max_concurrent` is reached and `on_overlap` is `queue`/`kill` (0–10000; `0` means the default). Firings beyond the cap are dropped and recorded as `queue_full` events. |

### `hooks`

Per-program lifecycle shell hooks. Full behavior table: [Lifecycle Hooks](/docs/03-orchestration/lifecycle-hooks).

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `pre_start` | string | — | Run before spawn; non-zero exit aborts start. |
| `post_start` | string | — | Run after PID assigned (async). |
| `pre_stop` | string | — | Run before stop signal (sync). |
| `post_stop` | string | — | Run after process exits (async). |

### `health_check`

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `type` | string | — | **Required.** `tcp`, `http`, or `exec`. |
| `host` | string | `127.0.0.1` | For `tcp` checks. |
| `port` | int | — | For `tcp` checks. |
| `url` | string | — | For `http` checks. |
| `method` | string | `GET` | For `http` checks: `GET`, `HEAD`, or `POST`. |
| `command` | string | — | For `exec` checks. |
| `interval_secs` | int | `5` | Seconds between probes. `0` = default. |
| `timeout_secs` | int | `3` (tcp) · `5` (http) · `7` (exec) | Max seconds a single probe may take. `0` = default. |
| `start_period_secs` | int | `1` | Grace period after start before the first probe. `0` = default. |
| `max_failures` | int | `3` | Consecutive failures before auto-restart; `0` disables auto-restart. Bounded by `retry_limit` — see [Health Checks](/docs/03-orchestration/health-checks#auto-restart--retry-limit). |

### `resource_limits` 💎

**Commercial only.** Linux cgroups CPU/memory limits; requires the `isolation` plugin on Linux. See [Resource Isolation](/docs/05-advanced-management/resource-isolation).

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `cpu_quota` 💎 | float | — | CPU quota in **cores** (`1.0` = one core, `0.5` = half a core). |
| `memory_limit` 💎 | int | — | Hard memory limit in **MB** (`1 MB = 1024² bytes`); kernel OOM-kills when exceeded. |
| `memory_warn_percent` 💎 | int | `80` | Pre-kill warning threshold as % of `memory_limit` (Tier 1). `0` disables. |
| `memory_warn_headroom` 💎 | int | `0` | Also warn when live memory is within this many MB of the limit. `0` disables. |
| `memory_high` 💎 | int | `0` | Kernel soft limit in MB (cgroup `memory.high`, Tier 2, opt-in). `0` disables. |

## `[[event_hooks]]` *(OSS)*

Global event listeners: **local scripts** (JSON on stdin) or **native webhooks** (`url`). Distinct from licensed `conf/notify.toml` webhooks (`notify` plugin). Full reference: [Event Hooks](/docs/03-orchestration/event-hooks).

> [!WARNING]
> `[webhook]` is not supported in `super.toml`. `superd` and `super check` reject configs that still contain a `[webhook]` table. For IM-specific templates, use `conf/notify.toml` with the `notify` plugin.

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `command` | string | — | Shell command (`sh -c`). Receives JSON on stdin. Ignored when `url` is set. |
| `url` | string | — | HTTP(S) webhook URL. When set, the event JSON is `POST`ed here instead of running `command`. |
| `headers` | map | — | Extra headers for webhook mode (e.g. `Authorization`). |
| `events` | list | `["*"]` | Event names (`process_fatal`, …) or `"*"`. |
| `programs` | list | `["*"]` | Program names to match, or `"*"`. |
| `async` | bool | `true` | Run hook in background task (webhooks are fire-and-forget). |
| `timeout_secs` | int | `30` | Kill hook script / abort webhook request after N seconds. |
| `id` | string | — | Optional label for logs. |

## `conf/notify.toml` 💎

**Licensed plugin only** (`notify`). Separate from `super.toml`. Hot-reloadable webhook / IM channels. Schema and presets: [Event Notifications](/docs/05-advanced-management/event-notifications).

## System events & reactions

Super emits [System Events](/docs/03-orchestration/system-events) (`process_fatal`, `process_started`, etc.). Where to configure reactions:

| Mechanism | Config file | Edition | Status |
| :--- | :--- | :--- | :--- |
| Lifecycle hooks | per-program `hooks` (stack JSON / API) | OSS | ✅ Active |
| Event hooks | `super.toml` → `[[event_hooks]]` | OSS | ✅ [Event Hooks](/docs/03-orchestration/event-hooks) |
| Webhook notifications | `conf/notify.toml` | 💎 Licensed (`notify`) | ✅ [Event Notifications](/docs/05-advanced-management/event-notifications) |
