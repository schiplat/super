---
title: "vs PM2"
weight: 2
description: "When PM2 is a Node.js process manager, and when Super is a better fit for mixed binaries."
---

[PM2](https://pm2.keymetrics.io/) is an excellent process manager for the Node.js ecosystem. It is often also used as a general-purpose supervisor for Go, Python, or Java applications.

That second use case is where the tools diverge: PM2’s control plane runs on Node.js; Super’s daemon is a single native binary that treats every child the same, whether or not it is a Node app.

## 1. Control-plane overhead

PM2 is written in JavaScript and runs on Node.js. Managing a small native binary still means running a Node.js VM (the PM2 God daemon, and typically `pm2-agent`) in the background.

Super’s daemon is a Rust binary with no JavaScript runtime. **Idle RSS still depends on OS, version, how many children you manage, and whether licensed plugins are loaded** — it is not a single number. Do not treat any megabyte range on this page as a measurement.

A dated, same-workload comparison (OSS Super, licensed Super, `supervisord`, and PM2) lives in the in-tree [benchmark plan](https://github.com/schiplat/super/tree/master/tools/benchmark). Until that lab publishes a snapshot, compare the tools on your own host rather than quoting a marketing RSS figure.

## 2. Resource limits (cgroups)

PM2 generally relies on the OS or Docker for CPU and memory caps. It does not enforce Linux cgroups itself.

OSS Super **stores** optional `resource_limits` on a program but **does not enforce** them. Linux cgroup v2 CPU/memory limits require the subscription **`isolation` plugin** on Linux. See [Resource Isolation](/docs/05-advanced-management/resource-isolation) and the [feature matrix](/docs/07-editions/feature-matrix).

```toml
# Stored on the program; enforced only when the isolation plugin is loaded on Linux.
# Example shape — check the config reference for the current schema.
```

Example: one program entry in a stack file (`stack.json` services[] or `stack.toml [[services]]`):

```json
{
  "name": "worker",
  "command": "/usr/local/bin/worker",
  "resource_limits": {
    "memory_limit": 256,
    "cpu_quota": 0.25
  }
}
```

Without that plugin, Super logs that limits are stored only. Do not describe cgroup enforcement as an OSS built-in.

**Different from PM2's `--max-memory-restart`:** PM2's memory option is a **soft watchdog** — PM2 polls each process roughly every 30 seconds and gracefully restarts it once RSS exceeds the threshold. Super's `memory_limit` is a **hard** cgroup v2 cap: the kernel OOM-kills the cgroup when it is exceeded (no graceful restart in between). Super adds **warning + OOM-confirmation events** around the hard cap (see [Resource Isolation — Warning & visibility](/docs/05-advanced-management/resource-isolation#warning--visibility-three-tier)), but never soft-restarts on memory. For PM2-style "restart on memory threshold" behaviour, poll `mem_usage` from the API and trigger a graceful restart yourself — see [Programmable Ops](/docs/04-production-scenarios/observability/programmatic-control).

## 3. Language agnostic

PM2 treats non-Node applications as fork-mode processes. Cluster mode (in-process load balancing) applies to Node.js scripts.

Super treats **all** binaries the same. A Rust binary, a Python script, or a Java JAR all get:

* Unified logging
* Health checks
* Graceful shutdown
* Dependency orchestration

## 4. Log rotation

PM2 needs an extra module (`pm2-logrotate`) for rotation.

Super rotates child logs in the OSS daemon. You do not install a plugin to keep disks from filling up. See [Logging](/docs/02-essentials/logging).

## 5. Command parity

`super` uses the same single-target style as PM2 — name, group, id, or `all` (see [CLI Reference — Lifecycle](/docs/06-internals/cli-reference#lifecycle)):

```bash
super <start|stop|restart|remove> <name|@group|id|all>
```

| PM2 | Super |
| :--- | :--- |
| `pm2 stop <app_name\|namespace\|id\|'all'>` | `super stop <name\|@group\|id\|all>` |
| `pm2 restart <app_name\|namespace\|id\|'all'>` | `super restart <name\|@group\|id\|all>` |
| `pm2 delete <app_name\|namespace\|id\|'all'>` | `super stop <...> && super remove <...>` |

One deliberate difference: `pm2 delete` stops and removes in one step, while Super requires a program to be stopped before it can be removed — `super remove` on a running program fails with `Cannot remove running program`. Use `super stop <...> && super remove <...>` to mirror `pm2 delete`. PM2's `json_conf` target has no equivalent in Super; declarative batches go through `super apply <stack.toml>`.

## Summary

* **Stick with PM2** if you run a Node.js stack and want cluster mode for zero-downtime Node reloads.
* **Look at Super** if you manage mixed binaries (Go, Rust, Python, Java, or a combination) and want a native control plane, built-in log rotation, and — with a subscription on Linux — optional cgroup limits.

Do not cite a fixed “saves N MB per instance” figure from this page. Measure on your hardware, or wait for a published benchmark snapshot with versions and methodology.
