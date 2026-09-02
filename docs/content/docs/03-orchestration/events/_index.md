---
title: "Events"
weight: 6
description: "Understand the event system: what is emitted, how it is recorded, and how to react."
---

Events are the daemon's structured signal layer. Every meaningful thing that happens — a process crash, a recovery, a cron run, a daemon startup — is emitted as an event, recorded to persistent history, and can trigger reactions.

The event system has three layers:

```
      ① EMIT ──▶ ② RECORD ──▶ ③ REACT
   what happens   event history   real-time actions
   (event types)  (events.db)     (hooks / notifications)
```

| Layer | Page | What it answers |
| :--- | :--- | :--- |
| **① Emit** | [Event Types](./types) | What events exist? What does each payload contain? |
| **② Record** | [Event History](./history) | Where are events stored? How long are they kept? How do I search and audit them? |
| **③ React** | [Event Hooks](./hooks) · [Alerting (`notify`) 💎](/docs/05-advanced-management/event-notifications) | OSS: scripts or webhook via `[[event_hooks]]`. Licensed: optional `notify` plugin for IM + storm suppression (same events, separate config). |

## Quick orientation

- **Which events are recorded?** All of them — crashes, OOM kills, backoff retries, recoveries, health restarts, cron runs, queue drops, and daemon startup/shutdown. See [Event Types](./types).
- **Where is the history stored?** A SQLite database (default `data/events.db`, WAL mode) with configurable retention. See [Event History](./history).
- **How do I look at it?** `super events <name>` from the CLI, or the events API. See [Event History — Querying](./history#querying).
- **How do I react in real time?** One OSS mechanism: [`[[event_hooks]]`](./hooks) in `super.toml` — each entry is either a **local script** (`command`) or a **webhook POST** (`url`), not both. For production IM alerting with templates and [storm suppression](/docs/05-advanced-management/event-notifications#storm-suppression), add the licensed **`notify`** plugin (`conf/notify.toml`); it uses the same events and can run **alongside** hooks. See [Event Hooks](./hooks) vs [Event Notifications](/docs/05-advanced-management/event-notifications).
- **Per-program start/stop hooks?** Those are a different mechanism (lifecycle hooks) — see [Lifecycle Hooks](/docs/03-orchestration/lifecycle-hooks).

> [!NOTE] OSS vs licensed
> **Event hooks** (`[[event_hooks]]`) are OSS — scripts or basic webhook POST. The licensed **`notify`** plugin is a separate **alerting** layer on the same events (IM templates, channels, storm suppression). Configure hooks in `super.toml` and alerting in `conf/notify.toml`; both can be active. See the [feature matrix](/docs/07-editions/feature-matrix/#event-hooks-vs-alerting).

## Related

* [Lifecycle Hooks](/docs/03-orchestration/lifecycle-hooks) — per-program start/stop scripts (a different, local mechanism)
* [Scheduled Tasks](/docs/02-essentials/scheduled-tasks) — cron events and auditing runs
* [Config Reference — `[storage]`](/docs/06-internals/config-reference#storage) — `events_file` / `events_keep_days`
