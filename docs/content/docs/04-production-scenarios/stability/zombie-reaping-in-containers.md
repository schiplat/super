---
title: "Container Deployment"
weight: 2
description: "Running Super in Docker and Kubernetes — what it handles, what it doesn't, and when to add an init wrapper."
---

Super is an **application-level process orchestrator**, not a full init system like `systemd` or `tini`. Understanding this boundary is essential for running it correctly in containers.

See the [Process Management Contract](/docs/02-essentials/process-management-contract) for the full rules on foreground execution, PGID escape, and zombie handling.

## The PID 1 Problem (Background)

In Linux containers, **Process ID 1 (PID 1)** has special responsibilities:

1.  It must `wait()` on adopted child processes, or they become **zombies** (`[defunct]`).
2.  It receives signals (like `SIGTERM`) from the container runtime.

If a simple shell script or Java app runs as PID 1 without reaping logic, short-lived child processes can accumulate as zombies and eventually exhaust the container's PID limit.

**Super does not implement a global SIGCHLD reaper.** This is intentional — a global reaper competes with Tokio's `child.wait()` on managed processes, causing race conditions. Instead, Super focuses on what it does best: orchestrating the processes it directly manages.

## What Super Handles

### 1. Process Group Management

Before spawning a managed process, Super calls `process_group(0)`. During shutdown, it sends signals to the **entire process group** (`kill(-PGID, SIGTERM)`), ensuring child processes spawned by your app are terminated together — not left as orphans holding ports or file handles.

### 2. Signal Forwarding

When you run `docker stop my-container`, Docker sends `SIGTERM` to PID 1.

*   **Shell script**: Often ignores it, forcing Docker to wait 10s and then `SIGKILL`.
*   **Super**: Catches `SIGTERM` / `SIGINT`, gracefully stops all managed services (respecting `shutdown_timeout`), then exits.

### 3. Managed Process Exit Detection

Every process Super spawns is tracked via `child.wait()`. When a managed process exits — normally or via signal — Super immediately updates its state (`Stopped`, `Fatal`, `Backoff`, etc.).

## What Super Does NOT Handle

If a **managed application** spawns its own children (e.g., a shell script running `convert image.png`) and does not call `wait()` on them, those grandchildren become zombies. Super will **not** reap them.

This is the responsibility of either:

*   The application itself (calling `wait()` or using `exec` instead of shell wrappers), or
*   A dedicated init wrapper (see below).

## Recommended Deployment

### Simple Setup (No Forking Children)

If your managed processes do not spawn short-lived shell children, Super alone is sufficient:

```dockerfile
FROM ubuntu:22.04
COPY superd /usr/local/bin/superd
COPY super.toml /etc/super/super.toml
# Foreground only — do not use superd --daemon or [server] daemon = true as PID 1
ENTRYPOINT ["/usr/local/bin/superd"]
```

### With Init Wrapper (Recommended for Production)

If your applications fork subprocesses (common with Java, Python, shell scripts), wrap Super with `tini` or `dumb-init`:

```dockerfile
FROM ubuntu:22.04
RUN apt-get update && apt-get install -y tini
COPY superd /usr/local/bin/superd
COPY super.toml /etc/super/super.toml
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/superd"]
```

`tini` handles PID 1 reaping; Super handles multi-service orchestration, health checks, and graceful shutdown. Keep `superd` in the foreground under both patterns.

## Summary

| Capability | Super | tini / dumb-init |
| :--- | :---: | :---: |
| Multi-service orchestration | ✅ | ❌ |
| Health checks & dependencies | ✅ | ❌ |
| SIGTERM → graceful shutdown | ✅ | ✅ |
| Process group kill on stop | ✅ | ❌ |
| Global zombie reaping (PID 1) | ❌ | ✅ |

Use Super for orchestration. Use `tini` when your workload spawns unmanaged children.
