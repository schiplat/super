---
title: "Web UI"
weight: 6
description: "Dashboard via the optional ui plugin; OSS is API and CLI only."
imageZoom: true
aliases:
  - /docs/02-essentials/web-ui/
  - /docs/02-essentials/web-ui
---

> **Licensed plugin 💎:** The dashboard requires the **`ui`** plugin in `[license].key`. OSS `superd` serves a short notice at `/` instead of a dashboard.

## OSS vs subscription

| Edition | Web UI at `/` |
| :--- | :--- |
| **OSS** (no plugins) | Static notice — **no dashboard**. Use `super` CLI or `/api/v1/*`. Links to [Get Super Pro](https://super.docs.sconts.com/go/pro/) and the [feature matrix](/docs/07-editions/feature-matrix). |
| **Licensed** | Full dashboard served by the authorized UI plugin. |

OSS `superd` does **not** embed a dashboard. The optional **`ui`** plugin serves it at runtime via `super_plugin_ui_v1` after you add a license key and plugin libraries — see [Get Super Pro](https://super.docs.sconts.com/go/pro/).

## Accessing the dashboard (licensed)

With the `ui` plugin loaded and authorized in `[license].key`:

**http://localhost:9002**

{{< callout icon="sparkles" >}}
  Assuming `port = 9002` in your config
{{< /callout >}}

Log in with an **Access Token** (`sk-…`) when the **`security`** plugin is enabled. Prefer generated tokens for day-to-day use; config `auth_secret` remains usable until an Admin explicitly disables it. See [Authentication](/docs/05-advanced-management/authentication).

## Dashboard tour

Screenshots below are from a licensed deployment. Use the tabs to browse each area — images are capped in width; **click to enlarge**.

{{< tabs >}}

  {{< tab name="Overview" icon="view-grid" >}}
Process list with host CPU/memory metrics (from the machine running **superd**), status filters, search, and topology view.

{{< ui-screenshot src="/images/overview.png" alt="Dashboard overview — process list and host metrics" >}}
  {{< /tab >}}

  {{< tab name="Program detail" icon="cog" >}}
Configuration drawer: command, hooks, health checks, resource limits, and environment for a selected program.

{{< ui-screenshot src="/images/program_config.png" alt="Program configuration drawer" >}}
  {{< /tab >}}

  {{< tab name="Logs" icon="terminal" >}}
Live stdout/stderr streaming from the process detail drawer.

{{< ui-screenshot src="/images/program_logtails.png" alt="Live program log tail" >}}
  {{< /tab >}}

  {{< tab name="Hot reload" icon="refresh" >}}
Reload plugin or dashboard assets without a full daemon restart (development workflow).

{{< ui-screenshot src="/images/reload_online.png" alt="Online reload controls" >}}
  {{< /tab >}}

  {{< tab name="Notifications" icon="bell" >}}
Notification Settings when the **`notify`** plugin is licensed — two tabs:

- **Webhooks** — destinations, triggers, and per-webhook Delivery Strategy
- **Inhibition rules** — When → Mute targets → For (cross-event storm suppression)

See [Event notifications](/docs/05-advanced-management/event-notifications#storm-suppression).

{{< ui-screenshot src="/images/notify.png" alt="Notification settings" >}}
  {{< /tab >}}

{{< /tabs >}}

## Deploy the ui plugin

Install the **`ui`** plugin library from your subscription delivery package into `$SUPER_ROOT/plugins/`.

Restart `superd` after updating plugins.

## Feature summary

| Area | What you get |
| :--- | :--- |
| **Overview** | Process counts, host metrics, filters, list/graph views |
| **Program detail** | Config, hooks, health checks, live logs, start/stop/restart |
| **Hot reload** | Reload plugins/dashboard without restarting `superd` |
| **Notifications** | Webhooks + Inhibition rules (`notify` plugin) |

The dashboard also includes create/edit forms, a [declarative stack editor](/docs/04-production-scenarios/delivery/declarative-stack), API token management, and a license page — not shown above.

## Security

**Without `security` plugin (OSS only):** The API and dashboard static assets are reachable without authentication on the bind address. OSS defaults to loopback-only startup (`allow_insecure_public_bind = false`).

**Licensed:** `security` is bundled and **must load** — startup fails otherwise. Dashboard and API require token auth.

**With `security` plugin loaded:** Token authentication and RBAC apply to the API and dashboard. Prefer generated Access Tokens for day-to-day login. `auth_secret` remains usable until an Admin explicitly disables it (after creating an Admin token). See [Access control](/docs/05-advanced-management/access-control) and [Authentication](/docs/05-advanced-management/authentication).

> **Security tip:** OSS exposure beyond localhost requires explicit `allow_insecure_public_bind = true` or the **`security` plugin**. Licensed deployments always load `security`.
