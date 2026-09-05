---
title: "Advanced Management 🌟"
weight: 5
description: "Security, governance, and observability with optional licensed plugins."
---

> [!NOTE]
> Pages in this section describe capabilities provided by optional plugins (`security`, `isolation`, `notify`, `ui`). OSS `superd` without those plugins does not register the related API routes. Each page states which plugin provides it at the top.

> [!TIP] Free 90-day beta trial
> Super Pro (the licensed plugin set) is available during the beta with a **free 90-day trial license** ([Portal claim](https://platform.ddl.sconts.com/portal/claim?product=super-pro&plan=first-trials-001)). We recommend licensed deployments for staging and non-critical workloads today; see the [feature matrix](/docs/07-editions/feature-matrix/) and the [Toward GA checklist](https://github.com/schiplat/super#toward-ga) on GitHub.

As your infrastructure grows from a single server to a fleet of edge devices or a microservices cluster, **governance** becomes critical.

You need to know **who** executed a restart command, ensure that a memory leak doesn't crash the whole machine, and get notified immediately when a service fails.

This section covers advanced capabilities enabled by licensed plugins.

### In this section

*   [**Dashboard**](./web-ui): Browser Dashboard from the `ui` plugin (OSS is API/CLI only).
*   [**Authentication**](./authentication): Securing the API with tokens (`security`).
*   [**Access Control (RBAC)**](./access-control): Fine-grained permissions (Viewer/Operator/Admin).
*   [**Resource Isolation**](./resource-isolation): CPU and memory limits via cgroups (`isolation`, Linux).
*   [**Operation Audit**](./operation-audit): Compliance logging for API mutations (`security`).
*   [**Event Notifications**](./event-notifications): IM/webhook alerts with **storm suppression** — cooldown, batch summaries, and cross-event inhibition (`notify`).
