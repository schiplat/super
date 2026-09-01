---
title: "System Events"
weight: 4
description: "Complete reference for SystemEvent types emitted by the daemon."
---

System events are structured signals emitted by `superd` when something meaningful happens in the cluster. They power **licensed notifications** (`notify.toml`, `notify` plugin), the **audit log** (`security` plugin), and **OSS event hooks** (`[[event_hooks]]`).

This page is the canonical list of all event types. Configuration for reacting to events differs by mechanism — see [Where to configure reactions](#where-to-configure-reactions).

## Event catalog

| Event name | Rust variant | When it fires | Payload fields |
| :--- | :--- | :--- | :--- |
| `process_started` | `ProcessStarted` | Process spawned successfully and received a PID | `program_id`, `program_name`, `pid` |
| `process_fatal` | `ProcessFatal` | Process stopped and will **not** auto-restart (retries exhausted, manual fatal, spawn/pre-start failure, cron failure, OTA rollback trigger, etc.) | `program_id`, `program_name`, `pid`, `uptime_secs`, `exit_code`, `signal`, `msg`, `log_tail` |
| `process_backoff` | `ProcessBackoff` | Process crashed but **will** retry (autorestart still active) | `program_id`, `program_name`, `pid`, `uptime_secs`, `exit_code`, `signal`, `retry_count` |
| `process_recovered` | `ProcessRecovered` | Process was unstable (backoff/fatal path) and is now **Healthy** again | `program_id`, `program_name`, `pid`, `uptime_sec` |
| `health_restart` | `HealthRestart` | Health probes failed `max_failures` times consecutively; the daemon auto-restarted the process | `program_id`, `program_name`, `pid`, `uptime_secs`, `retry_count`, `msg` |
| `system_startup` | `SystemStartup` | `superd` manager loop started (after loading programs) | `hostname` |
| `system_shutdown` | `SystemShutdown` | `superd` is shutting down gracefully | *(none)* |
| `memory_pressure` | `MemoryPressure` | Live (anonymous) memory of a limited cgroup crossed the warning threshold — process still running | `program_id`, `program_name`, `pid`, `usage_bytes`, `limit_bytes`, `warn_bytes` |
| `memory_oom_kill` | `MemoryOomKill` | Kernel OOM-killed a limited cgroup (`memory.events` → `oom_kill` incremented) | `program_id`, `program_name`, `pid`, `usage_bytes`, `limit_bytes`, `anon_bytes` |

### Notes

* **`signal` field** (`process_fatal` / `process_backoff`): set when the process was terminated by a signal rather than an exit code (e.g. `9` = SIGKILL, including cgroup OOM kills). When present, `exit_code` is `null`. OSS `superd` captures SIGKILL/OOM termination this way; see the Web UI / `super events` for the recorded anomaly.
* **`memory_pressure` / `memory_oom_kill`**: emitted by the licensed `isolation` plugin on Linux for programs with `resource_limits.memory_limit`. `memory_pressure` is a **pre-kill warning** (Tier 1 / opt-in Tier 2 throttle); `memory_oom_kill` is a **post-kill confirmation** that makes an OOM kill distinguishable from a manual `kill -9`. See [Resource Isolation — Warning & visibility](/docs/05-advanced-management/resource-isolation#warning--visibility-three-tier).
* **`process_fatal` + `log_tail`**: Licensed webhooks (`notify` plugin) can attach the last lines of stderr when `include_log_tail = true` on a channel. The tail is read at event time from the program log file.
* **`process_recovered`**: Only emitted after a prior crash/backoff (`alert_pending_recovery`). A clean first start does not emit recovery.
* **`health_restart`**: Emitted when a health check fails `max_failures` times in a row (see [Health Checks](/docs/03-orchestration/health-checks)). It fires *before* the restart, and the process is restarted regardless of `autorestart`/`exitcodes` (those only govern exit handling). After `retry_limit` health restarts the process goes Fatal (`process_fatal`) instead.
* **Cron jobs**: exit `0` → stopped quietly; non-zero exit → `process_fatal`.

## JSON shape (internal)

Events are serialized with an internally tagged enum:

```json
{
  "type": "ProcessFatal",
  "payload": {
    "program_id": "550e8400-e29b-41d4-a716-446655440000",
    "program_name": "web-server",
    "exit_code": 137,
    "msg": "Stopped after 3 retries.",
    "log_tail": "Error: bind: Address already in use\n"
  }
}
```

Licensed webhook envelopes wrap this in a richer outer object (`summary`, `markdown`, `system`, etc.). See [Event Notifications](/docs/05-advanced-management/event-notifications).

## Where to configure reactions

| Mechanism | Config location | Scope | Requires | Status |
| :--- | :--- | :--- | :--- | :--- |
| **Lifecycle hooks** | per-program `hooks` (stack JSON / API) | Per program, tied to start/stop flow | OSS | ✅ Implemented — see [Lifecycle Hooks](/docs/03-orchestration/lifecycle-hooks) |
| **Webhook notifications** | `conf/notify.toml` → `[[channels]]` | Global channels, filter by `triggers` | 💎 `notify` plugin | ✅ Implemented |
| **Event hooks** | `super.toml` → `[[event_hooks]]` | Global, filter by `events` + `programs` | OSS | ✅ Implemented — see [Event Hooks](/docs/03-orchestration/event-hooks) |
| **Rust `Extension::on_event`** | Compile-time or licensed plugin | Global | Plugin / custom build | ✅ Implemented |

### Current layout (today)

```
super.toml                    # daemon config only (no program tables)
├── [server]
├── [storage] / [logging]
└── [[event_hooks]]           # OSS — local scripts on system events

conf/conf.d/*.json            # program stacks — services[] + per-program hooks
conf/notify.toml              # notify plugin — [[channels]] + triggers
snapshot.json                 # persisted program state (includes hooks from API/stack)
```

**Lifecycle hooks** live **per program** because they run inside that program's start/stop pipeline.

**System event reactions** (webhooks, event hooks) are **global** — one listener handles events from any program, with optional name filters.

## Supervisor mapping

| Supervisor `[eventlistener]` | Super |
| :--- | :--- |
| `PROCESS_STATE_RUNNING` | `process_started` |
| `PROCESS_STATE_EXITED` | `process_backoff` or `process_fatal` (depends on autorestart) |
| `PROCESS_STATE_FATAL` | `process_fatal` |
| `TICK_60` | Not supported |

See also [vs Supervisor](/docs/04-production-scenarios/migrations/vs-supervisor).
