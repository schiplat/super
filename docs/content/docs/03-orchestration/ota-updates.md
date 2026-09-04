---
title: "Atomic OTA Updates"
weight: 4
description: "Perform fail-safe, transactional updates for your binaries."
---

Updating software on remote edge devices or production servers is risky. A partial download or a corrupted binary can leave the system in an unrecoverable state ("bricked").

Super solves this with **Transactional OTA (Over-The-Air) Updates**.

## The Transactional Flow

When you trigger an update, Super acts like a database transaction: **All or Nothing**.

1.  **Download**: The new binary is downloaded to a staging file (e.g., `app.new`).
2.  **Verify**: Checksum (SHA256) is verified.
3.  **Backup**: The current running binary is hard-linked to a backup (e.g., `app.bak`).
4.  **WAL**: The "Upgrade In-Progress" state is written to disk (Write-Ahead Log).
5.  **Swap**: The new binary replaces the old one atomically.
6.  **Restart**: The process is restarted.
7.  **Validate**: Super waits for verification to succeed.
    *   With a live `health_check`: commit when the probe reports Healthy; roll back on crash or `ota_verify_timeout`.
    *   Without a probe: wait at least `startsecs` (minimum 1s) of uptime before commit, so a crash-on-start cannot race the synthetic Healthy signal. If `ota_verify_timeout` is shorter than that dwell, Super extends the timeout automatically.
    *   ✅ **Success**: The backup is removed. Transaction committed.
    *   ❌ **Failure**: The process crashes / fails health / times out. **Rollback** restores the backup and restarts the previous version.

## Triggering an Update

Provide a new `artifact` block with a **different `checksum`** than the one already stored. Super compares checksums; if unchanged, config is saved but **no OTA download** runs.

### Via API (recommended for CI/CD)

Use **`PUT /api/v1/programs/{id}`** with the program UUID. Full reference: [API Reference — Update Program](/docs/06-internals/api-reference#update-program).

```bash
# 1. Resolve UUID by name
PROGRAM_ID=$(curl -s http://127.0.0.1:9002/api/v1/programs \
  | jq -r '.[] | select(.name=="my-app") | .id')

# 2. Trigger OTA
curl -X PUT "http://127.0.0.1:9002/api/v1/programs/${PROGRAM_ID}" \
  -H "Content-Type: application/json" \
  -d '{
    "artifact": {
      "source": "https://example.com/builds/v2.0.0/app-linux-amd64",
      "checksum": "a1b2c3d4e5f6789abcdef0123456789abcdef0123456789abcdef0123456789",
      "destination": "/usr/local/bin/my-app",
      "extract": false,
      "restart_policy": "immediate"
    }
  }'
```

With the `security` plugin: add `-H "Authorization: Bearer <token>"`.

### Via Stack (declarative, multi-service)

```bash
curl -X PUT http://127.0.0.1:9002/api/v1/stack \
  -H "Content-Type: application/json" \
  -d '{
    "prune": false,
    "services": [{
      "name": "my-app",
      "command": "/usr/local/bin/my-app",
      "artifact": {
        "source": "https://example.com/builds/v2.0.0/app-linux-amd64",
        "checksum": "a1b2c3d4e5f6789abcdef0123456789abcdef0123456789abcdef0123456789",
        "destination": "/usr/local/bin/my-app",
        "extract": false,
        "restart_policy": "immediate"
      }
    }]
  }'
```

### Via CLI

```bash
super update my-app \
  --artifact-url "https://example.com/builds/v2.0.0/app-linux-amd64" \
  --artifact-sha256 "a1b2c3d4e5f6789abcdef0123456789abcdef0123456789abcdef0123456789"
```

If the program already has an `artifact.destination`, you can omit `--artifact-destination`. Otherwise pass it explicitly:

```bash
super update my-app \
  --artifact-url "https://example.com/builds/v2.0.0/app-linux-amd64" \
  --artifact-sha256 "a1b2c3..." \
  --artifact-destination "/usr/local/bin/my-app"
```

For non-OTA config changes:

```bash
super update my-app --command /usr/local/bin/my-app-v2
super restart my-app    # required to run the new command
```

## Artifact schema

| Field | Required | Description |
| :--- | :--- | :--- |
| `source` | Yes | Download URL. **Remote hosts must use HTTPS** (HTTP allowed on loopback for dev). Cloud metadata endpoints are blocked. |
| `checksum` | Yes | SHA256 hex of the **downloaded bytes** (the archive when `extract` is true, otherwise the bare binary) |
| `destination` | Yes | Absolute path of the **final binary** on disk (not an extract root) |
| `extract` | Yes | `false` for a single binary; `true` to unpack `.tar.gz` / `.tgz` / `.tar` / `.zip` and stage one payload file (member matching the destination basename, else the sole regular file) |
| `restart_policy` | Yes | When the new binary becomes active (see below) |

### `restart_policy`

| Value | Behavior |
| :--- | :--- |
| `immediate` (default) | After swap: restart the process (`SIGTERM`, or spawn if stopped), then verify. Commit when Healthy (or after `startsecs` when no probe); roll back on crash or `ota_verify_timeout`. |
| `manual` | After swap: **commit immediately** and do **not** restart. The running process keeps the old image in memory until the next natural restart. |
| `signal:hup` (dashboard hot-reload default) / `signal` / `signal:<name>` | After swap: deliver a signal without restarting. Bare `signal` is equivalent to `signal:hup`. **Requires an enabled `health_check`** (tcp/http/exec); the API rejects `signal*` without one. Commit waits for the next Healthy probe (or `ota_verify_timeout`). |

## Verification tips

* Prefer a real `health_check` for production OTA — it is the strongest signal that the new version works.
* Keep `[server].ota_verify_timeout` greater than your probe `start_period` / interval (or greater than `startsecs` when you have no probe). Super auto-extends the timeout when no probe would otherwise commit after the deadline.
* `restart_policy=manual` skips verification by design — only use it when you accept swapping the on-disk binary without proving the new process can run.
* `restart_policy=signal*` **requires** an enabled `health_check`. Hot-reload does not exec a new process; without a probe, “still alive” is not proof the reload succeeded. Legacy configs that still have `signal*` without a probe are rejected on the next create/update; **startup** and **`super check`** also warn (or report an error for the snapshot) until you add a probe or change the policy.
* An `exec` health command of `true` / `:` / `/bin/true` / `/usr/bin/true` is accepted structurally but **does not verify** a hot-reload — prefer a real TCP/HTTP/exec probe that exercises the new binary.
* An exit code `0` during the verify window commits the upgrade (one-shot / batch jobs). Non-zero exits and unexpected signals roll back.

## Why this matters

*   **No "Half-Downloaded" States**: The running binary is never touched until the new one is fully downloaded and verified.
*   **Automatic Recovery**: If the new version has a segmentation fault or a configuration error, Super restores the previous working version automatically. No manual intervention required.

See also [Fail-Safe OTA](/docs/04-production-scenarios/delivery/fail-safe-ota).
