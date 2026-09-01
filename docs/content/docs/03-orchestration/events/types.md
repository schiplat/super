---
title: "Event Types"
weight: 1
description: "Complete catalog of events emitted by the daemon, with payload fields."
aliases:
  - /docs/03-orchestration/system-events/
---

Every event emitted by `superd` has a stable type name and a structured payload. This page is the canonical catalog. For *where* events go next — persisted history, hooks, notifications — see the [Events overview](/docs/03-orchestration/events).

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

### Record-only events

The following events are written to the [event history](/docs/03-orchestration/events/history) **only** — they are not `SystemEvent` variants, so they never fire `[[event_hooks]]`, licensed notifications, or lifecycle hooks. Use them to audit scheduler behavior:

| Event name | When it fires | Notes |
| :--- | :--- | :--- |
| `cron_started` | A scheduled firing was admitted and the run was spawned | `msg` carries the trigger time (ms) |
| `cron_exit` | A scheduled run exited (success **or** failure) | `exit_code` / `signal` carry the exit detail; `duration_secs` records the run duration |
| `cron_spawn_failed` | A scheduled firing could not be spawned | `msg` carries the spawn error |
| `queue_full` | A firing was dropped because the concurrency queue was full | See [Scheduled Tasks — overlap policy](/docs/02-essentials/scheduled-tasks#overlap-policy) |

### Notes

* **`signal` field** (`process_fatal` / `process_backoff`): set when the process was terminated by a signal rather than an exit code (e.g. `9` = SIGKILL, including cgroup OOM kills). When present, `exit_code` is `null`. OSS `superd` captures SIGKILL/OOM termination this way; see the Web UI / `super events` for the recorded event.
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

## Supervisor mapping

| Supervisor `[eventlistener]` | Super |
| :--- | :--- |
| `PROCESS_STATE_RUNNING` | `process_started` |
| `PROCESS_STATE_EXITED` | `process_backoff` or `process_fatal` (depends on autorestart) |
| `PROCESS_STATE_FATAL` | `process_fatal` |
| `TICK_60` | Not supported |

See also [vs Supervisor](/docs/04-production-scenarios/migrations/vs-supervisor).

## Where to configure reactions

Events feed three consumption paths:

| Mechanism | Config location | Scope | Requires | See |
| :--- | :--- | :--- | :--- | :--- |
| **Event history** | `[storage] events_file` / `events_keep_days` | Persistent record of all events | OSS | [Event History](/docs/03-orchestration/events/history) |
| **Event hooks** | `super.toml` → `[[event_hooks]]` | Global, filter by `events` + `programs` | OSS | [Event Hooks](/docs/03-orchestration/events/hooks) |
| **Webhook notifications** | `conf/notify.toml` → `[[channels]]` | Global channels, filter by `triggers` | 💎 `notify` plugin | [Event Notifications](/docs/05-advanced-management/event-notifications) |
| **Rust `Extension::on_event`** | Compile-time or licensed plugin | Global | Plugin / custom build | — |
