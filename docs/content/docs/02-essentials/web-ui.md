---
title: "Dashboard"
weight: 5
description: "Browser Dashboard embedded in OSS superd; subscription ui plugin adds Pro-only surfaces."
imageZoom: true
aliases:
  - /docs/05-advanced-management/web-ui/
  - /docs/05-advanced-management/web-ui
---

The Dashboard is the browser UI for `superd`. **OSS builds embed the shell** (process overview, detail drawer, stack editor, license page, login). Subscription installs can load the optional **`ui` plugin**, which registers **Pro-only** pages and actions — those stay hidden until the matching plugins are licensed.

## OSS vs subscription

| Edition | Dashboard at `/` |
| :--- | :--- |
| **OSS** (no plugins) | **Embedded shell** — overview, logs, stack, program create/edit, license CTA, optional [core auth](/docs/02-essentials/authentication/#oss-admin-secret) login. |
| **Licensed** + **`ui` plugin** | Same shell, plus Pro extensions when peer plugins load (see [Pro UI extensions](#pro-ui-extensions) below). |

> [!TIP]
> You do **not** need the `ui` plugin to open a basic Dashboard on OSS.

## Accessing the Dashboard

**http://localhost:9002** (default; see `port` in config)

- **OSS / loopback:** open by default. Set `auth_secret` in `conf/super.toml` to require the admin Bearer — see [Authentication](/docs/02-essentials/authentication).
- **Licensed:** the **`security`** plugin is required at startup. Prefer generated Access Tokens (`sk-…`) for day-to-day login; config `auth_secret` remains usable until an Admin disables it.

## Dashboard tour (OSS shell)

Screenshots below show the embedded shell (`docs/static/images/`). Use the tabs to browse each area — images are capped in width; **click to enlarge**.

{{< tabs >}}

  {{< tab name="Overview" icon="view-grid" >}}
Process list with host CPU/memory metrics (from the machine running **superd**), status filters, search, and topology view.

{{< ui-screenshot src="/images/overview.png" alt="Dashboard overview — process list and host metrics" caption="Overview — programs, host metrics, filters" >}}
  {{< /tab >}}

  {{< tab name="Program detail" icon="cog" >}}
Process detail drawer: actions, configuration (command, hooks, health checks, resource limits, environment). **Create / Edit Program** also exposes an **OTA Artifact** section (source, checksum, destination, extract, restart policy, download/verify timeouts). Saving with a **new checksum** triggers transactional OTA — same rule as the API/CLI; see [Atomic OTA Updates — When OTA runs](/docs/03-orchestration/ota-updates#when-ota-runs).

{{< ui-screenshot src="/images/program_config.png" alt="Program configuration in the detail drawer" caption="Program detail — Configuration" >}}
  {{< /tab >}}

  {{< tab name="Logs" icon="terminal" >}}
Live stdout/stderr streaming from the process detail drawer, plus file log history.

{{< ui-screenshot src="/images/program_logtails.png" alt="Live program log tail in the detail drawer" caption="Program detail — Logs" >}}
  {{< /tab >}}

{{< /tabs >}}

## Pro UI extensions

💎 **Subscription.** With a valid license, install the **`ui`** plugin into `$SUPER_ROOT/plugins/` and restart `superd`. The shell then loads extension assets and nav only when peer plugins are present — OSS never shows empty Pro stubs.

| Surface | Needs | Docs |
| :--- | :--- | :--- |
| Access Tokens (Account menu) | `security` + `ui` | [Authentication](/docs/02-essentials/authentication/) · [Access control](/docs/05-advanced-management/access-control) |
| Notification Settings (Webhooks / Inhibition / Delivery) | `notify` + `ui` | [Event notifications](/docs/05-advanced-management/event-notifications) |
| Process hot-reload action | `ui` (+ license) | Process detail action strip |

## Feature summary

| Area | OSS shell | Subscription (`ui` + peers) |
| :--- | :--- | :--- |
| **Overview / detail / logs** | ✅ | ✅ (+ optional Hot Reload) |
| **Stack editor / create-edit** | ✅ | ✅ |
| **License page** | ✅ (Community CTA) | ✅ (status when licensed) |
| **Access Tokens** | — | ✅ |
| **Notification Settings** | — | ✅ (see [Event notifications](/docs/05-advanced-management/event-notifications)) |

## Security

**OSS:** Dashboard and API follow [core auth](/docs/02-essentials/authentication/#oss-admin-secret) (loopback open by default; set `auth_secret` to require login; non-loopback without secret refuses to start). Multi-user tokens are not available without `security`.

**Licensed:** `security` **must** load — startup fails otherwise. Prefer generated Access Tokens for day-to-day login. See [Access control](/docs/05-advanced-management/access-control) and [Authentication](/docs/02-essentials/authentication).

> [!WARNING]
> Binding beyond localhost without a secret is unsafe. Non-loopback binds already require [core auth](/docs/02-essentials/authentication/) (or the `security` plugin when licensed). Prefer loopback, a reverse proxy with TLS, or a licensed multi-user deployment.
