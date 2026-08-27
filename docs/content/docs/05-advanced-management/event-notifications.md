---
title: "Event Notifications"
weight: 5
description: "Real-time alerts via Webhooks, Slack, DingTalk, Lark, and Teams."
---

### Webhook System 💎

Super acts as an intelligent observer. Instead of just logging errors, it can actively push events to external systems like Slack, Microsoft Teams, or your company's internal IM tools.

> **Licensed feature:** Event notifications require the `notify` plugin and a valid subscription license. OSS builds without the plugin will ignore `conf/notify.toml`.

## Configuration

Notifications are configured in a separate file **`conf/notify.toml`** (sibling to `super.toml`). This separation allows you to hot-reload alerting rules without restarting your processes.

> **Alerts live in `conf/notify.toml`**, not in `super.toml`. There is no `[webhook]` section in the daemon config schema. Licensed alerting uses `notify.toml` with the `notify` plugin — see [Config Reference — `[[event_hooks]]`](/docs/06-internals/config-reference#event_hooks-oss) for OSS script hooks.

### File location

```
$SUPER_ROOT/
├── conf/
│   ├── super.toml       # Main daemon config
│   └── notify.toml      # Webhooks + inhibition rules (this file)
├── run/                 # Optional pidfile when using superd --daemon
├── plugins/
│   └── notify.so        # Required plugin
```

If `notify.toml` does not exist, the plugin starts with zero channels and no notifications are sent.

---

## Configuration Reference

### Channel schema

Each channel is defined as a `[[channels]]` block in TOML:

```toml
[[channels]]
id = "unique-channel-id"          # Required: unique identifier
name = "Human-readable name"      # Required: display name
type = "webhook"                  # Required: channel type (see presets below)
triggers = ["process_fatal", "*"] # Required: event types to listen for
include_log_tail = true           # Optional: attach recent logs to notifications (default: false)

[channels.config]                 # Required: webhook-specific settings
url = "https://example.com/webhook"
secret = "hmac-signing-secret"    # Optional: HMAC-SHA256 signature key
headers = { Authorization = "Bearer token" }  # Optional: custom HTTP headers
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | Yes | Unique identifier for this channel. Used in API responses and logs. |
| `name` | string | Yes | Human-readable name displayed in the dashboard and logs. |
| `type` | string | Yes | Channel type: `webhook`, `slack`, `dingtalk`, `lark`, `feishu`, `wecom`, `wechat`, `teams`, `msteams`. See [Built-in Presets](#built-in-presets). |
| `triggers` | list | Yes | Event types to subscribe to. Use `["*"]` for all events, or specific event names. See [Supported Events](#supported-events). |
| `include_log_tail` | bool | No | When `true`, attaches the last ~2000 characters of stderr to `process_fatal` notifications. Default: `false`. |
| `template` | string | No | Custom Handlebars template (overrides preset). See [Custom Templates](#custom-templates). |

#### `[channels.config]`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `url` | string | Yes | Webhook endpoint URL. |
| `secret` | string | No | Signing secret for payload verification. Signing method depends on platform — see [Payload Signing](#payload-signing). |
| `headers` | table | No | Custom HTTP headers sent with every notification. Example: `{ Authorization = "Bearer token", X-Custom = "value" }` |

---

## Built-in Presets

You do not need to write complex JSON templates for popular platforms. Super includes built-in rich-text templates. Just set the `type` field to one of the supported presets:

| Type | Platform |
|------|----------|
| `slack` | Slack Incoming Webhooks |
| `dingtalk` | DingTalk Custom Robot |
| `lark` / `feishu` | Lark / Feishu Bot |
| `wecom` / `wechat` / `wechat_work` | WeCom (WeChat Work) |
| `teams` / `msteams` | Microsoft Teams (MessageCard) |
| `webhook` | Generic JSON (raw Super envelope) |

### Slack

```toml
[[channels]]
id = "slack-ops"
name = "Ops Team Slack"
type = "slack"
triggers = ["process_fatal", "process_backoff"]
include_log_tail = true

[channels.config]
url = "https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX"
```

Renders a Slack Block Kit message with a header, markdown body, and hostname context footer.

### DingTalk

```toml
[[channels]]
id = "dingtalk-alert"
name = "DingTalk Ops Group"
type = "dingtalk"
triggers = ["*"]
include_log_tail = true

[channels.config]
url = "https://oapi.dingtalk.com/robot/send?access_token=YOUR_TOKEN"
secret = "SEC..."
```

Renders a markdown message with title `Superd Alert: {program_name}`. When `secret` is set, Super computes the DingTalk HMAC signature automatically.

### Lark / Feishu

```toml
[[channels]]
id = "lark-alert"
name = "Backend On-Call"
type = "lark"
triggers = ["process_fatal", "system_startup", "system_shutdown"]

[channels.config]
url = "https://open.feishu.cn/open-apis/bot/v2/hook/YOUR_HOOK_ID"
```

Renders an interactive card with red header, markdown body, and hostname/version footer.

### WeCom / WeChat

```toml
[[channels]]
id = "wecom-ops"
name = "WeCom Ops Channel"
type = "wecom"
triggers = ["*"]

[channels.config]
url = "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=YOUR_KEY"
```

Renders a markdown message via the WeCom `msgtype: markdown` format.

### Microsoft Teams

```toml
[[channels]]
id = "teams-alerts"
name = "Teams Ops Channel"
type = "teams"
triggers = ["process_fatal"]
include_log_tail = true

[channels.config]
url = "https://outlook.office.com/webhook/YOUR_WEBHOOK_URL"
```

Renders a MessageCard with summary, hostname subtitle, and markdown body.

### Generic Webhook with HMAC

```toml
[[channels]]
id = "internal-monitoring"
name = "Internal Monitoring Hub"
type = "webhook"
triggers = ["*"]

[channels.config]
url = "https://api.my-company.com/v1/alerts"
secret = "my-super-secret-key-888"
headers = { Authorization = "Bearer sk-123456", X-Custom-Auth = "super-admin" }
```

Sends the raw Super JSON envelope (see [Default Envelope](#default-envelope-webhook-type) below).

---

## Supported Events

Use these strings in the `triggers` field. Full payload reference: [System Events](/docs/03-orchestration/system-events).

| Event | Description |
|-------|-------------|
| `process_started` | A process spawned successfully. |
| `process_fatal` | A process crashed and exhausted retries. **Includes stderr tail when `include_log_tail = true`.** |
| `process_backoff` | A process crashed but is restarting (flapping). |
| `process_recovered` | A previously-crashing process has become healthy. |
| `system_startup` | The daemon started. |
| `system_shutdown` | The daemon is shutting down. |
| `*` | All of the above. |

---

## Default Envelope (webhook type)

When `type = "webhook"`, Super sends this structured JSON payload:

```json
{
  "id": "uuid-of-notification",
  "timestamp": "2026-07-22T10:00:00Z",
  "event": "process_fatal",
  "system": {
    "hostname": "prod-server-1",
    "version": "1.3.0"
  },
  "summary": "[Fatal] worker on prod-server-1: Stopped after 3 retries.",
  "markdown": "### Process Fatal Alert\n- Service: worker\n- Host: prod-server-1\n...",
  "data": {
    "program_name": "worker",
    "pid": 12345,
    "exit_code": 1,
    "msg": "Stopped after 3 retries.",
    "log_tail": "Error: Connection refused..."
  },
  "log_tail": "Error: Connection refused..."
}
```

| Field | Description |
|-------|-------------|
| `id` | Unique notification ID (UUID v4) |
| `timestamp` | ISO 8601 timestamp (UTC) |
| `event` | Event type string |
| `system.hostname` | Host where `superd` is running |
| `system.version` | Plugin version |
| `summary` | One-line plain-text summary |
| `markdown` | Pre-rendered markdown with event details |
| `data` | Raw event payload |
| `log_tail` | Recent stderr (only present for `process_fatal` when `include_log_tail = true`) |

---

## Payload Signing

When `secret` is configured, Super signs outgoing requests using platform-specific methods:

### Generic Webhook (`type = "webhook"`)

Adds `X-Super-Signature: sha256=<hex-digest>` header. Signature is `HMAC-SHA256(secret, request_body)`.

```python
import hmac, hashlib

def verify(secret: str, body: bytes, header: str) -> bool:
    expected = hmac.new(secret.encode(), body, hashlib.sha256).hexdigest()
    received = header.replace("sha256=", "")
    return hmac.compare_digest(expected, received)
```

### DingTalk (`type = "dingtalk"`)

Appends `timestamp` and `sign` query parameters to the URL:

```
https://oapi.dingtalk.com/robot/send?access_token=XXX&timestamp=1680000000000&sign=<base64-encoded-signature>
```

Signature is `HMAC-SHA256(secret, timestamp + "\n" + secret)`, Base64-encoded.

```python
import hmac, hashlib, base64, time

def verify_dingtalk(secret: str, timestamp: str, sign: str) -> bool:
    string_to_sign = f"{timestamp}\n{secret}"
    expected = hmac.new(
        secret.encode(),
        string_to_sign.encode(),
        hashlib.sha256
    ).digest()
    expected_b64 = base64.b64encode(expected).decode()
    return hmac.compare_digest(expected_b64, sign)
```

### Lark / Feishu (`type = "lark"`, `"feishu"`)

Injects `timestamp` and `sign` fields into the JSON body:

```json
{
  "msg_type": "interactive",
  "card": { ... },
  "timestamp": "1680000000",
  "sign": "<base64-encoded-signature>"
}
```

Signature is `HMAC-SHA256(timestamp + "\n" + secret, secret)`, then Base64-encode the hex digest.

```python
import hmac, hashlib, base64

def verify_lark(secret: str, timestamp: str, sign: str) -> bool:
    string_to_sign = f"{timestamp}\n{secret}"
    hex_digest = hmac.new(
        string_to_sign.encode(),
        secret.encode(),
        hashlib.sha256
    ).hexdigest()
    expected = base64.b64encode(hex_digest.encode()).decode()
    return hmac.compare_digest(expected, sign)
```

### Platforms without signing

**Slack**, **Teams**, and **WeCom** do not support webhook signing — the URL itself is the credential. Do not set `secret` for these platforms.

---

## Custom Templates

For advanced use cases, override the default payload with a Handlebars template:

```toml
[[channels]]
id = "custom-integration"
name = "Custom Integration"
type = "webhook"
triggers = ["process_fatal"]

template = """
{
  "service": "{{ data.program_name }}",
  "severity": "critical",
  "message": {{ json_quote rendered_summary }},
  "host": "{{ system_hostname }}"
}
"""

[channels.config]
url = "https://api.example.com/alerts"
```

### Template variables

| Variable | Type | Description |
|----------|------|-------------|
| `system_hostname` | string | Hostname where `superd` is running |
| `system_version` | string | Plugin version |
| `rendered_summary` | string | One-line plain-text summary |
| `rendered_markdown` | string | Pre-rendered markdown with event details and log tail |
| `data` | object | Raw event data (fields vary by event type) |

**Event-specific `data` fields:**

| Event | Fields |
|-------|--------|
| `process_fatal` | `program_id`, `program_name`, `pid`, `exit_code`, `msg`, `log_tail` |
| `process_backoff` | `program_id`, `program_name`, `pid`, `exit_code`, `retry_count` |
| `process_recovered` | `program_id`, `program_name`, `pid`, `uptime_sec` |
| `process_started` | `program_id`, `program_name`, `pid` |
| `system_startup` | `hostname` |
| `system_shutdown` | *(none)* |

### `json_quote` helper

When embedding rendered text inside JSON strings, use `{{ json_quote ... }}` to escape special characters (newlines, quotes, backslashes) so the output remains valid JSON:

```handlebars
{
  "text": {{ json_quote rendered_markdown }}
}
```

Without `json_quote`, raw markdown would break JSON syntax.

### Template priority

When multiple template sources exist, Super resolves in this order:

1. `template` field (explicit custom template)
2. `type` preset (e.g., `type = "slack"`)
3. Default Super envelope

---

## Hot Reload

Configuration changes take effect without restarting the daemon:

```bash
super reload
```

The plugin re-reads `conf/notify.toml` and applies the new channel list immediately.

---

## Management API

| Method | Path | Role | Description |
|--------|------|------|-------------|
| `GET` | `/api/v1/system/notify` | any (secrets redacted for Viewer) | Read current config |
| `PUT` | `/api/v1/system/notify` | Admin, Operator | Replace entire config |
| `PUT` | `/api/v1/system/notify/channel` | Admin, Operator | Upsert a single channel |
| `GET` | `/api/v1/system/notify/stats` | any authenticated | Delivery metrics / snapshots |
| `POST` | `/api/v1/system/notify/test` | Admin, Operator | Send test notification |

### Read configuration

```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:9002/api/v1/system/notify
```

Sensitive fields (`secret`, `token`, `key`, `authorization`, etc.) are automatically masked as `********` for Viewer.

### Update configuration

```bash
curl -X PUT http://127.0.0.1:9002/api/v1/system/notify \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "channels": [
      {
        "id": "slack-ops",
        "name": "Slack",
        "type": "slack",
        "triggers": ["*"],
        "config": { "url": "https://hooks.slack.com/..." }
      }
    ]
  }'
```

### Test a channel

```bash
curl -X POST http://127.0.0.1:9002/api/v1/system/notify/test \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "test",
    "name": "Test",
    "type": "webhook",
    "triggers": ["*"],
    "config": { "url": "https://webhook.site/your-id" }
  }'
```

Sends a `system_startup` event with `hostname = "TEST-MODE"`.

---

## Metrics

Prometheus metrics are exposed at `/metrics`:

```
# HELP super_notify_sent_total Total number of notifications sent.
# TYPE super_notify_sent_total counter
super_notify_sent_total{status="success"} 142
super_notify_sent_total{status="failed"} 3
```

---

## Complete Example

A production-ready `conf/notify.toml` with multiple channels:

```toml
# Alert on crashes to Slack
[[channels]]
id = "slack-critical"
name = "Slack Critical Alerts"
type = "slack"
triggers = ["process_fatal", "system_shutdown"]
include_log_tail = true

[channels.config]
url = "https://hooks.slack.com/services/T00/B00/CRITICAL"


# All events to DingTalk
[[channels]]
id = "dingtalk-all"
name = "DingTalk All Alerts"
type = "dingtalk"
triggers = ["*"]

[channels.config]
url = "https://oapi.dingtalk.com/robot/send?access_token=TOKEN"
secret = "SEC..."


# Internal monitoring with HMAC + custom headers
[[channels]]
id = "internal-hub"
name = "Internal Monitoring Hub"
type = "webhook"
triggers = ["process_fatal", "process_backoff", "process_recovered"]

[channels.config]
url = "https://monitoring.internal/api/alerts"
secret = "hmac-secret-key"
headers = { "X-Source" = "superd", Authorization = "Bearer internal-token" }
```

---

## Storm Suppression 💎

When a process crashes and restarts rapidly, each `process_fatal` and `process_backoff` event triggers a notification. Without safeguards, a flapping service can flood your webhooks — and if the remote platform rate-limits you (HTTP `429`), important alerts may be lost.

Super's **storm suppression** system prevents notification floods with two complementary mechanisms:

| Mechanism | Scope | What it does |
|-----------|-------|-------------|
| **Delivery Strategy** | Per webhook (`[[channels]]`) | Controls how frequently that destination receives notifications |
| **Inhibition** | Global | Suppresses related events after a source event fires (same program) |

In the licensed **Web UI** ([Notification Settings](/docs/05-advanced-management/web-ui)), these map to two tabs:

| Dashboard tab | Configures |
|---------------|------------|
| **Webhooks** | Destinations, triggers, headers, and per-webhook **Delivery Strategy** |
| **Inhibition rules** | Global rules: **When** → **Mute targets** → **For** (duration) |

### Delivery Strategy

Each webhook channel defines a `strategy` that governs notification pacing **for that destination only**.

```mermaid
flowchart TB
  EV["Incoming events<br/>(matched triggers)"] --> MODE{{"strategy.mode"}}

  MODE -->|"immediate"| IMM["Send every event"]
  MODE -->|"cooldown"| CD["Send first of each event type<br/>Skip same-type repeats for cooldown_secs"]
  MODE -->|"batch"| BAT["Buffer for window_secs<br/>Flush one summary (cap max_events)"]

  IMM --> WH["Webhook destination"]
  CD --> WH
  BAT --> WH
```

```toml
[[channels]]
id = "slack-ops"
name = "Ops Slack"
type = "slack"
triggers = ["*"]

[channels.strategy]
mode = "cooldown"      # immediate | cooldown | batch
cooldown_secs = 60     # only for mode = "cooldown"
window_secs = 30        # only for mode = "batch"
max_events = 10         # only for mode = "batch"
```

#### Strategy Modes

| Mode | Behavior | Use case | Recommended defaults |
|------|----------|----------|----------------------|
| `immediate` | Every event is sent individually, without delay. | Low-noise destinations, or when every event matters. | — |
| `cooldown` | After sending a notification, skip subsequent events of the **same type** for `cooldown_secs`. | Preventing bursty repeat alerts from a flapping service. | `cooldown_secs = 60` |
| `batch` | Collect events into a window (`window_secs`), then send one aggregated summary. Capped at `max_events` per batch. | High-volume environments where per-event noise is unwanted. | `window_secs = 30`, `max_events = 10` |

The dashboard pre-fills these recommended values when you switch modes so fields are never left empty.

#### Cooldown in Detail

When `mode = "cooldown"`:

- The **first** event of a given type (`process_fatal`, `process_backoff`, etc.) fires immediately.
- Subsequent events of the **same type** are suppressed until `cooldown_secs` seconds have elapsed.
- Different event types do **not** share a cooldown — a `process_fatal` does not block a `system_startup`.

```toml
[channels.strategy]
mode = "cooldown"
cooldown_secs = 60      # At most 1 notification per event type per minute
```

> **Backward compatibility:** If no `[channels.strategy]` block is present, the channel defaults to `immediate`. The legacy `cooldown_secs` field (top-level on `[[channels]]`) remains accepted as a shorthand for `strategy = { mode = "cooldown", cooldown_secs = ... }`.

#### Batch in Detail

When `mode = "batch"`:

- Events are accumulated in an in-memory buffer during the `window_secs` window.
- A global ticker (1-second resolution) checks all batching channels and flushes windows that have elapsed.
- The aggregated notification includes a count of each event type and individual summaries up to `max_events`.

```toml
[channels.strategy]
mode = "batch"
window_secs = 30        # Collect events for 30 seconds
max_events = 10          # Cap individual event summaries at 10
```

**Batch payload example:**

```json
{
  "id": "uuid-of-batch-notification",
  "timestamp": "2026-07-24T10:00:30Z",
  "event": "batch_summary",
  "system": {
    "hostname": "prod-server-1",
    "version": "1.3.0"
  },
  "summary": "[Batch] 5 events in 30s: 3 process_fatal, 2 process_backoff",
  "data": {
    "window_secs": 30,
    "total_events": 5,
    "breakdown": {
      "process_fatal": 3,
      "process_backoff": 2
    },
    "events": [
      { "event": "process_fatal", "program_name": "worker-1", "msg": "Exit code 1" },
      { "event": "process_fatal", "program_name": "worker-2", "msg": "Exit code 137" },
      "... (truncated to max_events)"
    ]
  }
}
```

### Inhibition

While **Delivery Strategy** controls per-webhook pacing, **Inhibition** prevents redundant notifications **across all webhooks** based on event semantics (similar to Alertmanager `inhibit_rules`).

**Classic example:** If `process_fatal` fires for `worker`, there is little value in also sending `process_backoff` for `worker` — the fatal alert already covers the failure. Inhibition expresses that relationship.

#### Mental model (dashboard)

Configure each rule in order:

1. **When** — events that start (or refresh) the mute window → config field `sources`
2. **Mute targets** — events to silence while the window is active → config field `targets`
3. **For** — how long the window lasts → config field `ttl_secs`

Matching is limited to the **same program** (`match_on = ["program_name"]` in the dashboard).

```mermaid
flowchart LR
  subgraph scope["Same program_name · all webhooks"]
    direction LR
    W["1. When<br/>sources<br/>e.g. process_fatal"] --> M["2. Mute targets<br/>targets<br/>e.g. process_backoff"]
    M --> F["3. For<br/>ttl_secs<br/>e.g. 300s"]
  end

  style scope stroke-dasharray: 5 5
```

| Dashboard | TOML field | Role |
|-----------|------------|------|
| When | `sources` | Condition / trigger |
| Mute targets | `targets` | Action targets (what not to notify) |
| For | `ttl_secs` | Duration of the mute window |

#### Schema

```toml
[[inhibition_rules]]
id = "fatal-suppresses-backoff"   # Unique identifier
sources = ["process_fatal"]       # When: events that trigger inhibition
targets = ["process_backoff"]     # Mute targets: suppressed while a source is active
match_on = ["program_name"]       # Fields that must match between source and target
ttl_secs = 300                    # For: how long inhibition lasts (seconds)
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | Yes | Unique identifier for this rule. |
| `sources` | list | Yes | **When** — event types that activate this inhibition. |
| `targets` | list | Yes | **Mute targets** — event types suppressed while a source is active. |
| `match_on` | list | Yes | Fields that must have equal values for inhibition to apply. Typically `["program_name"]`. |
| `ttl_secs` | number | Yes | **For** — duration (seconds) that inhibition persists after the source event fires. |

#### How it Works

1. A **When** event fires (e.g., `process_fatal` for program `worker`).
2. Super creates an **active inhibitor** scoped to `match_on` values — here `program_name = "worker"`.
3. For `ttl_secs` (e.g., 300 seconds), any **Mute target** event (`process_backoff`) for the same `worker` is suppressed **globally** across all webhooks.
4. After the TTL expires, the inhibitor is cleared. If a **When** event fires again, the inhibitor is refreshed.

**Overlap is allowed:** an event type may appear in both `sources` and `targets` (for example When = Fatal or Restarting, Mute targets = Restarting). Evaluation order is: check suppression → send if allowed → then record the event as a source. So the first matching event still notifies; further repeats of the mute target are silenced for the duration.

> **Tip:** Prefer process events that share `program_name`. `system_startup` / `system_shutdown` do not carry a program name, so inhibiting them with `match_on = ["program_name"]` usually has no effect.

#### Example: Fatal → Restarting

```toml
[[inhibition_rules]]
id = "fatal-scenarios"
sources = ["process_fatal"]              # When
targets = ["process_backoff"]            # Mute targets
match_on = ["program_name"]
ttl_secs = 300                           # For: 5 minutes
```

**Scenario:**

| Time | Event | Action |
|------|-------|--------|
| T+0s | `process_backoff` for `worker` | Sent to all subscribed webhooks |
| T+2s | `process_fatal` for `worker` | Sent immediately; inhibitor created for `worker` |
| T+5s | `process_backoff` for `worker` | **Suppressed** (mute target under active inhibition) |
| T+8s | `process_backoff` for `api-server` | Sent (different program) |
| T+300s | `process_backoff` for `worker` | Sent again (TTL expired) |

#### Audit Logging

Suppressed events are recorded in the notify audit log (`notify.log`) with a `suppressed_by` field pointing to the inhibiting rule ID. This ensures you can audit what was suppressed and why.

### Choosing the Right Combination

| Scenario | Recommended Configuration |
|----------|--------------------------|
| Single service, low noise | `immediate` — no configuration needed |
| Service flaps occasionally | Delivery Strategy `cooldown` with `cooldown_secs = 60` |
| Many services, high event volume | Delivery Strategy `batch` with `window_secs = 30`, `max_events = 10` |
| Crash + restart noise for one program | Inhibition: When `process_fatal` → Mute targets `process_backoff` |
| Cascading / high volume + semantics | `batch` + Inhibition rules together |

### Configuration in notify.toml

All storm suppression settings live in the same `conf/notify.toml`:

```toml
# --- Webhooks with delivery strategies ---

[[channels]]
id = "slack-critical"
name = "Slack Critical"
type = "slack"
triggers = ["process_fatal", "system_shutdown"]
include_log_tail = true

[channels.config]
url = "https://hooks.slack.com/services/T00/B00/CRITICAL"

[channels.strategy]
mode = "cooldown"
cooldown_secs = 60


[[channels]]
id = "dingtalk-batch"
name = "DingTalk Summary"
type = "dingtalk"
triggers = ["*"]

[channels.config]
url = "https://oapi.dingtalk.com/robot/send?access_token=TOKEN"

[channels.strategy]
mode = "batch"
window_secs = 30
max_events = 10


# --- Global inhibition (When → Mute targets → For) ---

[[inhibition_rules]]
id = "fatal-blocks-backoff"
sources = ["process_fatal"]
targets = ["process_backoff"]
match_on = ["program_name"]
ttl_secs = 300
```

### Hot Reload

Like webhook configuration, inhibition rules take effect on `super reload` — no daemon restart needed.

```bash
super reload
```

---

## Troubleshooting

| Symptom | Cause | Solution |
|---------|-------|----------|
| No notifications sent | No channels configured | Add at least one `[[channels]]` block to `conf/notify.toml` |
| `400` on test | Invalid channel config | Check `url` is present; verify TOML syntax |
| Signature mismatch on receiver | `secret` mismatch | Ensure the same `secret` on both sides |
| Config not updating after reload | Syntax error in `notify.toml` | Check daemon logs for parse errors; fix TOML and reload |
| Template rendering fails | Invalid Handlebars syntax | Check template for unclosed `{{ }}` or undefined variables |
| `log_tail` missing in notification | `include_log_tail = false` or not a `process_fatal` event | Set `include_log_tail = true`; log tail only appears on `process_fatal` |
| Sensitive fields visible in API | Using Admin token | Non-Admin users see redacted fields; this is expected |
