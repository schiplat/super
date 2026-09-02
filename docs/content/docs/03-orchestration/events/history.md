---
title: "Event History"
weight: 2
description: "How events are persisted, retained, queried, and audited."
---

Every event emitted by `superd` is recorded to a persistent, queryable **event history**. This page covers storage, retention, querying, and statistics. For the list of event types, see [Event Types](/docs/03-orchestration/events/types).

## What gets recorded

**All** events are recorded — not just anomalies:

* Program lifecycle: crashes (`process_fatal`), backoff retries (`process_backoff`), recoveries (`process_recovered`), starts (`process_started`), health restarts (`health_restart`)
* Scheduler: `cron_started`, `cron_exit` (with `duration_secs`), `cron_spawn_failed`, `queue_full`
* Daemon: `system_startup`, `system_shutdown`
* Licensed `isolation` plugin (Linux, with `resource_limits`): `memory_pressure`, `memory_oom_kill`

## Storage

Event history lives in a **SQLite database**, by default `data/events.db` (WAL mode). The location is configurable:

```toml
# super.toml — [storage]
[storage]
events_file = "./data/events.db"  # relative paths resolve under SUPER_ROOT
events_keep_days = 30             # 0 = keep everything (default is 30)
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `events_file` | path | `./data/events.db` | SQLite database location. Relative paths resolve under `SUPER_ROOT`. |
| `events_keep_days` | int | `30` | Retention window (days). Events older than this are pruned **once per day**. `0` keeps everything (unlimited). |

> [!NOTE] Storage isn't manual
> The Manager `try_send`s each record to a dedicated background task that drains a bounded queue into SQLite transaction batches (up to 512 rows per flush). Persistence never blocks the actor loop — if the queue is full, new records are dropped rather than stalling process control. The database is tuned for high-throughput workloads: indexes aligned with the `(ts_ms, id)` sort key, WAL PRAGMA settings (large page cache, memory temp tables, capped WAL growth).

### Retention

By default events are pruned after **30 days** (once per UTC day). Set `events_keep_days = 0` for **unlimited retention**. Pruning is time-based only — there is no per-program cap.

> [!TIP] Storage footprint
> A busy daemon records events on every lifecycle transition and cron firing. With the default 30-day window a typical deployment stays well under a few tens of MB. For high-frequency cron jobs, raise `events_keep_days` and use the [time-window filters](#querying) to keep queries fast.

## Querying

### CLI — `super events`

```bash
super events <name|id>                    # all recorded events (newest first)
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

Every record carries a **Unix timestamp** (`ts`, seconds) and a millisecond-precision `ts_ms`; the table's `Time` column renders it as local time. The `signal` column shows the terminating signal — `9` (`SIGKILL`) typically indicates a cgroup/kernel OOM kill under `resource_limits`.

### API

The same querying is available over the API:

* **`GET /api/v1/programs/{id}/events`** — a program's event history. Query params: `from`, `to`, `event_type`, `exit_code`, `q`, `limit`, `offset`, `sort_by` (`time` · `event` · `exit_code` · `signal` · `retry_count` · `duration_secs` · `msg`), `order` (`asc` / `desc`).
* **`GET /api/v1/events`** — same filters, plus `program_id` to scope to one program (omit for the whole daemon).
* **`GET /api/v1/events/stats?program_id=<uuid>`** — retention statistics: `total`, `by_type` (count per event type), `first_ts`/`last_ts` (retained time range).

Full reference: [API Reference — Event History](/docs/06-internals/api-reference#event-history).

## Statistics

`super events <name> --stats` (or `GET /api/v1/events/stats`) summarizes the retained history:

* `total` — number of retained events
* `by_type` — count per event type (ordered by count, descending)
* `first_ts` / `last_ts` — the retained time range

```bash
super events my-worker --stats
# total: 428
# first_ts: 2026-08-01 00:00:00 UTC
# last_ts:  2026-09-01 23:59:59 UTC
# by_type:
#   process_fatal      12
#   process_backoff    34
#   cron_exit         360
#   ...
```

## Common workflows

### Audit cron runs

```bash
super events <name> --type cron_exit        # all runs, with duration_secs
super events <name> --type cron_exit --stats   # how many, and over what range
```

### Diagnose a restart loop

```bash
super events <name> --type process_fatal     # why it stopped
super events <name> --type health_restart    # health-driven restarts
super events <name> --q "oom"                # message text search
```

### Check for OOM kills

```bash
super events <name> --type memory_oom_kill   # cgroup OOM confirmations (isolation plugin)
super events <name> --exit-code 137          # conventional 128+SIGKILL exits
```
