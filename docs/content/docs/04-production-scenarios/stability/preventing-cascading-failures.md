---
title: "Preventing Cascading Failures"
weight: 1
description: "How to stop boot loops and crash cycles using intelligent dependency orchestration."
---

A common production nightmare is the **"Startup Avalanche"**.

**The Scenario**: You restart your server. The database (`db`), cache (`redis`), and backend API (`api`) all try to start at the same time.
1.  `api` tries to connect to `db`.
2.  `db` is still initializing files and not accepting connections.
3.  `api` crashes with `ConnectionRefused`.
4.  Super restarts `api`.
5.  `api` crashes again.
6.  Super enters "Backoff" mode for `api`.
7.  By the time `db` is finally ready, `api` is stuck in a long backoff timer, causing extended downtime.

## The Naive Solution: `sleep`

Admins often patch this by adding arbitrary sleeps in shell scripts:

```bash
# start.sh
/usr/bin/postgres &
sleep 10  # Hope 10 seconds is enough?
/usr/bin/api
```

This is **brittle**. If the DB takes 11 seconds, it fails. If it takes 1 second, you wasted 9 seconds.

## The Super Solution: Topology + Health

Super solves this deterministically by combining **Dependency Topology** with **Active Health Checks**.

### 1. Define the Health Check

First, tell Super how to know when the provider (`db`) is *actually* ready to serve traffic, not just when the process started.

```json
{
  "services": [
    {
      "name": "postgres",
      "command": "/usr/bin/postgres",
      "health_check": {
        "type": "tcp",
        "port": 5432
      }
    }
  ]
}
```

### 2. Define the Dependency

Next, tell the consumer (`api`) to wait.

```json
{
  "services": [
    {
      "name": "api",
      "command": "/usr/bin/api",
      "depends_on": ["postgres"]
    }
  ]
}
```

### The Result

When Super starts:
1.  It sees `api` depends on `postgres`.
2.  It starts `postgres`.
3.  `api` enters the **`Waiting`** state (it does not spawn yet).
4.  Super polls `localhost:5432`.
5.  Once `postgres` opens the port, it transitions to **`Healthy`**.
6.  Only then does Super spawn `api`.

**Zero crash loops. Zero race conditions. Fastest possible startup time.**
