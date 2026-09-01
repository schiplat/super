---
title: "vs Supervisor"
weight: 1
description: "Why migrate from Supervisord? Zero dependencies, TOML config, and a modern JSON API."
---

[Supervisor](http://supervisord.org/) has been the industry standard for process management for over a decade. It is stable and battle-tested. However, it was designed in an era before containers, microservices, and modern DevOps pipelines.

Here is why **Project Super** is the modern successor.

## 1. The Dependency Tax

Supervisor is written in Python. To run it, you must install a Python interpreter and the necessary libraries.

*   **Bare Metal**: You have to manage Python versions and `pip` environments.
*   **Containers**: Adding Python to a minimal base image (like Alpine or Distroless) often **doubles** the image size.

**Comparison:**

| Feature | Supervisor (Python) | Project Super (Rust) |
| :--- | :--- | :--- |
| **Runtime Requirement** | Python 2.7 or 3.x | **None (Static Binary)** |
| **Docker Base Image** | `python:slim` or manual install | `scratch` or `alpine` |
| **Disk Footprint** | ~50MB+ (Interpreter + Libs) | **~15MB** (`superd` + `super`, release build) |

## 2. Configuration: INI vs TOML

Supervisor uses the INI format, which lacks nested structures and typing. Super uses **TOML**, which maps 1:1 to JSON and supports arrays, tables, and strong types.

**Supervisor (`supervisord.conf`):**
```ini
[program:my-app]
command=/bin/app
autostart=true
autorestart=true
environment=KEY="val",KEY2="val2"
```

**Super (daemon config is TOML; programs are stack files (TOML default, JSON compatible) or API/CLI):** — example `conf/conf.d/my-app.json`:

```json
{
  "services": [
    {
      "name": "my-app",
      "command": "/bin/app",
      "autostart": true,
      "retry_limit": 3,
      "env": {
        "KEY": "val",
        "KEY2": "val2"
      }
    }
  ]
}
```

## 3. The API Gap: XML-RPC vs REST

This is the most significant difference for DevOps automation.

### Supervisor: XML-RPC
Supervisor exposes an XML-RPC interface. It is notoriously difficult to interact with unless you use a specific client library.

*   **Debugging**: You cannot simply `curl` it to see the status.
*   **Integration**: Integrating with modern dashboards or CI/CD requires writing complex XML-RPC wrappers.

### Super: REST & WebSockets
Super adopts an **API-First** design. The CLI is just a wrapper around the HTTP API.

**Get Status:**
```bash
curl http://localhost:9002/api/v1/programs
```

**Restart a Process** (API paths use program **UUID**, not name):

```bash
ID=$(curl -s http://localhost:9002/api/v1/programs | jq -r '.[] | select(.name=="my-app") | .id')
curl -X POST "http://localhost:9002/api/v1/programs/${ID}/restart"
```

**Real-time Logs:**
Connect to `ws://localhost:9002/ws?id=...` to stream logs instantly. No polling required.

## Migration Cheatsheet

Mapping your muscle memory from `supervisorctl` to `super`:

| Action | supervisorctl | super |
| :--- | :--- | :--- |
| **Check Status** | `supervisorctl status` | `super list` |
| **Start Process** | `supervisorctl start <name>` | `super start <name>` |
| **Tail Logs** | `supervisorctl tail -f <name>` | `super logs <name>` |
| **Reload Config** | `supervisorctl reread && update` | `super update <name> ...` |
| **Group Action** | `supervisorctl restart <group>:` | `super restart @<group>` |
| **Reload app config** | `supervisorctl signal HUP <name>` | `super reload <name>` |
| **Reload daemon config** | `supervisorctl reload` | `super reload` *(no target)* |
| **Stop the manager** | `supervisorctl shutdown` | `super shutdown` |

`supervisord` often daemonizes by default; `superd` stays in the **foreground** unless you pass `--daemon` / set `[server] daemon = true` (Unix, not for systemd/Docker). Child programs must still run with `daemon off` — see [Managed Program Requirements](/docs/02-essentials/process-management-contract/).

## Supervisor Migration: `reread` / `update` / `reload`

Supervisor and Super use different configuration models. Use this table when migrating automation:

| Supervisor | What it does | Super equivalent |
| :--- | :--- | :--- |
| **`supervisorctl reread`** | Re-read config files into memory; **does not** change running processes | No 1:1 command. Edit `super.toml` / stack JSON locally; changes apply on next explicit action. |
| **`supervisorctl update`** | Apply config changes; **may** start new programs and restart changed ones | **`super update <name> ...`** updates persisted config. Does **not** auto-restart unless you also change OTA `artifact` checksum or run **`super restart <name>`**. |
| **`supervisorctl reload`** | Re-read **supervisord** main config (not program sections) | **`super reload`** *(no target)* — reloads system config (`super.toml`), log level, includes. |
| **`supervisorctl restart <name>`** | Stop then start one program | **`super restart <name>`** |
| **`supervisorctl signal HUP <name>`** | Send signal to running process | **`super reload <name>`** or **`super signal <name> hup`** |

### Decision guide

1. **Changed program command/env only** → `super update <name> ...` then `super restart <name>` if it is running.
2. **Changed global server settings** → `super reload` (no target).
3. **App supports SIGHUP config reload (nginx, etc.)** → `super reload <name>` without restart.
4. **Deploy new binary** → use OTA `artifact` block, or replace binary + `super restart <name>`.
5. **Zero-downtime release** → use load balancer / blue-green; Super intentionally does **not** run two instances of the same program (same as Supervisor).

### Fields mapped from Supervisor

| Supervisor | Super |
| :--- | :--- |
| `stopwaitsecs` | `stopsecs` (optional; else `[server].shutdown_timeout`) |
| `priority` | `priority` (lower starts first; complements `depends_on`) |
| `stdout_logfile` | `stdout_logfile` (must resolve under `storage.log_dir`) |
| `stderr_logfile` | `stderr_logfile` (must resolve under `storage.log_dir`) |
| `startretries` | `retry_limit` |
| `autorestart=unexpected` | `autorestart = "unexpected"` + `exitcodes` |

### Not yet mapped (use workarounds)

| Supervisor | Workaround |
| :--- | :--- |
| `stopsignal` (per program) | `super signal <name> <sig>` manually; default stop uses SIGTERM |
| `redirect_stderr=true` | Not supported; stdout/stderr are separate files |
| `[eventlistener:x]` | OSS `[[event_hooks]]` + licensed `notify.toml`; see [Event Hooks](/docs/03-orchestration/events/hooks) |
