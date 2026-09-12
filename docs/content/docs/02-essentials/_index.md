---
title: "Essentials"
weight: 2
description: "Core day-to-day use: configuration, auth, Dashboard, process ops, and logging."
---

Now that you have Super running, dig into the capabilities you use every day — including the **embedded Dashboard** and optional **API authentication** that ship with OSS `superd`.

### In this section

*   [**Managed Program Requirements**](./process-management-contract): Hard requirements for supervised applications (read before adding programs).
*   [**Configuration**](./configuration): A deep dive into `super.toml`.
*   [**Authentication**](./authentication): OSS admin secret; multi-user Access Tokens with the `security` plugin.
*   [**Environment & Secrets**](./environment-secrets): `env`, `env_file`, and safe handling of credentials.
*   [**Dashboard**](./web-ui): Browser UI embedded in OSS; Pro surfaces via the `ui` plugin.
*   [**Process Operations**](./process-control): Managing services via the CLI (including `numprocs` multi-process programs).
*   [**Logging**](./logging): How Super handles stdout/stderr and log rotation.
*   [**Scheduled Tasks (Cron)**](./scheduled-tasks): Periodic jobs with overlap protection.

Licensed governance (RBAC, audit, cgroups, storm-suppressed alerting) lives under [Advanced Management](/docs/05-advanced-management/).
