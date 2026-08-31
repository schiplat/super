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

To turn a standard program into a scheduled task, add the `cron` field to its stack entry. Super uses an extended cron expression format (Seconds, Minutes, Hours, Days, Months, Day of Week).

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

## Preventing Overlap

What happens if a job takes longer to run than the interval between its scheduled times? (e.g., a backup takes 2 hours, but it runs every 1 hour).

Super prevents overlaps by design. If a cron job is triggered but its previous instance is still `Running`, Super will **skip the new tick** and log a warning. Your system will never be flooded with overlapping jobs.

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
