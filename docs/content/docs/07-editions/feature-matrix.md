---
title: "Feature Matrix"
weight: 1
description: "OSS core vs optional licensed plugins."
---

| Feature | OSS | Licensed |
| :--- | :---: | :---: |
| **Core Process Management** | ✅ | ✅ |
| **Dependency Orchestration** | ✅ | ✅ |
| **Atomic OTA Updates** | ✅ | ✅ |
| **Health Checks (TCP/HTTP)** | ✅ | ✅ |
| **Web UI (Dashboard)** | ❌ | ✅ (`ui` plugin) |
| **Log Rotation & Streaming** | ✅ | ✅ |
| **Prometheus Metrics** | ✅ <br>(basic) | ✅ <br>(+ plugin metrics) |
| **Historical Logs API** | ✅ | ✅ |
| **System Stats API** | ✅ | ✅ |
| **Event Hooks** (`[[event_hooks]]`) | ✅ <br>(local scripts + native webhook) | ✅ |
| **Cron Scheduled Tasks** | ✅ | ✅ |
| **Linux Cgroups Isolation** | ❌ | ✅ (`isolation` plugin, **Linux only**) |
| **RBAC (User Roles)** | ❌ | ✅ (`security` plugin — **required** for licensed startup) |
| **Audit Logging** | ❌ | ✅ (`security` plugin — **required** for licensed startup) |
| **Webhook Notifications** | ✅ <br>(generic HTTP POST via `[[event_hooks]]`) | ✅ <br>(`notify` plugin: IM templates + channels) |
| **License** | MIT | Commercial plugin license |

Same **`superd`** and **`super`** binaries for both columns — **Licensed** is OSS plus commercial plugins: drop `plugins/*.so` + `[license].key` in `conf/super.toml` to enable the right-hand column (see [Editions](/docs/07-editions/)).

> [!IMPORTANT]
> `security` is included with every subscription and is **required for startup** when `[license].key` is valid. RBAC, audit logs, and API token auth come from the `security` plugin. See [Authentication](/docs/05-advanced-management/authentication#licensed-deployments-require-security).

## Which setup do I need?

### OSS

*   Personal projects, homelab, or local development.
*   Loopback-first defaults (`127.0.0.1`, `allow_insecure_public_bind = false`); explicit opt-in required for network-facing bind without auth.
*   Trusted private network (VPN/VPC) with firewall in front of the API if you must expose the port.
*   No strict per-process CPU/memory enforcement.

### Licensed

*   **`security.so` + `auth_secret`** — required for any licensed startup (included with subscription). `auth_secret` bootstraps Access Tokens; Admins may explicitly disable it after creating an Admin token.
*   **PaaS** or shared hosting with cgroup isolation (`isolation`, **Linux hosts only**).
*   **Webhook notifications** for on-call (`notify`).
*   **Visual dashboard** (`ui`) — requires `security` for licensed startup.
*   Regulated environments needing **audit logs** (`security`).
*   Exposing API/Dashboard beyond localhost — **`security` is always loaded** when licensed; configure bind and tokens accordingly.
