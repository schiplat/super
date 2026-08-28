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

A dated, same-workload comparison (OSS Super, licensed Super, `supervisord`, and PM2) lives in the in-tree [benchmark plan](https://github.com/hzbd/super/tree/master/benchmark). Until that lab publishes a snapshot, compare the tools on your own host rather than quoting a marketing RSS figure.

## 2. Resource limits (cgroups)

PM2 generally relies on the OS or Docker for CPU and memory caps. It does not enforce Linux cgroups itself.

OSS Super **stores** optional `resource_limits` on a program but **does not enforce** them. Linux cgroup v2 CPU/memory limits require the subscription **`isolation` plugin** on Linux. See [Resource Isolation](/docs/05-advanced-management/resource-isolation) and the [feature matrix](/docs/07-editions/feature-matrix).

```toml
# Stored on the program; enforced only when the isolation plugin is loaded on Linux.
# Example shape — check the config reference for the current schema.
```

```json
{
  "name": "worker",
  "command": "/usr/local/bin/worker",
  "resource_limits": {
    "memory_limit": 268435456,
    "cpu_quota": 25.0
  }
}
```

Without that plugin, Super logs that limits are stored only. Do not describe cgroup enforcement as an OSS built-in.

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

## Summary

* **Stick with PM2** if you run a Node.js stack and want cluster mode for zero-downtime Node reloads.
* **Look at Super** if you manage mixed binaries (Go, Rust, Python, Java, or a combination) and want a native control plane, built-in log rotation, and — with a subscription on Linux — optional cgroup limits.

Do not cite a fixed “saves N MB per instance” figure from this page. Measure on your hardware, or wait for a published benchmark snapshot with versions and methodology.
