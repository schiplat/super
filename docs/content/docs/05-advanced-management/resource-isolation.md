---
title: "Resource Isolation"
weight: 3
description: "Enforcing limits with Linux Cgroups v2."
---

### Cgroups Integration 💎

Super integrates directly with the Linux Kernel's Control Groups (v2) to provide hardware-level isolation for managed processes.

> **Context:** For the “noisy neighbor” production scenario, see [Resource Isolation (scenario)](/docs/04-production-scenarios/stability/resource-isolation).

## Configuration Reference

Resource limits are defined per program — via JSON stack files, the API, or CLI (`super add --memory-limit ... --cpu-quota ...`).

```json
{
  "services": [
    {
      "name": "data-processor",
      "command": "./worker",
      "resource_limits": {
        "memory_limit": 1073741824,
        "cpu_quota": 200.0
      }
    }
  ]
}
```

*   `memory_limit` — hard memory limit in bytes. If the process (and its children) exceed this, the OOM Killer terminates it.
*   `cpu_quota` — CPU quota as a percentage. `100.0` = 1 full core, `50.0` = half a core. The scheduler throttles the process if it exceeds this usage.

## Requirements

*   **OS**: Linux only.
*   **Kernel**: Cgroups v2 enabled (Standard on Ubuntu 22.04+, Debian 11+, Fedora).
*   **Privileges**: The `superd` daemon usually requires root privileges to create and manage cgroups (writing to `/sys/fs/cgroup`).

## Monitoring Limits

You can check if Cgroups are being enforced via the metrics endpoint:

```bash
curl http://localhost:9002/metrics | grep cgroup
# super_cgroup_enforced_total 5
```

Superd logs include `Applying limits` on start and `Hot-updating resources` when limits change on a running process.

## Verify & adjust

### Pre-flight

* Run **`superd`** with the **`isolation` plugin** loaded on Linux, with permission to write under `/sys/fs/cgroup` (typically **root** on bare metal).
* **Containers**: cgroup mounts are often read-only unless you provide a writable cgroup namespace or run privileged. Limits will not apply if Super logs a read-only cgroup warning at startup.
* Confirm cgroup v2: `mount | grep cgroup2` (or `stat -fc %T /sys/fs/cgroup/` shows `cgroup2fs`).

### Confirm a program is in a cgroup

Each managed program gets a directory named by its UUID:

```bash
# Replace <id> with the program UUID from `super list` or the API
ls /sys/fs/cgroup/super/<id>/
cat /sys/fs/cgroup/super/<id>/cpu.max      # e.g. 50000 100000 ≈ 50% of one core
cat /sys/fs/cgroup/super/<id>/memory.max   # hard memory cap in bytes
```

When the process stops, Super removes the cgroup directory (`after_stop` cleanup).

### Hot-update limits (no restart for CPU quota)

Change limits on a **running** program without restarting it (CPU quota is updated in place):

```bash
# CLI (requires isolation plugin on Linux)
super update data-processor --cpu 10
super update data-processor --memory 536870912

# API
curl -X PUT http://127.0.0.1:9002/api/v1/programs/<id> \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"resource_limits": {"cpu_quota": 10.0, "memory_limit": 536870912}}'
```

Re-check `cpu.max` / `memory.max` under `/sys/fs/cgroup/super/<id>/` and watch process CPU with `top -p <pid>`.

### Troubleshooting

| Symptom | Likely cause |
| :--- | :--- |
| No `/sys/fs/cgroup/super/<id>/` | Non-Linux build, limits not set, or cgroup create failed (check superd logs) |
| Limits ignored in Docker | Read-only cgroup mount — use a writable cgroup or run with appropriate privileges |
| Process killed under cap | Expected OOM behaviour when exceeding `memory_limit` |
| `super_cgroup_enforced_total` is 0 | No programs currently have active cgroup enforcement |

