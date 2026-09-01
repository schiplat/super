---
title: "Scheduled Tasks (Cron)"
weight: 6
description: "Replace legacy crontab by scheduling periodic jobs directly in Super."
aliases:
  - /docs/05-advanced-management/scheduled-tasks/
---

Process managers are traditionally used for long-running daemons (like web servers). However, managing periodic tasks (like database backups or log cleanups) usually forces you to fall back to the system's `crontab`, which lacks centralized logging, alerting, and observability.

Super natively supports **cron-based scheduled tasks** in the open-source `superd` binary.

## Configuration

To turn a standard program into a scheduled task, add the `cron` field to its stack entry. Super uses an extended cron expression format (Seconds, Minutes, Hours, Days, Months, Day of Week). Example: `conf/conf.d/db-backup.json`:

```json
{
  "services": [
    {
      "name": "db-backup",
      "command": "/scripts/backup.sh",
      "cron": "0 0 2 * * *"
    }
  ]
}
```

## State Machine Differences

When a program has a `cron` expression, Super fundamentally changes how it manages the process lifecycle:

1. **No Autostart**: Even if `autostart` is `true`, the process will **not** start immediately when the daemon boots. It will remain in the `Stopped` state until the cron scheduler triggers it.
2. **Success (Exit 0)**: When the job finishes successfully (exit code `0`), Super marks it as `Stopped`. It **does not** attempt to restart it. It simply waits for the next cron tick.
3. **Failure (Exit != 0)**: If the job fails, Super marks it as `Fatal` and fires a `process_fatal` system event. Pair with [Event Hooks](/docs/03-orchestration/event-hooks) (OSS) or licensed [Event Notifications](/docs/05-advanced-management/event-notifications) (`notify` plugin) for external alerting.

## Overlap Policy

What happens if a job takes longer to run than the interval between its scheduled times? (e.g., a backup takes 2 hours, but it runs every 1 hour).

The `on_overlap` field controls this. It can be set per-program via the stack entry, `super add --on-overlap`, or `super update --on-overlap`:

| Value | Behavior |
|---|---|
| `skip` (default) | Drop the new tick and log a warning. The running instance is never disturbed. |
| `queue` | Keep the tick queued and start the next run as soon as the current instance exits. Runs never overlap, but every tick is eventually executed. |
| `kill` | Terminate the running instance (SIGTERM), then start the new run. Useful when a late tick means the data it produces is now stale. |

Example: `conf/conf.d/db-backup.json`:

```json
{
  "services": [
    {
      "name": "db-backup",
      "command": "/scripts/backup.sh",
      "cron": "0 0 2 * * *",
      "on_overlap": "queue"
    }
  ]
}
```

With `skip` (the default), Super prevents overlaps by design: if a cron job is triggered but its previous instance is still `Running`, the new tick is skipped and a warning is logged. Your system will never be flooded with overlapping jobs.

## Catch-up Policy

If the daemon was down at a scheduled time (maintenance, upgrade, crash), those slots are missed. The `catchup` field decides what happens to them when the daemon comes back:

| Value | Behavior |
|---|---|
| `skip` (default) | Drop the missed slots entirely. Only future scheduled runs fire. |
| `latest` | Run once, immediately, for the most recent missed slot, then continue the normal schedule. |
| `all` | Backfill every missed slot, up to a cap of 10 runs, starting as soon as possible after recovery. |

Example: `conf/conf.d/db-backup.json`:

```json
{
  "services": [
    {
      "name": "db-backup",
      "command": "/scripts/backup.sh",
      "cron": "0 0 2 * * *",
      "catchup": "latest"
    }
  ]
}
```

The `all` cap exists so a long outage cannot flood the machine with catch-up runs. With `catchup` left at its default, scheduled jobs behave like classic `crontab`: a missed run while the daemon is down is simply lost.

## Jitter

Jobs that share a schedule boundary (e.g. many tasks at `0 2 * * * *`) can create a burst of simultaneous work. The `jitter` field adds a random delay in `[0, jitter]` **seconds** to each trigger, spreading the load. Example: `conf/conf.d/db-backup.json`:

```json
{
  "services": [
    {
      "name": "db-backup",
      "command": "/scripts/backup.sh",
      "cron": "0 0 2 * * *",
      "jitter_sec": 60
    }
  ]
}
```

```bash
super add --name db-backup --cron "0 0 2 * * *" --jitter 60 /scripts/backup.sh
```

The delay is drawn uniformly at random for every trigger, so the effective run time drifts around the scheduled slot instead of all jobs colliding at the same second. Jitter only delays the start — it never causes a run to be skipped.

## Concurrency

Some jobs are allowed to overlap **up to a limit**: think of a render or thumbnail task whose schedule fires every minute, where most ticks finish in a few seconds but a backlog occasionally needs several runs in flight at once.

By default `max_concurrent` is `1` — a scheduled task runs at most one instance at a time, and `on_overlap` decides what happens to extra firings. Raising it lets up to N runs of the same task run simultaneously. Example: `conf/conf.d/thumbnails.json`:

```json
{
  "services": [
    {
      "name": "thumbnails",
      "command": "/scripts/thumb.sh",
      "cron": "* * * * * *",
      "max_concurrent": 4
    }
  ]
}
```

```bash
super add --name thumbnails --cron "* * * * * *" --max-concurrent 4 /scripts/thumb.sh
```

When a firing arrives and fewer than `max_concurrent` instances are running, it is admitted immediately — even if the previous run is still active. Only when every slot is taken does `on_overlap` decide what to do:

* `skip` — drop the firing.
* `queue` — enqueue it (bounded by `max_queued`).
* `kill` — terminate the oldest instance and enqueue the new one.

### Bounded queue

Queued firings are capped by `max_queued` (default `100`, `0` means the default). When the queue is full, new firings are **dropped** and a `queue_full` event is recorded on the program's event history — visible via `super events`. This keeps a straggling long-running task from accumulating an unbounded backlog. Example: `conf/conf.d/db-backup.json`:

```json
{
  "services": [
    {
      "name": "db-backup",
      "command": "/scripts/backup.sh",
      "cron": "0 0 2 * * *",
      "max_concurrent": 2,
      "max_queued": 10
    }
  ]
}
```

```bash
super add --name db-backup --cron "0 0 2 * * *" --max-concurrent 2 --max-queued 10 /scripts/backup.sh
```

`max_concurrent` is capped at `64`; `max_queued` at `10000`.

## Flapping Detection Exemption

Cron jobs are **exempt from flapping detection**. A regular (non-cron) program that exits and is restarted too frequently within `flapping_window` is flagged as `Fatal` and its `autostart` is disabled — that is the intended guard for long-running services. Short-interval cron jobs (e.g. every few seconds) intentionally start and exit on every tick, so treating them like a restart loop would permanently disable the schedule. Super skips the flapping check for any program with a `cron` expression, allowing arbitrary schedule intervals.

## CLI Usage

You can create cron jobs directly from the CLI:

```bash
super add --name daily-cleanup --cron "0 0 3 * * *" /scripts/cleanup.sh
```

You can also manually trigger a cron job out of schedule for testing purposes using the standard start command:

```bash
super start daily-cleanup
```

## Related

* [Config Reference — `cron`](/docs/06-internals/config-reference)
* [System Events — cron failures](/docs/03-orchestration/system-events)
