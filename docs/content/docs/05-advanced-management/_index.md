---
title: "Advanced Management 💎"
weight: 6
description: "Licensed Super Pro plugins: RBAC, audit, cgroup isolation, and production alerting."
---

> [!NOTE]
> Pages in this section cover **subscription plugins** (`security`, `isolation`, `notify`, and Dashboard Pro extensions via **`ui`**). OSS already includes an [embedded Dashboard](/docs/02-essentials/web-ui/) and [core authentication](/docs/02-essentials/authentication/) — those live under **Essentials**. Each page below states which plugin it requires.

> [!TIP] Free 90-day beta trial
> Super Pro is available during the beta with a **free 90-day trial license** ([Portal claim](https://platform.ddl.sconts.com/portal/claim?product=super-pro&plan=first-trials-001)). Compare editions in the [feature matrix](/docs/07-editions/feature-matrix/) and see the [Toward GA checklist](https://github.com/schiplat/super#toward-ga) on GitHub.

As your infrastructure grows — shared hosts, public binds, regulated environments — **governance** becomes critical: who may restart a process, whether a memory leak can take down the machine, and how incident bursts reach your IM channels without flooding them.

### In this section

*   [**Access Control (RBAC)**](./access-control): Viewer / Operator / Admin roles (`security`).
*   [**Operation Audit**](./operation-audit): Compliance logging for API mutations (`security`).
*   [**Resource Isolation**](./resource-isolation): CPU and memory limits via cgroups (`isolation`, Linux).
*   [**Event Notifications**](./event-notifications): IM/webhook alerts with **storm suppression** — cooldown, batch summaries, and cross-event inhibition (`notify`).

Related Essentials (always available in OSS):

*   [**Authentication**](/docs/02-essentials/authentication/) — single admin secret; multi-user Access Tokens when `security` is loaded
*   [**Dashboard**](/docs/02-essentials/web-ui/) — embedded shell; Pro UI extensions (Access Tokens UI, Notification Settings, hot-reload) via the **`ui`** plugin when peer plugins are loaded
