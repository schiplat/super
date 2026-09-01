---
title: "Managed Program Requirements"
weight: 1
description: "Hard requirements for applications managed by Super — foreground execution, no PGID escape, and zombie reaping boundaries."
---

Before you add programs to Super, read this page. It defines the **contract** between Super and the workloads it supervises. Violations may lead to orphaned processes, untracked PIDs, or zombies that Super cannot clean up.

## 1. Foreground only

Every **application managed by Super** must run in **foreground / non-daemonized** mode.

This rule applies to managed programs, **not** to how you start `superd` itself. `superd` may run in the foreground (default, required under systemd/Docker) or optionally self-daemonize with `superd --daemon` / `[server] daemon = true` when you are not using an init system — see [Installation — Systemd](/docs/01-getting-started/installation/#method-3-systemd-vm--bare-metal).

Managed programs must **not**:

* Call `fork()` and exit the parent while leaving a detached child running.
* Call `setsid()` or otherwise detach from the controlling terminal to “daemonize” themselves.

Super tracks the process it spawns. If your app exits the parent immediately after forking, Super believes the service has stopped while work continues outside its control.

**Examples**

| Application | Do this | Not this |
| :--- | :--- | :--- |
| Nginx | `nginx -g "daemon off;"` | Default daemon mode |
| Node.js | `node app.js` | Custom double-fork wrapper |
| Custom services | Run the main process in the foreground | Shell script that forks and exits |

Set `command` / `args` on each program (stack file, `super add`, or the API) so the **main PID Super starts is the real service**, not a launcher that exits after forking.

## 2. No double-fork or PGID escape

Any attempt to **double-fork**, **escape the process group (PGID)**, or otherwise run outside the tree Super created is considered a **contract violation**.

Super assigns a process group before spawn and stops services with `kill(-PGID, …)` so the direct tree is torn down together. Processes that break out of that group are **orphans**: Super does **not** guarantee tracking, signal delivery, or forced cleanup for them.

If you need a traditional system daemon, run it under the host init (systemd) — not under Super.

## 3. Zombies and init separation

Super follows **separation of concerns** with the host OS:

| Layer | Role |
| :--- | :--- |
| **Host init** (systemd on bare metal, **Tini** / **dumb-init** as PID 1 in containers) | Reap adopted zombies, handle PID 1 duties |
| **Super** | Orchestrate managed services, health checks, dependencies, graceful shutdown of **its** process trees |

Super is **not** a global zombie reaper. A global `SIGCHLD` handler would compete with Tokio’s `child.wait()` on managed processes and can cause deadlocks or race conditions.

If a managed app spawns short-lived children and does not `wait()` on them, those zombies are the responsibility of:

1. The application (prefer `exec` over shell wrappers, or call `wait()`), or  
2. The host/container init wrapper.

For Docker and Kubernetes, see [Container Deployment](/docs/04-production-scenarios/stability/zombie-reaping-in-containers).

## Summary

| Rule | Requirement |
| :--- | :--- |
| Foreground | No self-daemonization (`fork` + exit parent, `setsid`, etc.) |
| Process group | Stay inside the PGID Super created; no escape |
| Zombies | Host init reaps; Super orchestrates, does not replace PID 1 |

Super enforces this contract by design. Workloads that need full init-system semantics belong at the **machine or container** level, not inside a supervised application slot.
