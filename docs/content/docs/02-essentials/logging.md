---
title: "Logging"
weight: 5
description: "How Super captures, rotates, and streams logs."
---

Super captures the `stdout` and `stderr` streams of every managed process. This decouples logging from your application logic—your app just needs to print to the console.

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
[child_logging]
# Max size per file in MB (default: 10)
max_size_mb = 10

# Number of backups to keep (default: 5)
max_backups = 5

# Prefix each captured line with a timestamp: local | utc | none (default: local)
timestamp = "local"
```

When a log file exceeds `max_size_mb`:
1.  `app.out` is renamed to `app.out.1`.
2.  Existing backups are shifted (`.1` -> `.2`, etc.).
3.  The oldest backup (beyond `max_backups`) is deleted.

## Real-time Streaming

You don't need to `tail -f` files manually. Super provides a WebSocket-based stream via the CLI.

```bash
# Stream logs for a specific program
super logs my-app
```

This stream aggregates both stdout and stderr in real-time.

> [!NOTE]
> To prevent a runaway process from crashing the daemon or flooding the network, Super truncates extremely long single lines (16KB limit) before processing.
