---
title: "What's New in 1.4.0"
weight: 10
description: "Readiness-aware reload, Unix socket transport, batch operation safety, timestamped child logs, and cron scheduling policies."
---

What changed in Super 1.4.0, written as a user-facing overview. Full flag/config schemas live in the [CLI reference](/docs/06-internals/cli-reference) and [config reference](/docs/06-internals/config-reference).

## Readiness-aware reload

`super reload` can now wait for the new configuration to actually take effect — it applies the config, then polls every affected program until its health checks pass, and only then reports success.

```bash
super reload --wait [--timeout 30]
```

- `--wait`: reload and wait for all affected programs to become `Healthy`.
- `--timeout N`: readiness wait timeout in seconds (default `30`). Exits non-zero if any program is not ready in time, so scripts can fail loudly instead of silently operating on a half-applied config.
- `super start` / `super restart` gain the same readiness option via `--wait-healthy` (wait for `Healthy`, not just `Running`).

For OTA updates, the daemon can enforce a verification window on its own: `[server] ota_verify_timeout` (default `60`, `0` disables) makes a newly-restarted version that fails its health checks within the window roll back automatically to the previous version.

## Unix socket transport

The API can now be exposed on a Unix domain socket instead of (or alongside) TCP — ideal for local-only management with zero network exposure.

```toml
[server]
socket = "run/superd.sock"   # relative paths resolve under SUPER_ROOT
socket_mode = "0600"         # owner-only by default; group access via 0640/0660
socket_only = true           # disable the TCP listener entirely
```

- The socket file is created owner-only (`0600`) by default; world-writable modes are refused at startup.
- The CLI connects with `super --server unix:///path/to/superd.sock` (REST and `super logs --follow` both ride the socket).
- Without `--server` and without a saved login, the CLI auto-discovers `$SUPER_ROOT/run/superd.sock` and prefers it over TCP.

## Batch operation safety

Operations that touch more than one program (`@group` / `all`) now show exactly what would happen before anything runs:

```bash
$ super stop all
WARNING: You are about to STOP 4 programs.
   - web1
   - web2
   - api1
   - standalone
Are you sure you want to continue? [y/N]
```

- `--dry-run` prints the same preview list and exits without executing anything — handy for auditing what `@group` or `all` would hit.
- `--yes` / `-y` skips the prompt entirely, for scripts and cron jobs.
- Single-target operations (`super stop web1`) never prompt.

## Timestamped child logs

Captured child `stdout`/`stderr` lines are now prefixed with a timestamp by default:

```
[2026-09-01 09:15:33] request handled in 12ms
```

```toml
[child_logging]
timestamp = "local"   # local (default) | utc | none
```

`none` restores the previous raw format. The WebSocket log stream always carries the raw, un-prefixed line.

## Cron scheduling policies

Scheduled tasks can now opt into three behaviors that classic cron schedules lack:

| Policy | Field | Options | Default |
| :--- | :--- | :--- | :--- |
| Overlap | `on_overlap` | `skip` / `queue` / `kill` | `skip` |
| Catch-up | `catchup` | `skip` / `latest` / `all` | `skip` |
| Jitter | `jitter_sec` | seconds | `0` |

```bash
super add --name db-backup --cron "0 0 2 * * *" \
    --on-overlap queue --catchup latest --jitter 60 /scripts/backup.sh
```

- `on_overlap=queue` starts the queued run as soon as the current instance exits; `kill` terminates the running instance first (stale results beat a long-running job).
- `catchup=latest` backfills the most recent slot missed during a daemon outage; `all` backfills every missed slot (capped at 10 runs).
- `jitter_sec` adds a random delay of up to that many seconds before each trigger, so jobs sharing a schedule boundary no longer collide at the same instant.

## Cron concurrency & bounded queue

By default a scheduled task runs at most one instance at a time. `max_concurrent` raises that ceiling: up to N runs of the same task can be in flight simultaneously, and `on_overlap` only kicks in once every slot is taken. This suits jobs where most ticks finish quickly but an occasional backlog is fine to absorb (e.g. a per-minute thumbnail/render task with `max_concurrent=4`).

`max_queued` bounds the queue behind a full concurrency limit. When the queue is full, new firings are dropped and recorded as `queue_full` events, so a slow job can never accumulate an unbounded backlog.

```bash
super add --name thumbnails --cron "* * * * * *" \
    --max-concurrent 4 --max-queued 10 /scripts/thumb.sh
```

| Field | Default | Cap | Meaning |
| :--- | :--- | :--- | :--- |
| `max_concurrent` | `1` | `64` | Max overlapping cron runs at once |
| `max_queued` | `100` | `10000` | Max queued firings while at `max_concurrent`; excess firings drop with a `queue_full` event |

Full behavior: [Scheduled Tasks (Cron)](/docs/02-essentials/scheduled-tasks).

## Health check tuning & auto-restart

Health probes now carry four tuning knobs, and — new in 1.4.0 — can restart a process that stays unhealthy.

```json
{
  "services": [
    {
      "name": "api",
      "command": "./app",
      "health_check": {
        "type": "http",
        "url": "http://127.0.0.1:3000/healthz",
        "interval_secs": 10,
        "timeout_secs": 3,
        "start_period_secs": 5,
        "max_failures": 3
      }
    }
  ]
}
```

| Key | Default | Meaning |
| :--- | :--- | :--- |
| `interval_secs` | `5` | Seconds between probes |
| `timeout_secs` | `3` (tcp) · `5` (http) · `7` (exec) | Max seconds a single probe may take |
| `start_period_secs` | `1` | Grace period before the first probe (slow-start friendly) |
| `max_failures` | `3` | Consecutive failures before auto-restart; `0` disables |

After `max_failures` consecutive failures the daemon restarts the process and records a `health_restart` event (visible in `super events` and webhooks). The counter resets as soon as the process reports healthy again, and `retry_limit` bounds how many health restarts a persistently-broken process can consume before it goes `Fatal` — no infinite restart loops.

Full behavior: [Health Checks](/docs/03-orchestration/health-checks).
