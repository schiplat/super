---
title: "What's New in 1.4.0"
weight: 10
description: "Readiness-aware reload, Unix socket transport, batch operation safety, and timestamped child logs."
---

What changed in Super 1.4.0, written as a user-facing overview. Full flag/config schemas live in the [CLI reference](./cli-reference) and [config reference](./config-reference).

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
