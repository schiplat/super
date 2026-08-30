---
title: "Process Operations"
weight: 4
description: "Start, stop, restart, and signal processes using the CLI."
---

The `super` CLI tool allows you to interact with the daemon locally or remotely. It uses the HTTP API under the hood.

## Basic Commands

### List Processes

View the status of all managed programs.

```bash
$ super list

ID         Name      Group     Status   PID    CPU   Mem
--------   -------   -------   ------   ----   ---   ---
8f1a...    api-srv   backend   Running  4021   2%    45MB
```

### Start / Stop / Restart

You can target a program by **Name** or **ID**.

```bash
# Start
super start api-srv

# Stop (sends SIGTERM)
super stop api-srv

# Restart (Stop -> Start)
super restart api-srv
```

## Group Operations

If you assign a `group` to your programs (JSON stack, API, or CLI), you can control multiple processes at once using the `@` prefix.

**Config:**
```json
{
  "services": [
    { "name": "api-1", "command": "/usr/bin/api", "group": "backend" },
    { "name": "api-2", "command": "/usr/bin/api", "group": "backend" }
  ]
}
```

**CLI:**
```bash
# Restart all services in the 'backend' group
super restart @backend
```

Also supported: `super start all` and `super stop all`.

## Sending Signals

Sometimes you need to send a specific POSIX signal (e.g., to reload configuration without restarting).

```bash
# Send SIGHUP (often used for config reload)
super signal api-srv --sig hup

# Send SIGUSR1
super signal api-srv --sig usr1
```

Supported signals: `hup`, `int`, `term`, `kill`, `quit`, `usr1`, `usr2`.

## Wait Strategy

By default, CLI commands are async. You can use `--wait` to block until the operation is verified.

```bash
# Wait up to 10 seconds for the process to actually stop
super stop api-srv --wait --timeout 10
```
