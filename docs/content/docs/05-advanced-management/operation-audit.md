---
title: "Operation Audit"
weight: 4
description: "Track 'Who did What and When'."
---

> [!IMPORTANT] Licensed feature — `security` plugin
> This page covers a **licensed feature** provided by the **`security` plugin**, which is included with every subscription and **required for licensed startup**. It needs a valid `[license].key`, the plugin library in `$SUPER_ROOT/plugins/`, and `auth_secret`. OSS `superd` without the plugin writes no audit log.

### Audit Logging 💎

For compliance and security analysis, knowing the current state isn't enough—you need a history of changes. Super maintains a secure, write-only audit log of all administrative actions.

## Log Format

Audit logs are stored separately from application logs, typically in `./logs/audit.log`. They follow a structured text format:

```text
[TIMESTAMP] IP=... User=[NAME (ROLE)] Action=METHOD Path=URI Status=CODE
```

## Example Entries

**1. Admin logs in and creates a token:**
```text
[2023-10-27 10:00:01] IP=192.168.1.50 User=[RootAdmin (Admin)] Action=POST Path=/api/v1/auth/tokens Status=201
```

**2. Operator restarts a service:**
```text
[2023-10-27 10:05:23] IP=192.168.1.50 User=[ci-bot (Operator)] Action=POST Path=/api/v1/programs/api-server/restart Status=200
```

**3. Viewer attempts unauthorized action (Denied):**
```text
[2023-10-27 10:15:00] IP=10.0.0.5 User=[guest (Viewer)] Action=DELETE Path=/api/v1/programs/db Status=403
```

## System Events

Internal system lifecycle events are also recorded in the audit log for a complete timeline:

```text
[2023-10-27 09:00:00] [SYSTEM] Superd Boot: production-server-01
[2023-10-27 09:00:01] [SYSTEM] Process 'api-server' started.
[2023-10-27 14:30:00] [SYSTEM] 🚨 FATAL: 'worker-proc' - Stopped after 3 retries.
```
