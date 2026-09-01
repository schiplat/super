---
title: "Event Hooks"
weight: 3
description: "Run local scripts or POST to webhooks on system events."
aliases:
  - /docs/03-orchestration/event-hooks/
---

Event hooks let you react to [events](/docs/03-orchestration/events/types) by running shell commands on the **same machine** as `superd`, or by POSTing the event JSON to an HTTP(S) **webhook** — no plugin or license required. This is the OSS equivalent of Supervisor's `[eventlistener]`; licensed [Event Notifications](/docs/05-advanced-management/event-notifications) (the `notify` plugin) additionally provide IM-specific templates and channel routing.

> [!NOTE]
> Hooks fire on `SystemEvent`s (lifecycle, health, memory events). **Record-only** events (`cron_started`, `cron_exit`, `cron_spawn_failed`, `queue_full`) are persisted to the [event history](/docs/03-orchestration/events/history) but never trigger hooks.

## Configuration

Define global hooks in `super.toml`. Each hook is either a **local command** (`command`) or a **webhook** (`url`) — if `url` is set, `command` is ignored.

{{< tabs items="Local command,Webhook" >}}

{{< tab >}}
```toml
# super.toml — [event_hooks]
[[event_hooks]]
id = "archive-on-fatal"
command = "/opt/super/archive.sh"
events = ["process_fatal"]
programs = ["*"]          # default: all programs
async = true              # default: true
timeout_secs = 30         # default: 30

[[event_hooks]]
command = "python3 /etc/super/handler.py"
events = ["process_backoff", "process_fatal"]
programs = ["api-server", "worker"]
async = false             # run sequentially (still non-blocking for the manager)
```
{{< /tab >}}

{{< tab >}}
```toml
# super.toml — [event_hooks]
[[event_hooks]]
id = "ops-alert"
url = "https://ops.example.com/hooks/super"
headers = { Authorization = "Bearer s3cret-token" }
events = ["process_fatal", "process_backoff"]
programs = ["*"]
async = true              # default: true (fire-and-forget)
timeout_secs = 10         # request timeout in seconds
```
{{< /tab >}}

{{< /tabs >}}

Webhook requests are `POST` with `Content-Type: application/json` and the same JSON body used for local hooks. Non-2xx responses and timeouts are logged as warnings only — webhooks never block process management.

Reload hooks without restarting programs:

```bash
super reload    # re-reads super.toml, including [[event_hooks]]
```

## JSON payload (stdin / webhook body)

Each matching hook receives one JSON object — on **stdin** for command hooks, as the request **body** for webhooks:

```json
{
  "event": "process_fatal",
  "timestamp": "2026-07-06T16:16:00Z",
  "hostname": "prod-1",
  "version": "1.1.9",
  "program": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "name": "web-server",
    "pid": 1234,
    "uptime_secs": 502
  },
  "payload": {
    "exit_code": 137,
    "signal": 9,
    "msg": "Stopped after 3 retries.",
    "log_tail": null
  }
}
```

`system_startup` / `system_shutdown` events omit the `program` field.

> [!WARNING]
> The `signal` field (when present) is the terminating signal number. A `signal: 9` (`SIGKILL`) with `exit_code: null` is typical of a **cgroup/kernel OOM kill** when [resource limits](/docs/05-advanced-management/resource-isolation) are enforced — don't mistake it for a code-based crash.

## Environment variables

In addition to stdin JSON, hooks receive:

| Variable | When set |
| :--- | :--- |
| `SUPER_EVENT` | Always |
| `SUPER_HOSTNAME` | Always |
| `SUPER_ID` | Program events |
| `SUPER_NAME` | Program events |
| `SUPER_PID` | When PID is known |
| `SUPER_EXIT_CODE` | Fatal / backoff with exit code |
| `SUPER_UPTIME_SECS` | Fatal / backoff / recovered |

## Behavior

* Commands run via `sh -c` (pipes and redirects work).
* Hooks **never block** process management — failures are logged only.
* `async = true` (default): each hook runs in its own task.
* `async = false`: hooks for the same event run one after another in a background task.
* Non-zero exit or timeout → warning log; no impact on managed processes.

## OSS vs licensed notifications

| | Event hooks (OSS) | Notifications (Licensed 💎) |
| :--- | :--- | :--- |
| Config | `super.toml` → `[[event_hooks]]` | `conf/notify.toml` (`notify` plugin) |
| Execution | Local script **or** native webhook POST | HTTP to Slack / 钉钉 / etc. |
| Data | JSON stdin / body + env | Rich envelope + IM templates |

> [!TIP] You can use both
> Licensed notify for on-call alerts with IM templates, event hooks for simple webhooks or local automation (archiving, systemd triggers, …). Start with OSS — `command` for local automation, `url` for a quick webhook; once alerting becomes a daily operational need, that's when `notify` pays off.

> [!TIP] Going production? Upgrade your alerts.
> OSS webhooks deliver raw event JSON — perfect for small deployments and self-hosted alert receivers. For **production-grade alerting**, the licensed `notify` plugin builds on the *same events* and adds:
>
> - Ready-made **Slack / 钉钉 / Feishu** message templates
> - Multiple **channels** with per-channel routing
> - **Deduplication & batching** so a crash storm doesn't flood your phone
> - Delivery retries and delivery metrics
>
> **→ [Event Notifications — the licensed alerting plugin](/docs/05-advanced-management/event-notifications/)**

## Related

* [Events overview](/docs/03-orchestration/events) — the three-layer event system
* [Event Types](/docs/03-orchestration/events/types) — full event catalog
* [Event History](/docs/03-orchestration/events/history) — persisted record of all events
* [Lifecycle Hooks](/docs/03-orchestration/lifecycle-hooks) — per-program start/stop scripts
* [Config Reference — `[[event_hooks]]`](/docs/06-internals/config-reference#event_hooks-oss)
