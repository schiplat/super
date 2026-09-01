---
title: "Resource Isolation"
weight: 3
description: "Enforcing limits with Linux Cgroups v2."
---

> [!IMPORTANT] Licensed feature — `isolation` plugin
> This page covers a **licensed feature** provided by the **`isolation` plugin** (Linux only, cgroups v2, privileged access to `/sys/fs/cgroup`). It requires a valid subscription `[license].key` and the plugin library in `$SUPER_ROOT/plugins/`. OSS `superd` without the plugin ignores `resource_limits` and emits no cgroup events.

### Cgroups Integration 💎

Super integrates directly with the Linux Kernel's Control Groups (v2) to provide hardware-level isolation for managed processes.

> [!NOTE]
> For the “noisy neighbor” production scenario, see [Resource Isolation (scenario)](/docs/04-production-scenarios/stability/resource-isolation).

## Configuration Reference

Resource limits are defined per program — via JSON stack files, the API, or CLI (`super add --memory 512 --cpu 1.5`).

```json
{
  "services": [
    {
      "name": "data-processor",
      "command": "./worker",
      "resource_limits": {
        "memory_limit": 1024,
        "cpu_quota": 2.0
      }
    }
  ]
}
```

*   `memory_limit` — hard memory limit in **MB** (binary, `1 MB = 1024² bytes`). If the process (and its children) exceed this, the OOM Killer terminates it.
*   `memory_warn_percent` — optional; warn (emit a `memory_pressure` event) when live memory reaches this % of `memory_limit`. Default `80`; `0` disables pre-kill warning (see [Warning & visibility](#warning--visibility-three-tier)).
*   `memory_warn_headroom` — optional; also warn when live memory comes within this many MB of `memory_limit` (absolute headroom; whichever threshold triggers first). `0` (default) disables.
*   `memory_high` — optional; kernel **soft limit** in MB. When exceeded the kernel throttles the cgroup instead of killing it (see [Tier 2 — `memory.high` (opt-in)](#tier-2--memoryhigh-opt-in)). `0` (default) disables.
*   `cpu_quota` — CPU quota in **cores** (`1.0` = one full core, `0.5` = half a core, `2.0` = two cores; fractions allowed). The scheduler throttles the process if it exceeds this usage.

> [!WARNING]
> `memory_limit` is a **hard cap** enforced by the kernel — exceeding it kills the process (OOM). It is not a soft "restart when memory exceeds N" policy like PM2's `--max-memory-restart`. Super adds **warning + OOM confirmation** events around the hard cap, but never soft-restarts (see below). For graceful threshold-based restarts, poll `mem_usage` via the API and restart yourself — see [Programmable Ops](/docs/04-production-scenarios/observability/programmatic-control).

## Warning & visibility (three-tier)

Super combines **pre-kill warning**, an **opt-in kernel throttle**, and **post-kill OOM confirmation** around the hard cap. All three surface as `SystemEvent`s — recorded in the event history (`super events <name>`), forwarded by OSS [event hooks](/docs/03-orchestration/event-hooks), and alerted by licensed [event notifications](/docs/05-advanced-management/event-notifications).

### Tier 1 — pre-kill warning (default on)

A background watcher polls each limited cgroup's **anonymous memory** (`memory.stat` → `anon`, page cache is excluded because the kernel can reclaim it and it never causes OOM) every **10 s**.

* When `anon` ≥ threshold (`memory_warn_percent`% of the limit, or within `memory_warn_headroom` of it), Super emits `memory_pressure` **once per 5-minute cooldown** per program. The process keeps running.
* A program only re-arms the warning after it drops back below threshold − **5 pp** (hysteresis), preventing flapping at the boundary.
* `memory_warn_percent = 0` and `memory_warn_headroom = 0` disable this tier.

**What a warning buys you:** with a leak rate of ~100 MB/s, the default 80% threshold on a 1 GiB cap leaves ~200 MB / ~2 s of reaction time. It is **best-effort** — a fast OOM between polls still kills; the kill is then confirmed by Tier 3.

### Tier 2 — `memory.high` (opt-in)

Setting `memory_high` writes the cgroup v2 **soft limit** (`memory.high`). When the cgroup exceeds it the kernel **throttles** reclaim (slows allocation) instead of OOM-killing. Useful to hold a process under its hard cap long enough for Tier 1 to fire reliably.

* This is **off by default** because throttling can slow memory-healthy processes that briefly spike (GC pre-collection heap growth, cache warmup). Only enable it if you accept kernel-level throttling as a trade for earlier, kernel-accurate pressure signals.
* The `memory_pressure` event fires on the same thresholds when the cgroup is under throttle.

### Tier 3 — OOM confirmation (always on)

Super watches each limited cgroup's `memory.events` → `oom_kill` counter. When it increments, Super emits **`memory_oom_kill`** with a usage snapshot (`memory.current`, `memory.max`, `anon`). This makes an OOM kill **distinguishable** from a manual `kill -9`: instead of a generic `signal 9` exit, you get an explicit "memory cap exceeded" event with the exact usage at kill time.

### Recommended setup

```json
{
  "name": "worker",
  "command": "/usr/local/bin/worker",
  "resource_limits": {
    "memory_limit": 1024,
    "memory_warn_percent": 80,
    "memory_warn_headroom": 0,
    "memory_high": 0
  }}
```

The defaults are conservative: Tier 1 warning on, Tier 2 off, Tier 3 always on. Raise/lower `memory_warn_percent` to trade earlier warning against false positives from normal fluctuation; add `memory_high` only when you want kernel-enforced throttling before the hard cap.

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

Superd logs include `[isolation] Applying limits for '…' (PID: …)` on start. When limits change on a **running** process, the cgroup files (`cpu.max`, `memory.max`) are updated in place (see [Hot-update limits](#hot-update-limits-no-restart-for-cpu-quota)).

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
super update data-processor --cpu 1.5
super update data-processor --memory 512

# API
curl -X PUT http://127.0.0.1:9002/api/v1/programs/<id> \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"resource_limits": {"cpu_quota": 1.5, "memory_limit": 512}}'
```

Re-check `cpu.max` / `memory.max` under `/sys/fs/cgroup/super/<id>/` and watch process CPU with `top -p <pid>`.

### Troubleshooting

| Symptom | Likely cause |
| :--- | :--- |
| No `/sys/fs/cgroup/super/<id>/` | Non-Linux build, limits not set, or cgroup create failed (check superd logs) |
| Limits ignored in Docker | Read-only cgroup mount — use a writable cgroup or run with appropriate privileges |
| Process killed under cap | Expected OOM behaviour when exceeding `memory_limit`; confirm via the `memory_oom_kill` event |
| `memory_pressure` never fires | Program's live memory stays below `memory_warn_percent`/headroom, or warnings disabled (`0`) |
| Throttled process despite `memory_high = 0` | CPU quota throttling, not memory; `memory.high` is only written when `memory_high` is set |
| `super_cgroup_enforced_total` is 0 | No programs currently have active cgroup enforcement |

