---
title: "API Reference"
weight: 5
description: "HTTP REST API endpoints and JSON schemas."
---

Super exposes a RESTful API on port `9002` (default). All responses are in JSON format.

## Authentication

**Without `security` plugin loaded** (OSS only): No API authentication. The API is open on the bind address. OSS ships with `host = "127.0.0.1"` and `allow_insecure_public_bind = false`; `superd` **refuses startup** on a non-loopback bind unless you set that flag to `true`. See [Configuration — OSS security defaults](/docs/02-essentials/configuration#oss-security-defaults-fail-closed).

**Licensed (`[license].key` valid):** `security` is bundled with every subscription and **must load** — otherwise `superd` refuses startup. API auth is always active when licensed. See [Authentication — Licensed deployments require security](/docs/05-advanced-management/authentication#licensed-deployments-require-security).

**With `security` plugin loaded**: All API requests require `Authorization: Bearer <token>` (except `/health`, `/metrics`, and docs whitelist). Public bind is allowed because auth middleware is active. Config `auth_secret` bootstraps Access Tokens and stays usable until an Admin explicitly disables it. See [Authentication](/docs/05-advanced-management/authentication).

## Health & docs

*   **GET** `/health` — liveness (no auth)
*   **GET** `/api/docs` — Swagger UI when `enable_docs = true` (docs feature build)
*   **GET** `/api/v1/openapi.json` — OpenAPI catalog (same docs gate / whitelist)

## Programs

### List Programs
Get a summary of all managed processes.

*   **GET** `/api/v1/programs`

**Response:**
```json
[
  {
    "id": "a1b2c3d4-...",
    "name": "api-server",
    "status": "Running",
    "pid": 12345,
    "cpu_usage": 2.5,
    "mem_usage": 10485760
  }
]
```

### Create Program
Register a new process dynamically.

*   **POST** `/api/v1/programs`

**Body:**
```json
{
  "name": "worker-1",
  "command": "./worker",
  "autostart": true,
  "autorestart": "unexpected",
  "exitcodes": [0],
  "startsecs": 10,
  "retry_limit": 3
}
```

| Field | Default | Description |
| :--- | :--- | :--- |
| `autorestart` | `unexpected` | `unexpected`, `true`, or `false` (Supervisor-compatible) |
| `exitcodes` | `[0]` | Exit codes treated as success when `autorestart=unexpected` |
| `startsecs` | `10` | Seconds of stable run before exit resets retry counter |

> [!NOTE]
> `autostart` controls boot-time start only. To disable crash auto-restart, set `"autorestart": "false"`.

**Response:** `201 Created` with the new UUID(s). Validation failures return `400` with `{ "status": "error", "message": "..." }`. The message names the field (`command: …`, `health_check.url: …`) and, when a name is set, `program '…':`. JSON syntax / unknown fields include `JSON line N column M`. Duplicate names return `409`.

CLI `super add` / `super update` and the dashboard use the same Manager checks. `super check` applies the create-body rules to `[include]` stacks (offline, no daemon) in TOML or legacy JSON.

### Get Details
Get full configuration and state for a specific program.

*   **GET** `/api/v1/programs/{id}`

`{id}` is the program **UUID** (not the name). Resolve it from `GET /api/v1/programs`.

### Update Program
Partially update an existing program. Only fields present in the body are changed; omitted fields are left unchanged.

*   **PUT** `/api/v1/programs/{id}`

**Body** (all fields optional):

```json
{
  "command": "/usr/local/bin/my-app",
  "env": { "LOG_LEVEL": "debug" },
  "autorestart": "unexpected",
  "health_check": { "type": "http", "url": "http://127.0.0.1:8080/health" }
}
```

| Field | Description |
| :--- | :--- |
| `name`, `command`, `args`, `cwd`, `user`, `group` | Program identity and execution |
| `env`, `env_file` | Environment (`env_file` = `""` clears) |
| `autostart`, `retry_limit`, `autorestart`, `exitcodes`, `startsecs`, `stopsecs`, `priority` | Restart / stop behaviour |
| `depends_on`, `health_check`, `hooks` | Orchestration |
| `stdout_logfile`, `stderr_logfile` | Custom log paths (must resolve under `storage.log_dir`) |
| `artifact` | OTA binary update — see below |
| `cron` | Cron expression — see [Scheduled Tasks](/docs/02-essentials/scheduled-tasks). |
| `on_overlap` | `skip` (default) / `queue` / `kill` — cron overlap policy |
| `catchup` | `skip` (default) / `latest` / `all` — missed-slot backfill policy |
| `jitter_sec` | Max random delay (seconds) before each cron trigger |
| `max_concurrent` | Max overlapping cron runs at once (default `1`, 1–64) |
| `max_queued` | Cap on queued cron firings when at `max_concurrent` (default `100`; `0` = default) — firings beyond the cap are dropped and recorded as `queue_full` events |
| `resource_limits` | 💎 Requires `isolation` plugin on Linux — stored in config always; enforced only when plugin is loaded |

> [!IMPORTANT] Update persists config only
> Updating `command`, `env`, etc. **persists config only** — it does **not** restart a running process. Call `POST /api/v1/programs/{id}/restart` explicitly, or change `artifact.checksum` to trigger an automatic OTA restart.

#### OTA update via API

When `artifact.checksum` differs from the stored value, Super starts the transactional OTA flow (download → verify → backup → swap → restart → health validate / rollback). See [Atomic OTA Updates](/docs/03-orchestration/ota-updates).

**Step 1 — resolve UUID:**

```bash
curl -s http://127.0.0.1:9002/api/v1/programs \
  | jq -r '.[] | select(.name=="my-app") | .id'
```

**Step 2 — trigger update:**

```bash
curl -X PUT "http://127.0.0.1:9002/api/v1/programs/${PROGRAM_ID}" \
  -H "Content-Type: application/json" \
  -d '{
    "artifact": {
      "source": "https://example.com/builds/v2.0.0/app-linux-amd64",
      "checksum": "a1b2c3d4e5f6789abcdef0123456789abcdef0123456789abcdef0123456789",
      "destination": "/usr/local/bin/my-app",
      "extract": false,
      "restart_policy": "immediate"
    }
  }'
```

| `artifact` field | Description |
| :--- | :--- |
| `source` | Download URL (HTTPS; HTTP only on loopback) |
| `checksum` | Expected SHA256 hex of the **downloaded bytes** |
| `destination` | Absolute path of the final binary on disk |
| `extract` | `true` to unpack `.tar.gz` / `.tgz` / `.tar` / `.zip` and stage one payload file |
| `restart_policy` | `immediate` (default), `manual`, `signal`, or `signal:<hup\|int\|term\|quit\|usr1\|usr2>`. **`signal*` requires an enabled `health_check`.** |

**With `security` plugin**: add `-H "Authorization: Bearer <token>"`.

**Response:** `200 OK` on success; `400` if the program is not found or validation fails. The `message` names the field (and `program '…'` when known). JSON syntax / unknown fields include `JSON line N column M`.

### Control Actions

Perform lifecycle actions.

*   **POST** `/api/v1/programs/{id}/start`
*   **POST** `/api/v1/programs/{id}/stop` (Query param: `?force=true`)
*   **POST** `/api/v1/programs/{id}/restart`

### Historical Logs

Read the last N lines from on-disk log files (`{uuid}.out` / `{uuid}.err`).

*   **GET** `/api/v1/programs/{id}/logs`

**Query parameters:**

| Param | Default | Description |
| :--- | :--- | :--- |
| `tail` | `200` | Lines from end of file (max 5000) |
| `source` | both | `stdout` or `stderr` |

**Response:**
```json
{
  "id": "a1b2c3d4-...",
  "logs": [
    { "source": "stdout", "content": "line-1\nline-2\n" },
    { "source": "stderr", "content": "error line\n" }
  ]
}
```

### Event History

Read a program's persisted event history (`data/events.db`, SQLite). Default sort is `time` ascending; pass `order=desc` and/or `sort_by=…` to change. **All** lifecycle events are recorded — not just anomalies. Optional query filters are combinable. For a user-oriented walkthrough of storage, retention, and querying, see [Event History](/docs/03-orchestration/events/history).

*   **GET** `/api/v1/programs/{id}/events`

| Query param | Type | Description |
| :--- | :--- | :--- |
| `from` | int | Inclusive lower bound on `ts` (Unix seconds) |
| `to` | int | Inclusive upper bound on `ts` (Unix seconds) |
| `event_type` | string | Exact event type (e.g. `process_fatal`, `cron_exit`) |
| `exit_code` | int | Exact exit code |
| `q` | string | Free-text match on `msg` |
| `limit` | int | Max rows |
| `offset` | int | Pagination offset |
| `sort_by` | string | `time` (default) · `event` · `exit_code` · `signal` · `retry_count` · `duration_secs` · `msg` |
| `order` | string | `asc` (default) or `desc` |

*   **GET** `/api/v1/events` — same filters, plus `program_id` to scope to one program (omit for the whole daemon).
*   **GET** `/api/v1/events/stats?program_id=<uuid>` — retention statistics: `total`, `by_type` (count per event type), `first_ts`/`last_ts` (retained time range).

**Response:** array of event records:

```json
[
  {
    "ts": 1760000000,
    "ts_ms": 1760000000123,
    "program_id": "2d4c3f1e-...",
    "program_name": "web",
    "event": "process_fatal",
    "exit_code": null,
    "signal": 9,
    "retry_count": 3,
    "duration_secs": null,
    "msg": "Stopped after 3 retries. Last exit code: None"
  }
]
```

| Field | Description |
| :--- | :--- |
| `ts` | Unix timestamp (seconds) |
| `ts_ms` | Unix timestamp (milliseconds) — precise time point, stable ordering |
| `program_id` | Owning program id (`null` for system-wide events) |
| `program_name` | Owning program name (`null` for system-wide events) |
| `event` | `process_fatal` · `process_backoff` · `process_recovered` · `process_exit` · `health_restart` · `cron_started` · `cron_exit` · `cron_spawn_failed` · `queue_full` · `system_startup` · `system_shutdown` |
| `exit_code` | Process exit code, when captured |
| `signal` | Terminating signal (e.g. `9` = SIGKILL, includes cgroup OOM kills) |
| `retry_count` | Backoff retry counter (fatal/backoff only) |
| `duration_secs` | Execution duration in seconds (cron runs only) |
| `msg` | Human-readable detail |

Events survive `superd` restarts. Default retention is **30 days** (`[storage] events_keep_days`); set `0` to keep everything. Pruning runs once per day.

### Send Signal

*   **POST** `/api/v1/programs/{id}/signal`

**Body:**
```json
{
  "signal": "hup"
}
```

## System & Stack

### Apply Stack (Declarative)
Update the entire system state to match a stack definition. The request body is **JSON or TOML** — both use the same schema as stack files (see [Declarative Stacks](/docs/04-production-scenarios/delivery/declarative-stack)), selected by `Content-Type`.

*   **PUT** `/api/v1/stack`

**Response:** `200 OK` with apply log lines. Invalid body or program bodies return `400`; `message` names `services[i] (name=…)` and the field, or `path:line:col:` / `JSON line N column M` for parse errors.

#### JSON body (default)

JSON is the default body type: omit `Content-Type` or send `application/json`. Plain `curl -d` works — a single-line JSON document is valid. Example file `stack.json`:

```json
{
  "prune": true,
  "services": [ ... list of program configs ... ]
}
```

```bash
curl -X PUT http://prod-server:9002/api/v1/stack \
  -H "Authorization: Bearer $TOKEN" \
  -d @stack.json
```

#### TOML body

TOML requires `Content-Type: application/toml` (or `text/toml`). Use `--data-binary` — plain `-d` strips newlines and breaks multi-line TOML. Example file `stack.toml`:

```toml
# stack.toml
prune = true

[[services]]
name = "web"
command = "/bin/true"
```

```bash
curl -X PUT http://prod-server:9002/api/v1/stack \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/toml" \
  --data-binary @stack.toml
```

### Shutdown
Gracefully stop the daemon (same for foreground and `superd --daemon` instances).

*   **POST** `/api/v1/system/shutdown`

### System Stats
Host-level CPU and memory snapshot (refreshed every ~3s by the monitor thread).

*   **GET** `/api/v1/system/stats`

**Response:**
```json
{
  "cpu_percent": 12.4,
  "memory_used_bytes": 4294967296,
  "memory_total_bytes": 17179869184,
  "timestamp": 1719820800
}
```

## Observability

### Prometheus Metrics
Export metrics in Prometheus text format.

*   **GET** `/metrics`

### Log Stream (WebSocket)
Stream stdout/stderr.

*   **WS** `/ws?id={program_id}`

## Batch Operations

Perform actions on multiple programs simultaneously.

*   **POST** `/api/v1/programs/batch`

**Body:**
```json
{
  "target_ids": ["uuid-1", "uuid-2"], // Or omit and use "group_name": "backend"
  "select_all": false,
  "action": {
    "type": "Restart",
    "payload": null
  }
}
```

The `action` is a tagged union (`type` + `payload`). Payload shape varies by action:

| Action | Payload |
| :--- | :--- |
| `Start` / `Restart` / `Remove` | `null` |
| `Stop` | `{ "force": false }` |
| `Signal` | `{ "signal": "term" }` (one of `hup`, `int`, `term`, `kill`, `quit`, `usr1`, `usr2`) |

Example — stop several programs:

```json
{
  "target_ids": ["uuid-1", "uuid-2"],
  "select_all": false,
  "action": { "type": "Stop", "payload": { "force": false } }
}
```

## Security & Authentication (`security` plugin 💎)

> [!WARNING]
> Without the plugin, these routes are not registered. Requests return **404 Not Found**.

Manage access tokens for API authorization. Bootstrap with config `auth_secret`; Admins may optionally disable it after creating an Admin token. See [Authentication](/docs/05-advanced-management/authentication#optional-disable-auth_secret).

### Login
*   **POST** `/api/v1/auth/login`

### Logout
*   **POST** `/api/v1/auth/logout`

### Auth status
*   **GET** `/api/v1/auth/status`

### Disable auth_secret
*   **POST** `/api/v1/auth/secret/disable`

### List Tokens
*   **GET** `/api/v1/auth/tokens`

### Create Token
*   **POST** `/api/v1/auth/tokens`

### Renew Token
*   **POST** `/api/v1/auth/tokens/{id}/renew`

### Create Token body
```json
{
  "name": "ci-deploy-bot",
  "role": "operator"
}
```

### Revoke Token
*   **DELETE** `/api/v1/auth/tokens/{id}`


## System Configuration (licensed plugins 💎)

> [!NOTE]
> **License route** is served by **OSS core** when a valid `[license].key` is configured at startup.  
> **Notify routes** require the `notify` plugin; without it they return **404 Not Found**.
>
> **Authentication:** When the `security` plugin is loaded, protected routes (including license) require a valid Bearer token — same as other authenticated API calls.

### Get License Info
*   **GET** `/api/v1/system/license`

Returns verified subscription metadata plus runtime plugin versions (versions are **not** part of the signed license claims).

**Auth:** Required when `security` plugin is active (`Authorization: Bearer <token>`).

**Response `200`:**
```json
{
  "issued_to": "Customer Name",
  "issued_at": 1710000000,
  "major_version": 1,
  "grants": ["security", "notify", "ui"],
  "expires_at": 1741536000,
  "license_id": "550e8400-e29b-41d4-a716-446655440000",
  "features": ["auth", "notify", "dashboard"],
  "plugin_versions": {
    "security": "0.1.0",
    "ui": "0.1.0"
  }
}
```

| Field | Notes |
|-------|-------|
| `expires_at` | Omitted when license is perpetual (no expiry in claims) |
| `features` | UI feature codes derived from authorized `grants[]` |
| `plugin_versions` | Loaded plugin crate versions at runtime |

**Errors:** `404` if no license configured; `401` if auth required and missing/invalid token.

### Manage Notifications
View or hot-reload webhook channels.

*   **GET** `/api/v1/system/notify`
*   **PUT** `/api/v1/system/notify`
*   **POST** `/api/v1/system/notify/test`