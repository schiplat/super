---
title: "Logging"
weight: 5
description: "How Super captures, rotates, and streams logs."
---

Super captures the `stdout` and `stderr` streams of every managed process. This decouples logging from your application logic—your app just needs to print to the console.

> [!IMPORTANT]
> Capture is **not** infinite full-history archival. Lines longer than `max_line_size_kb` (default **16**) are truncated before write/stream, and rotation deletes the oldest backups beyond `max_backups`. For long-term, complete retention, raise `[child_logging]` `max_size_mb` / `max_backups` (and `max_line_size_kb` if needed), or ship logs to a centralized log system. Files the child writes itself (for example `app.log` under its own path) are **not** collected by Super.

## Log Files

By default, logs are stored in the `./logs` directory relative to the Super root. The file naming convention is:

*   `{program_id}.out` (Standard Output)
*   `{program_id}.err` (Standard Error)

Every line written to disk is prefixed with a timestamp. The prefix mode is controlled by `timestamp` in `[child_logging]`:

| Value | Example |
| :--- | :--- |
| `local` (default) | `[2026-09-01 10:14:00] worker started` |
| `utc` | `[2026-09-01T02:14:00Z] worker started` |
| `none` | `worker started` |

Timestamps are captured by the daemon when the line is consumed, not by the child process itself. The raw line (without prefix) is what gets streamed over the WebSocket.

### Automatic Rotation

To prevent logs from consuming all disk space, Super implements automatic rotation.

You can configure this in `super.toml`:

```toml
# super.toml — [child_logging]
[child_logging]
# Max size per file in MB (default: 10)
max_size_mb = 10

# Number of backups to keep (default: 5)
max_backups = 5

# Max single-line length in KB; longer lines are truncated (default: 16)
max_line_size_kb = 16

# Prefix each captured line with a timestamp: local | utc | none (default: local)
timestamp = "local"
```

When a log file exceeds `max_size_mb`:
1.  `app.out` is renamed to `app.out.1`.
2.  Existing backups are shifted (`.1` -> `.2`, etc.).
3.  The oldest backup (beyond `max_backups`) is deleted — that history is gone unless you copied it elsewhere.

## Retention and completeness

| Limit | Default | Effect |
| :--- | :--- | :--- |
| `max_size_mb` | `10` | Triggers rotation when the active `.out` / `.err` file grows past this size. |
| `max_backups` | `5` | How many rotated files to keep; older ones are **deleted**. |
| `max_line_size_kb` | `16` | Single lines longer than this are truncated (with a `...[TRUNCATED]` marker) before disk and WebSocket. |

Super’s on-disk child logs are a **bounded local buffer** for ops and debugging, not a compliance archive. Prefer:

* Larger `max_size_mb` / `max_backups` when you need more local history, or
* A collector / SIEM that tails `$SUPER_ROOT/logs/` (or the child’s own log files) for durable, searchable retention.

Event History (`data/events.db`) records **lifecycle events** (crashes, cron, recoveries, …). It does **not** store full stdout/stderr text — see [Event History](/docs/03-orchestration/events/history).

## Real-time Streaming

You don't need to `tail -f` files manually. Super provides a WebSocket-based stream via the CLI.

```bash
# Stream logs for a specific program
super logs my-app
```

This stream aggregates both stdout and stderr in real-time.

> [!NOTE]
> Extremely long single lines are truncated at `max_line_size_kb` (default **16KB**) so a runaway process cannot exhaust daemon memory or flood the WebSocket. Raise the limit if you must keep longer lines locally; for durable full logs, use rotation settings or an external collector as above.
