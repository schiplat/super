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
| **Log Rotation & Streaming** | ✅ | ✅ |
| **Prometheus Metrics**<br>Basic scrape in OSS; plugin metrics when licensed | ✅ | ✅ |
| **Historical Logs API** | ✅ | ✅ |
| **System Stats API** | ✅ | ✅ |
| **Event reactions** (`[[event_hooks]]`) | ✅ <br>local scripts **or** webhook POST (`command` or `url`) | ✅ <br>same hooks; optional `notify` alongside |
| **Cron Scheduled Tasks** | ✅ | ✅ |
| **RBAC (User Roles)**<br>`security` plugin (**required** for licensed startup) | ❌ | ✅ |
| **Audit Logging**<br>`security` plugin (**required** for licensed startup) | ❌ | ✅ |
| **Linux Cgroups Isolation**<br>`isolation` plugin (**Linux only**) | ❌ | ✅ |
| **Dashboard**<br>`ui` plugin; OSS is API/CLI only | ❌ | ✅ |
| **Alerting**<br>`notify` plugin: IM templates, multi-channel routing, [storm suppression](/docs/05-advanced-management/event-notifications#storm-suppression) | ❌ | ✅ |
| **License** | MIT | Commercial |

Same **`superd`** and **`super`** binaries for both columns — **Licensed** is OSS plus commercial plugins: drop `plugins/*.so` + `[license].key` in `conf/super.toml` to enable the right-hand column (see [Editions](/docs/07-editions/)).

To purchase, renew, or request a trial for the commercial plugin set, see **[Get Super Pro](/go/pro/)** (checkout and license version coverage). Existing subscribers can also sign in to the **[Super Pro Portal](https://platform.ddl.sconts.com/portal/login)**.

> [!IMPORTANT]
> `security` is included with every subscription and is **required for startup** when `[license].key` is valid. RBAC, audit logs, and API token auth come from the `security` plugin. See [Authentication](/docs/05-advanced-management/authentication#licensed-deployments-require-security).

## Event hooks vs alerting

**OSS has one reaction mechanism:** `[[event_hooks]]` in `super.toml` — use `command` for local scripts or `url` for a basic webhook POST (raw event JSON). There is no separate “webhook notifications” row or config in OSS; webhook POST **is** an event hook.

**Licensed alerting** is the optional **`notify`** plugin (`conf/notify.toml`) — Slack/钉钉/Teams presets, channel routing, cooldown/batch, and inhibition rules. It listens to the *same* events; you can use **both** hooks and `notify` together.

## Which setup do I need?

### OSS

*   Personal projects, homelab, or local development.
*   Loopback-first defaults (`127.0.0.1`, `allow_insecure_public_bind = false`); explicit opt-in required for network-facing bind without auth.
*   Trusted private network (VPN/VPC) with firewall in front of the API if you must expose the port.
*   No strict per-process CPU/memory enforcement.

### Licensed

*   **`security.so` + `auth_secret`** — required for any licensed startup (included with subscription). `auth_secret` bootstraps Access Tokens; Admins may explicitly disable it after creating an Admin token.
*   **PaaS** or shared hosting with cgroup isolation (`isolation`, **Linux hosts only**).
*   **Production alerting** (`notify` plugin) — IM/webhook channels and [storm suppression](/docs/05-advanced-management/event-notifications#storm-suppression); complements (does not replace) `[[event_hooks]]`.
*   **Visual dashboard** (`ui`) — requires `security` for licensed startup.
*   Regulated environments needing **audit logs** (`security`).
*   Exposing API/Dashboard beyond localhost — **`security` is always loaded** when licensed; configure bind and tokens accordingly.
