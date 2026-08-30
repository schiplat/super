---
title: "Technical FAQ"
weight: 2
description: "Common questions about system integration and internals."
---

## Super vs Systemd?

**Q: Why use Super when Systemd exists?**

**A:** Systemd is a **System-level** init system. Super is an **Application-level** supervisor.
*   **Use Systemd** to boot the OS and start the Super daemon.
*   **Use Super** to manage your application stack (API, Worker, DB).
*   **Why?**
    1.  **Docker**: Systemd is heavy/impossible to run inside containers. Super is native to Docker.
    2.  **Unified API**: Systemd varies by Linux distro. Super provides a consistent JSON API across Ubuntu, Alpine, macOS, and Dev containers.
    3.  **App-Aware**: Systemd doesn't understand "Health Checks" via HTTP or "Atomic Binary Swaps".

## Zombie Processes

**Q: How does Super handle Zombies?**

**A:** Super does **not** act as a global zombie reaper. Managed apps must follow the [Managed Program Requirements](/docs/02-essentials/process-management-contract): run in the foreground, do not escape the process group, and rely on the host init (systemd, Tini) for PID 1 duties.

In short, Super tracks direct children with `child.wait()` and tears down process groups on stop; it does not reap zombies from misbehaving grandchild processes.

For deployment patterns (including `tini` in Docker), see [Container Deployment](/docs/04-production-scenarios/stability/zombie-reaping-in-containers).

## Log Truncation

**Q: Why are my log lines cut off?**

**A:** To protect the daemon's memory stability and WebSocket bandwidth, Super truncates any single log line longer than **16KB**.
If an application goes into a loop printing 100MB lines, it would otherwise crash the supervisor (OOM). We prioritize the stability of the management plane over the completeness of a runaway log line.
