---
title: "Editions"
weight: 8
description: "The OSS vs Licensed edition model: one binary, two run modes."
---

Project Super is **open-core** software: a single MIT-licensed binary that runs in two modes — **OSS** (free, no plugins) and **Licensed** (same binary + commercial plugins). There is no separate "Premium daemon", no fork, and no reinstall to upgrade.

## One binary, two run modes

The core is open-source under the [**MIT License**](https://opensource.org/licenses/MIT) and ships as **`superd`** + **`super`** with no compile-time dependency on commercial code.

| | **OSS** | **Licensed** |
| :--- | :--- | :--- |
| Binaries | `superd` + `super` (MIT) | **The same binaries** — drop-in enable, no reinstall |
| What enables it | install & run | a valid `[license].key` in `conf/super.toml`, `[server].auth_secret`, **and** plugin libraries under `$SUPER_ROOT/plugins/` (`security` required) |
| Plugins loaded | none | authorized ones only (verified against the key) |
| Feature examples | process management, OTA, health checks, cron, event hooks, log rotation, **embedded Dashboard**, single-admin API auth | + multi-user tokens / RBAC / audit (`security`), cgroup limits (`isolation`, Linux), notifications (`notify`), Dashboard Pro extensions (`ui` plugin) |
| License | MIT | Commercial plugin license (see [Get Super Pro](/go/pro/)) |

Licensed plugins are optional `.so` / `.dylib` files loaded at runtime after license verification — same binaries, no separate “Premium daemon.” If `[license].key` is absent (or invalid without licensed intent), `superd` runs in OSS mode and ignores the plugin directory.

## Terminology

These terms are used consistently across the documentation:

| Term | Meaning |
| :--- | :--- |
| **OSS** | The open-source run mode: `superd` + `super`, MIT, no plugins. |
| **Licensed** | The run mode with a valid `[license].key` and commercial plugins loaded (also "licensed feature", "licensed plugins"). |
| **Super Pro** | **Brand name** of the commercial plugin set (multi-user tokens / RBAC / audit, isolation, notify, Dashboard Pro UI). Used for purchasing and trials — [Get Super Pro](/go/pro/). The docs describe capabilities as *Licensed*. |
| **License key** | The signed `[license].key` value in `conf/super.toml`; runtime credential that authorizes which plugins load. |
| **Plugin** | A signed `.so` / `.dylib` under `$SUPER_ROOT/plugins/` that implements a licensed capability. |

Pages mark licensed capabilities with **💎** and a "Licensed feature" callout at the top of the page.

## Switching between modes

*   **OSS → Licensed:** add `[license].key` and `[server].auth_secret` to `conf/super.toml`, place the plugin libraries from your subscription into `$SUPER_ROOT/plugins/` (including **`security`**), then restart `superd`. Same binaries, nothing to rebuild.
*   **Licensed → OSS:** remove the key (and plugin libraries), or use `[license].strict = false` behavior that degrades to OSS on an invalid key — see [Authentication](/docs/02-essentials/authentication#invalid-or-incompatible-license-key).

> [!IMPORTANT]
> When licensed intent is present (plugins on disk, `auth_secret` set, or a non-loopback bind), `superd` **refuses startup** instead of silently dropping licensed features — see [Authentication](/docs/02-essentials/authentication#invalid-or-incompatible-license-key).

## Getting Super Pro

**Get Super Pro:** [guide](/go/pro/) — **[free 90-day trial](https://platform.ddl.sconts.com/portal/claim?product=super-pro&plan=first-trials-001)** during beta; optional ¥10/mo OSS sponsorship on Afdian (no Pro). Existing subscribers: [Super Pro Portal](https://platform.ddl.sconts.com/portal/login).

### In this section

*   [**Feature Matrix**](./feature-matrix): A side-by-side comparison of OSS and Licensed.
