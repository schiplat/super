---
title: "Programmable Ops"
weight: 2
description: "Build automation scripts and CI/CD pipelines using the HTTP API."
---

While the `super` CLI is powerful, modern DevOps requires automation. Because Super follows an **API-First** design philosophy, every action you perform in the CLI maps directly to a standard RESTful endpoint.

This allows you to build "Self-Healing" scripts, custom dashboards, or CI/CD integrations using standard tools like `curl`, Python `requests`, or Node.js.

## The API-First Philosophy

Unlike Supervisor (which uses XML-RPC) or Systemd (which relies on D-Bus/C bindings), Super exposes a clean JSON API.

*   **Base URL**: `http://localhost:9002/api`
*   **Format**: JSON Request / JSON Response
*   **Authentication**: Bearer token when the `security` plugin is loaded; otherwise open (OSS default).

## Scenario 1: Auto-Remediation Script

Imagine you have a legacy application that leaks memory over time. You want to restart it gracefully when it consumes more than 1GB of RAM — the PM2 `--max-memory-restart` pattern (a soft threshold watchdog), rather than a hard cgroup cap.

> [!NOTE]
> Why not `resource_limits.memory_limit`? That is a hard cgroup v2 cap: the kernel OOM-kills the process when it is exceeded, with no graceful restart. If you want PM2-style "restart when RSS exceeds N", poll `mem_usage` from the API and trigger a graceful restart, as below.

Here is how you can write a simple Python "Watchdog" using the Super API.

```python
import requests
import time

API_URL = "http://localhost:9002/api"
LIMIT_BYTES = 1024 * 1024 * 1024 # 1 GB

def check_and_restart():
    # 1. Get list of all programs
    resp = requests.get(f"{API_URL}/programs")
    programs = resp.json()

    for prog in programs:
        name = prog['name']
        mem_usage = prog.get('mem_usage', 0)

        # 2. Check logic
        if mem_usage > LIMIT_BYTES:
            print(f"[Watchdog] {name} is using {mem_usage} bytes. Restarting...")

            # 3. Trigger graceful restart via API
            requests.post(f"{API_URL}/programs/{prog['id']}/restart")

if __name__ == "__main__":
    while True:
        try:
            check_and_restart()
        except Exception as e:
            print(f"Error: {e}")
        time.sleep(60)
```

## Scenario 2: CI/CD Integration

You can trigger deployments directly from GitHub Actions, GitLab CI, or Jenkins using `curl`.

**Example: Restarting a service after deployment**

```bash
# In your CI pipeline script — resolve UUID first (API paths use id, not name)
ID=$(curl -s http://prod-server:9002/api/v1/programs | jq -r '.[] | select(.name=="my-app") | .id')
curl -X POST "http://prod-server:9002/api/v1/programs/${ID}/restart"
```

**Example: Updating configuration dynamically**

```bash
# Update the binary path and arguments without editing files on the server
ID=$(curl -s http://prod-server:9002/api/v1/programs | jq -r '.[] | select(.name=="my-app") | .id')
curl -X PUT "http://prod-server:9002/api/v1/programs/${ID}" \
     -H "Content-Type: application/json" \
     -d '{
           "command": "/usr/local/bin/new-version",
           "args": ["--feature-flag", "enabled"]
         }'
```

## Scenario 3: Custom Dashboard

Since the API returns standard JSON, you can easily build a custom React/Vue admin panel tailored to your company's needs, embedding Super's status alongside your business metrics.

For full endpoint documentation, refer to the [API Reference](/docs/06-internals/api-reference).
