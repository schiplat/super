---
title: "Fail-Safe OTA"
weight: 1
description: "Updating remote edge devices without fear of bricking."
---

Updating software on 1,000 remote devices is terrifying. A network glitch during download or a buggy binary can leave a device in a "zombie" state, requiring a physical truck roll to fix.

Super introduces a **Transactional OTA (Over-The-Air)** mechanism designed specifically for these high-stakes environments.

## The Problem: "The Valley of Death"

In traditional update scripts (e.g., `wget && restart`), there is a critical window of vulnerability:
1.  **Partial Download**: `wget` fails at 99%, but the script tries to run the corrupted binary.
2.  **Bad Config**: The binary is fine, but it crashes immediately due to a missing config.
3.  **No Backup**: The old binary was overwritten, so you cannot go back.

## The Super Solution: Atomic Transactions

Super treats updates like a database transaction. It follows a strict **WAL (Write-Ahead Log)** protocol.

### The Update Flow

When you submit an update request:

1.  **Staging**: Super downloads the new binary to a temporary path (e.g., `app.new`). The current running service is untouched.
2.  **Verification**: It calculates the SHA256 checksum. If it doesn't match, the update aborts immediately. Zero downtime.
3.  **Backup**: Super creates a hard link of the *current* binary to `app.bak`.
4.  **WAL**: The "Upgrade In-Progress" state is written to disk (Write-Ahead Log).
5.  **Swap**: It uses `rename(2)` to atomically replace the binary.
6.  **Restart**: The process is restarted according to the **Restart Policy**.
7.  **Validate**: Super waits for the `health_check` to pass.
    *   ✅ **Success**: The backup is removed. Transaction committed.
    *   ❌ **Failure**: The process crashes or fails health checks. **Rollback** is triggered. The backup is restored, and the old version is restarted.

## Restart Policies

The `restart_policy` field controls *when* the new binary becomes active.

| Policy | Description | Use Case |
| :--- | :--- | :--- |
| **`immediate`** | **Default**. Sends `SIGTERM` to restart the process immediately after the binary swap. | Standard web services, critical patches. |
| **`manual`** | Swaps the binary on disk but **does not** restart. The new version runs on the next natural restart (e.g., reboot). | Non-critical background agents. |
| **`signal`** | Swaps the binary and sends a custom signal (e.g., `SIGHUP`) instead of restarting. | Applications that support internal **Hot Reloading** (like Nginx or some Go apps). |

> [!WARNING]
> As of v1.1.7, `immediate` is the primary supported policy. `manual` and `signal` are reserved for future implementation.

## Triggering an Update

You don't need complex orchestration tools. You just need to tell Super where the new artifact is.

### 1. The Update Payload

Define the artifact details in a JSON object.

```json
{
  "artifact": {
    "source": "https://cdn.example.com/builds/v1.2.0/edge-agent",
    "checksum": "a1b2c3d4e5f6...",
    "destination": "/usr/local/bin/edge-agent",
    "extract": false,
    "restart_policy": "immediate"
  }
}
```

### 2. Trigger via API

Resolve the program **UUID** first (`GET /api/v1/programs`), then `PUT` the artifact. See [API Reference — Update Program](/docs/06-internals/api-reference#update-program).

```bash
PROGRAM_ID=$(curl -s http://device-ip:9002/api/v1/programs \
  | jq -r '.[] | select(.name=="edge-agent") | .id')

curl -X PUT "http://device-ip:9002/api/v1/programs/${PROGRAM_ID}" \
  -H "Content-Type: application/json" \
  -d '{
    "artifact": {
      "source": "https://cdn.example.com/builds/v1.2.0/edge-agent",
      "checksum": "a1b2c3d4e5f6...",
      "destination": "/usr/local/bin/edge-agent",
      "extract": false,
      "restart_policy": "immediate"
    }
  }'
```

## Why it is "Fail-Safe"

Even if the device loses power exactly in the middle of an update:
*   **Before Swap**: The old binary is still there. Super starts the old version on reboot.
*   **During Validation**: Super sees the WAL record (restore path) on reboot. It knows an update was pending and wasn't committed. It triggers a rollback to ensure safety.

This guarantees that your fleet **always** comes back online.
