---
title: "Web UI"
weight: 6
description: "Dashboard via the optional ui plugin; OSS is API and CLI only."
imageZoom: true
aliases:
  - /docs/02-essentials/web-ui/
  - /docs/02-essentials/web-ui
---

> [!NOTE]
> The dashboard requires the **`ui`** plugin in `[license].key`. OSS `superd` serves a short notice at `/` instead of a dashboard.

## OSS vs subscription

| Edition | Web UI at `/` |
| :--- | :--- |
| **OSS** (no plugins) | Static notice — **no dashboard**. Use `super` CLI or `/api/v1/*`. Links to [Get Super Pro](https://super.docs.sconts.com/go/pro/) and the [feature matrix](/docs/07-editions/feature-matrix). |
| **Licensed** | Full dashboard served by the authorized UI plugin. |

OSS `superd` does **not** embed a dashboard. The optional **`ui`** plugin serves it at runtime via `super_plugin_ui_v1` after you add a license key and plugin libraries — see [Get Super Pro](https://super.docs.sconts.com/go/pro/).

## Accessing the dashboard (licensed)

With the `ui` plugin loaded and authorized in `[license].key`:

**http://localhost:9002**

> [!NOTE]
> Assuming `port = 9002` in your config

Log in with an **Access Token** (`sk-…`) when the **`security`** plugin is enabled. Prefer generated tokens for day-to-day use; config `auth_secret` remains usable until an Admin explicitly disables it. See [Authentication](/docs/05-advanced-management/authentication).

## Dashboard tour

Screenshots below are from a licensed deployment (`docs/static/images/`). Use the tabs to browse each area — images are capped in width; **click to enlarge**.

{{< tabs >}}

  {{< tab name="Overview" icon="view-grid" >}}
Process list with host CPU/memory metrics (from the machine running **superd**), status filters, search, and topology view.

{{< ui-screenshot src="/images/overview.png" alt="Dashboard overview — process list and host metrics" caption="Overview — programs, host metrics, filters" >}}
  {{< /tab >}}

  {{< tab name="Program detail" icon="cog" >}}
Process detail drawer: actions, configuration (command, hooks, health checks, resource limits, environment).

{{< ui-screenshot src="/images/program_config.png" alt="Program configuration in the detail drawer" caption="Program detail — Configuration" >}}
  {{< /tab >}}

  {{< tab name="Logs" icon="terminal" >}}
Live stdout/stderr streaming from the process detail drawer, plus file log history.

{{< ui-screenshot src="/images/program_logtails.png" alt="Live program log tail in the detail drawer" caption="Program detail — Logs" >}}
  {{< /tab >}}

  {{< tab name="Inhibition rules" icon="bell" >}}
**Notifications → Inhibition rules** when the **`notify`** plugin is licensed: When → Mute targets → For (cross-event storm suppression).

See [Event notifications](/docs/05-advanced-management/event-notifications#storm-suppression).

{{< ui-screenshot src="/images/notify_mute.png" alt="Notification settings — Inhibition rules tab" caption="Notifications — Inhibition rules" >}}
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
| **Notifications** | Webhooks + Inhibition rules (`notify` plugin) |

The dashboard also includes create/edit program forms, a [stack editor](/docs/04-production-scenarios/delivery/declarative-stack), API token management, and a license page — not shown above.

## Security

**Without `security` plugin (OSS only):** The API and dashboard static assets are reachable without authentication on the bind address. OSS defaults to loopback-only startup (`allow_insecure_public_bind = false`).

**Licensed:** `security` is bundled and **must load** — startup fails otherwise. Dashboard and API require token auth.

**With `security` plugin loaded:** Token authentication and RBAC apply to the API and dashboard. Prefer generated Access Tokens for day-to-day login. `auth_secret` remains usable until an Admin explicitly disables it (after creating an Admin token). See [Access control](/docs/05-advanced-management/access-control) and [Authentication](/docs/05-advanced-management/authentication).

> [!WARNING]
> OSS exposure beyond localhost requires explicit `allow_insecure_public_bind = true` or the **`security` plugin**. Licensed deployments always load `security`.
