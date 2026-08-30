---
title: "Dependencies"
weight: 1
description: "Define startup order using the depends_on directive."
---

In a microservices architecture, services often have strict startup orders. For example, a backend API cannot accept requests until the database is ready.

Super allows you to define these relationships using the `depends_on` field on each program (JSON stack files, API, or CLI).

## Configuration

```json
{
  "services": [
    {
      "name": "postgres-db",
      "command": "/usr/bin/postgres",
      "health_check": { "type": "tcp", "port": 5432 }
    },
    {
      "name": "backend-api",
      "command": "./api-server",
      "depends_on": ["postgres-db"]
    }
  ]
}
```

## How it works

When you start `backend-api` (or when Super autostarts it):

1.  Super checks if `postgres-db` is running.
2.  If `postgres-db` is not running, it starts it.
3.  **Crucially**, Super waits for `postgres-db` to become **Healthy** (pass its health check).
4.  Only then does `backend-api` start.

## State: "Waiting"

If a dependency is missing or unhealthy, the dependent process enters the `Waiting` state.

```text
$ super list

ID        Name          Status    Notes
--------  ------------  --------  ------------------------
a1b2...   backend-api   Waiting   Waiting for: postgres-db
c3d4...   postgres-db   Starting  ...
```

Once `postgres-db` turns `Healthy`, `backend-api` automatically transitions to `Starting`.
