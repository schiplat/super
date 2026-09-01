---
title: "Health Checks"
weight: 2
description: "Configure TCP, HTTP, and Exec probes to monitor service availability."
---

Super goes beyond checking if a process ID (PID) exists. It actively probes the service to determine if it is truly operational.

Health checks are critical for:
1.  **Dependency resolution**: Unblocking dependent services once upstream is healthy.
2.  **Operational visibility**: Distinguishing "process running" from "service actually ready".
3.  **OTA Validation**: Verifying a new version before committing an update.

## Types of Checks

### 1. TCP Check

The simplest check. Succeeds if Super can establish a TCP connection to the port.

```json
{
  "services": [
    {
      "name": "my-app",
      "command": "./app",
      "health_check": {
        "type": "tcp",
        "port": 8080
      }
    }
  ]
}
```

The check may also set `host` (defaults to `127.0.0.1`).

### 2. HTTP Check

Performs an HTTP request. Succeeds if the response status code is `200-299`. Only `http://` and `https://` URLs are accepted for outbound probes.

```json
{
  "services": [
    {
      "name": "my-app",
      "command": "./app",
      "health_check": {
        "type": "http",
        "url": "http://127.0.0.1:8080/healthz",
        "method": "GET"
      }
    }
  ]
}
```

`method` is optional (defaults to `GET`).

### 3. Exec Check

Runs a shell command. Succeeds if the command exits with code `0`. Ideal for checking file existence, database queries, or custom scripts.

```json
{
  "services": [
    {
      "name": "my-app",
      "command": "./app",
      "health_check": {
        "type": "exec",
        "command": "grep 'ready' /tmp/app.state"
      }
    }
  ]
}
```

## Behavior

*   **Interval**: Checks are performed every `interval_secs` (default `5`).
*   **Startup**: After the process starts, Super waits `start_period_secs` (default `1`) before the first probe, then waits for the first successful check before marking the process as `Healthy`.
*   **Failure**: If a check fails while running, status stays `Running` (unhealthy) until the next check passes. Dependents that use `depends_on` wait for `Healthy`.
*   **Auto-restart**: After `max_failures` consecutive failures (default `3`), Super restarts the process automatically. The restart counter resets as soon as the process reports healthy again. Set `max_failures = 0` to disable auto-restart (mark unhealthy only).
*   **Timeout**: A single probe that exceeds `timeout_secs` counts as a failure.

### Auto-restart & retry limit

A health-triggered restart works like a manual `super restart` — it is not gated by `autorestart`/`exitcodes`, which only govern exit handling. To avoid an infinite restart loop, health restarts are counted per program and bounded by `retry_limit` (default `3`):

1. After `max_failures` consecutive failed probes → `health_restart` event, process restarted.
2. If the process recovers (any successful probe), the counter resets to `0`.
3. If the process stays unhealthy across `retry_limit` health restarts → `process_fatal`, the process is stopped and marked `Fatal` (`autostart` is disabled).

```json
{
  "services": [
    {
      "name": "api",
      "command": "./app",
      "retry_limit": 3,
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

## Tuning reference

Every probe type shares the same four knobs. All defaults apply when the key is omitted or set to `0`, except `max_failures` where `0` **disables** auto-restart.

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `interval_secs` | int | `5` | Seconds between probes. |
| `timeout_secs` | int | `3` (tcp) · `5` (http) · `7` (exec) | Max seconds a single probe may take before it is counted as failed. |
| `start_period_secs` | int | `1` | Grace period after process start before the first probe; probes during this window are skipped, so slow-starting applications are not penalized. |
| `max_failures` | int | `3` | Consecutive failures before the daemon auto-restarts the program. `0` disables auto-restart. |
