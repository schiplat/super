---
title: "Dependencies"
weight: 1
description: "Define startup order using the depends_on directive."
---

In a microservices architecture, services often have strict startup orders. For example, a backend API cannot accept requests until the database is ready.

Super allows you to define these relationships using the `depends_on` field on each program (stack files, API, or CLI).

## Configuration

Example: `conf/conf.d/deps.json`:

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
2.  If `postgres-db` is not running, Super starts it automatically (through the normal start path).
3.  **Crucially**, Super waits for `postgres-db` to become **Healthy** (pass its health check).
4.  Only then does `backend-api` start.

A dependency that is already busy in its own lifecycle (waiting for *its* dependencies, restarting, or crashed) is left alone — Super does not force-start it. This also breaks dependency cycles: if `A` depends on `B` and `B` depends on `A`, both simply stay in `Waiting` until you break the cycle manually.

## State: "Waiting"

If a dependency is unhealthy or not yet running, the dependent process enters the `Waiting` state.

```text
$ super list

ID        Name          Status    Notes
--------  ------------  --------  ------------------------
a1b2...   backend-api   Waiting   Waiting for: postgres-db
c3d4...   postgres-db   Starting  ...
```

Once `postgres-db` turns `Healthy`, `backend-api` automatically transitions to `Starting`.

## Invalid references

`depends_on` names are validated when the configuration is submitted — via `super apply`, `super add`, or the create/update API. A name that matches no program (typo, or a service removed from the stack) is rejected up front:

```text
$ super apply deps.toml
Error: services[1] (name=backend-api): depends_on: unknown service(s): postgres-db
```

Within a single stack file, forward references are allowed: a service may depend on another service defined later in the same file.

Two runtime cases remain (legacy configs restored from a snapshot, or a dependency removed by another operator):

- A **dangling name** keeps the dependent in `Waiting` (never `Fatal`) and the reason is surfaced in `super status backend-api` under `Last error` (for example, `Dependency 'postgres-db' not found (config error)`). The log carries a matching `warn` entry. As soon as a program with that name becomes Healthy, the dependent starts automatically.
- **Removing** a program that a `Waiting` dependent references refreshes the dependent's `Last error` with a removal notice, so the cause is visible without reading logs.
