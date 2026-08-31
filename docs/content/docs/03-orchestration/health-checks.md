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

*   **Interval**: Checks are performed every 5 seconds.
*   **Startup**: Super waits for the first successful check before marking a process as `Healthy`.
*   **Failure**: If a check fails while running, status stays `Running` (unhealthy) until the next check passes. Dependents that use `depends_on` wait for `Healthy`.
