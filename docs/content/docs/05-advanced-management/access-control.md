---
title: "Access Control (RBAC)"
weight: 2
description: "Fine-grained permissions for teams."
---

> [!IMPORTANT] Licensed feature — `security` plugin
> This page covers a **licensed feature** provided by the **`security` plugin**, which is included with every subscription and **required for licensed startup**. It needs a valid `[license].key`, the plugin library in `$SUPER_ROOT/plugins/`, and `auth_secret`. OSS `superd` without the plugin does not register the RBAC / role APIs.

### Role-Based Access Control 💎

When multiple engineers manage a system, you don't want everyone to have `root` access. The `security` plugin implements RBAC to separate concerns.

## Available Roles

| Role | Permissions | Ideal For |
| :--- | :--- | :--- |
| **Viewer** | **Read-Only**. Process status, logs, metrics; stack/notify configs with secrets redacted. Can renew own Access Token. | Developers debugging issues, Dashboard displays. |
| **Operator** | **Operational Actions**. Create programs; manage notification channels; Start, Stop, Restart, and Signal processes. Cannot edit/delete programs, apply stack, or revoke tokens. Stack reads are redacted. Can renew own token. | SREs, On-call engineers, Automated Watchdogs. |
| **Admin** | **Full Access**. Program CRUD, plaintext configs, all tokens, disable `auth_secret`, system reload/shutdown. | System Administrators, CI/CD Deployment pipelines. |

## Usage Example

### Scenario: The Junior Developer

You want a developer to be able to view logs and restart their service, but not delete the database service or change the root configuration.

**1. Create an Operator Token** (Admin `sk-…` token, or `auth_secret` during an active root bootstrap session):

```bash
curl -X POST http://127.0.0.1:9002/api/v1/auth/tokens \
  -H "Authorization: Bearer <admin-token-or-auth_secret>" \
  -H "Content-Type: application/json" \
  -d '{"name":"dev-team","role":"operator"}'
```

After the first Admin Access Token exists, an Admin may optionally **disable** `auth_secret` — see [Authentication](/docs/05-advanced-management/authentication#optional-disable-auth_secret).

**2. Developer usage:**

```bash
# Restart allowed
curl -X POST -H "Authorization: Bearer sk-..." \
  http://127.0.0.1:9002/api/v1/programs/<id>/restart

# Delete denied (403 Forbidden)
curl -X DELETE -H "Authorization: Bearer sk-..." \
  http://127.0.0.1:9002/api/v1/programs/<database-id>
```
