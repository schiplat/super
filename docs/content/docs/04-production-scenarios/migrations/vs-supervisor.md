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
5. **Zero-downtime release** → use load balancer / blue-green. Super (like Supervisor) does **not** dual-run old and new versions of the same program during a deploy. For **N concurrent workers** from one definition, use `numprocs` (below) — that is multi-process workers, not a rolling release.

### Fields mapped from Supervisor

| Supervisor | Super |
| :--- | :--- |
| `command` / `directory` / `user` | `command` (+ args) / `cwd` / `user` |
| `environment=K="v",…` | `env` map (`%(ENV_X)s` expands at import time) |
| `autostart` | `autostart` |
| `numprocs` | `numprocs` (spawn N processes; see [Process Operations](/docs/02-essentials/process-control/#multi-process-programs-numprocs)) |
| `process_name` | `process_name` (template with `{num}`; default `{name}-{num}`) |
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
| `[eventlistener:x]` | OSS `[[event_hooks]]` + licensed `notify.toml`; see [Event Hooks](/docs/03-orchestration/events/hooks) |

## Import tool: `super import supervisor`

The CLI converts an existing `supervisord.conf` (and its `[include]` files) into a Super stack automatically — parsing INI, mapping fields, and reporting everything it could not carry over:

```bash
# Preview the plan and warnings; works without a running daemon
super import supervisor /etc/supervisor/conf.d/app.conf --dry-run

# Review first, apply later: write a stack TOML draft instead of applying
super import supervisor /etc/supervisor/conf.d/app.conf --emit-toml app-stack.toml
super apply app-stack.toml

# Import directly (connects to the daemon, asks for confirmation)
super import supervisor /etc/supervisor/conf.d/app.conf

# Name collisions — pick a mode up front, or decide at the prompt
super import supervisor /etc/supervisor/conf.d/app.conf --on-collision rename                      # import as web-c41f9a, side by side
super import supervisor /etc/supervisor/conf.d/app.conf --on-collision rename --collision-suffix staging  # fixed name: web-staging
super import supervisor /etc/supervisor/conf.d/app.conf --on-collision override                    # replace existing programs in place
super import supervisor /etc/supervisor/conf.d/app.conf                                            # no flag -> prompt asks [y/r/o/N]
```

### Name collisions: skip / rename / override

When a program name in the file already exists on the daemon, `--on-collision` decides what happens:

| Mode | Behaviour | Typical use |
| :--- | :--- | :--- |
| `skip` *(default)* | Existing program untouched, imported entry left out | Idempotent re-runs; fill in only what is missing |
| `rename` | Imports under `{name}-{suffix}` (random 6-char hex, or `--collision-suffix`); prints the old→new mapping | **Gray-release migration**: old program keeps running while you verify the imported copy, then stop/remove the old one |
| `override` | Existing program updated in place with the imported config (typed confirmation: `override` / prompt `o`) | You know the live copy is stale and want the file to win |

`rename` is unique against both the daemon and the rest of the batch, so reruns and duplicate names in one file never collide. `--yes` never escalates to `override` — an in-place takeover always needs the flag or an explicit interactive answer.

### What the importer does

- **Maps 1:1** — `command` (split into args), `directory`, `user`, `autostart`, `autorestart`, `exitcodes`, `startsecs`, `stopwaitsecs`, `priority`, `startretries`, `environment`, `stdout_logfile`/`stderr_logfile`, `redirect_stderr=true` (stderr joins the stdout stream), `numprocs` + `process_name` (`%(process_num)02d` → `{num}`)
- **Expands placeholders** — `%(here)s`, `%(program_name)s`, and `%(ENV_X)s` (the latter from the *importing shell*; each use prints a warning so you can verify the value)
- **Follows `[include] files=`** — relative patterns resolve against the including file; missing or unreadable includes are a hard error so nothing is silently dropped
- **Applies `[group:x] programs=`** — group membership becomes the `group` field on each member
- **Skips daemon machinery silently** — `[supervisord]`, `[unix_http_server]`, `[supervisorctl]`, `[rpcinterface:*]` describe the source daemon itself and have no target equivalent
- **Warns instead of guessing** — `stopsignal=QUIT` (stop always uses SIGTERM here; send QUIT manually), per-program log rotation keys (rotation is configured globally via `[child_logging]`), `umask`, and unknown sections (`[eventlistener:x]`, `[fcgi-program:x]`)

### Safety behaviour

- **Collisions follow `--on-collision`.** `skip` (default) leaves existing programs untouched; `rename` imports side by side under a fresh `{name}-{suffix}` name; `override` updates in place after typed confirmation. See [Name collisions](#name-collisions-skip--rename--override).
- **Imports never prune.** The generated apply has `prune = false` — nothing outside the imported file is touched.
- **`--no-start`** forces `autostart = false` on everything imported, so a later daemon restart does not launch all programs at once. Start them deliberately with `super start <name>`.
- **`--remap-logs`** rewrites foreign absolute log paths (e.g. `/var/log/web/out.log`) to bare file names so they land inside Super's `storage.log_dir`, which is where custom log paths must live. Without the flag, foreign log paths are kept as-is and the apply-side validation will reject paths outside the log dir.
- **Every imported program is stamped `source = import:supervisor`.** The label travels with the program config (snapshot, detail API, `super info`) so you can always tell imported programs apart from hand-created ones — useful during the verify phase of a gray-release migration and when auditing what an import touched. Other write paths stamp their own labels (`cli:add`, `cli:update`, `stack:<file>`, `include:<file>`); a direct API create/update may set `source` explicitly.

### What cannot be converted

| Supervisor | Why | What to do |
| :--- | :--- | :--- |
| `stopsignal=QUIT/HUP/…` | Stop always sends SIGTERM (then SIGKILL after `stopsecs`) | Send the signal manually (`super signal <name> quit`) or accept SIGTERM |
| `[eventlistener:x]` | Different model: hooks + notification plugin | Re-implement with [`[[event_hooks]]`](/docs/03-orchestration/events/hooks) |
| `[fcgi-program:x]` | No FastCGI spawner | Run the FCGI server behind its own process definition |
| Per-program `*_logfile_maxbytes/_backups` | Rotation is global (`[child_logging]`) | Set global `max_size_mb` / `max_backups`; import reports the differences |
| `%(process_num)02d` zero padding | Name templates use `{num}` without width | Fine for <10 procs; pad in your own naming if it matters |
