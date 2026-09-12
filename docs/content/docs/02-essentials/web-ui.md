---
title: "Dashboard"
weight: 5
description: "Browser Dashboard embedded in OSS superd; subscription ui plugin adds Tokens, Notify, and other Pro surfaces."
imageZoom: true
aliases:
  - /docs/05-advanced-management/web-ui/
  - /docs/05-advanced-management/web-ui
---

The Dashboard is the browser UI for `superd`. **OSS builds embed the shell** (process overview, detail drawer, stack editor, license page, login). With a subscription, the optional **`ui` plugin** loads a small extension bundle that registers **Pro-only pages and actions** (Access Tokens, Notification Settings, hot-reload) — those entries stay hidden until the matching plugins are licensed.

## OSS vs subscription

| Edition | Dashboard at `/` |
| :--- | :--- |
| **OSS** (no plugins) | **Embedded shell** — overview, logs, stack, program create/edit, license CTA, optional [core auth](/docs/02-essentials/authentication#oss-built-in-auth-single-admin-secret) login. No Tokens / Notify menus. |
| **Licensed** + **`ui` plugin** | Same shell, plus Pro extensions (Tokens, Notifications, process hot-reload) when `security` / `notify` (and peers) are loaded. |

> [!TIP]
> You do **not** need the `ui` plugin to open a basic Dashboard on OSS. Add `ui` (and a license) when you want the Pro UI surfaces listed below.

## Accessing the Dashboard

**http://localhost:9002** (default; see `port` in config)

- **OSS / loopback:** open by default. Set `[server].auth_required = true` (or bind beyond loopback) to require the admin Bearer secret — see [Authentication](/docs/02-essentials/authentication).
- **Licensed:** the **`security`** plugin is required at startup. Prefer generated Access Tokens (`sk-…`) for day-to-day login; config `auth_secret` remains usable until an Admin disables it.

## Dashboard tour

Screenshots below are from a licensed deployment (`docs/static/images/`). Use the tabs to browse each area — images are capped in width; **click to enlarge**.

{{< tabs >}}

  {{< tab name="Overview" icon="view-grid" >}}
Process list with host CPU/memory metrics (from the machine running **superd**), status filters, search, and topology view. Available in OSS and licensed installs.

{{< ui-screenshot src="/images/overview.png" alt="Dashboard overview — process list and host metrics" caption="Overview — programs, host metrics, filters" >}}
  {{< /tab >}}

  {{< tab name="Program detail" icon="cog" >}}
Process detail drawer: actions, configuration (command, hooks, health checks, resource limits, environment). **Create / Edit Program** also exposes an **OTA Artifact** section (source, checksum, destination, extract, restart policy, download/verify timeouts). Saving with a **new checksum** triggers transactional OTA — same rule as the API/CLI; see [Atomic OTA Updates — When OTA runs](/docs/03-orchestration/ota-updates#when-ota-runs).

Licensed installs with the `ui` plugin may show extra actions (for example **Hot Reload** / SIGHUP) in the process action strip.

{{< ui-screenshot src="/images/program_config.png" alt="Program configuration in the detail drawer" caption="Program detail — Configuration" >}}
  {{< /tab >}}

  {{< tab name="Logs" icon="terminal" >}}
Live stdout/stderr streaming from the process detail drawer, plus file log history.

{{< ui-screenshot src="/images/program_logtails.png" alt="Live program log tail in the detail drawer" caption="Program detail — Logs" >}}
  {{< /tab >}}

  {{< tab name="Inhibition rules" icon="bell" >}}
**Notification Settings** when the **`notify`** plugin **and** the **`ui`** plugin are licensed — three routes under `/settings/notify/`:

| Route | Page |
| :--- | :--- |
| `/settings/notify/webhooks` | Webhooks + delivery strategy |
| `/settings/notify/rules` | Inhibition rules (When → Mute targets → For) |
| `/settings/notify/delivery` | Persisted delivery history (OK / Fail / Cooldown / Inhibited) |

These pages are **Pro Dashboard extensions** (not part of the OSS shell). Without `notify` / `ui`, the Notifications menu does not appear. See [Event notifications](/docs/05-advanced-management/event-notifications#storm-suppression) and [Delivery history](/docs/05-advanced-management/event-notifications#delivery-history).

{{< ui-screenshot src="/images/notify_rules.png" alt="Notification settings — Inhibition rules" caption="Notifications — Inhibition rules" >}}
  {{< /tab >}}

{{< /tabs >}}

## Deploy the ui plugin (Pro extensions)

Install the **`ui`** plugin library from your subscription package into `$SUPER_ROOT/plugins/` (instance root from [`SUPER_ROOT`](/docs/06-internals/environment-variables#super_root)). Pair it with the plugins whose UI you need (`security` for Tokens, `notify` for Notification Settings).

Restart `superd` after updating plugins. The shell loads the plugin’s extension assets and registers routes / nav items only when those capabilities are present — OSS installs never show empty Pro stubs.

## Feature summary

| Area | OSS shell | With subscription `ui` + peer plugins |
| :--- | :--- | :--- |
| **Overview / detail / logs** | ✅ | ✅ (+ optional Hot Reload action) |
| **Stack editor / create-edit** | ✅ | ✅ |
| **License page** | ✅ (Community CTA) | ✅ (status when licensed) |
| **Access Tokens** | — | ✅ (`security` + `ui`) — Account menu |
| **Notifications** | — | ✅ (`notify` + `ui`) — Webhooks, Inhibition rules, Delivery |

## Security

**OSS:** Dashboard and API follow [core auth](/docs/02-essentials/authentication#oss-built-in-auth-single-admin-secret) (loopback open by default; non-loopback or `auth_required` requires the admin secret). Multi-user tokens are not available without `security`.

**Licensed:** `security` **must** load — startup fails otherwise. Prefer generated Access Tokens for day-to-day login. See [Access control](/docs/05-advanced-management/access-control) and [Authentication](/docs/02-essentials/authentication).

> [!WARNING]
> Binding beyond localhost without authentication is unsafe. Prefer loopback, a reverse proxy with TLS, or a licensed `security` deployment.
