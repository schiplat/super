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

The shell uses a left sidebar with two groups: **Overview · Programs · Stacks · Events** and **Settings** (Authentication on OSS admin sessions, License). The top bar carries connection state, theme, and the account menu.

- **Overview** — status counters, a *Needs attention* list (failing / flapping / unhealthy programs), recent events, and host CPU/memory trends.
- **Programs** — the process table (search, group and status filters, pagination) and the per-program detail page at `/programs/<id>`: status hero with live tiles (CPU / memory / uptime / pid), a Logs section (live stream or file history with stdout/stderr and tail controls), an Events timeline scoped to the program, and the full read-only Configuration reference. Lifecycle buttons (Start / Restart / Stop) are pinned to the bottom bar.
- **New / Edit Program** — a sectioned form with an anchor rail (Run · Lifecycle · Scheduling · Health · Updates · Hooks · Logs · Depends on · Limits), completion dots, and a sticky footer; create additionally offers a read-only JSON preview of the request.
- **Stacks** — the JSON stack editor with diff/confirm before apply.
- **Events** — the global event history across every program (filters, severity chips, pagination).
- **Settings → Authentication** — SSO (OIDC) provider, client secret, ordered role rules, sessions and audit; see [SSO (OIDC)](/docs/02-essentials/sso/). License status lives in the account menu.

Pro extensions keep registering through the same slot system (extra pages, process actions, detail tabs); those stay hidden until the matching plugins are licensed.
