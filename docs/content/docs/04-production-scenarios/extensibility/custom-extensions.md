---
title: "Custom Extensions"
weight: 1
description: "Hook custom Rust logic into the process lifecycle: env injection, pre-flight start gates, auditing, metrics."
---

Super's process lifecycle is extensible at one point: the **`Extension` trait** — a middleware-style interface whose hooks run around every managed process. There are two ways to hook into it:

* **In-process `Extension` (OSS)** — Rust code compiled into a binary that embeds `super-core`. This is the always-available path, and the one this page is about.
* **Runtime plugins (licensed)** — signed native libraries that stock `superd` loads from `$SUPER_ROOT/plugins/` after license verification, bridged onto the same trait internally.

> [!NOTE]
> Stock `superd` never loads arbitrary compiled-in extensions — it only bridges licensed plugins (an empty `NoOpExtension` when none are licensed). To run your own `Extension`, you embed `super-core` in your own binary — the same pattern `superd` itself uses. The full trait reference, embedding guide, and a buildable example live in [Writing Extensions](/docs/09-development/writing-extensions).

## Hook reference

Every hook has a default implementation; implement only what you need. Behavior below matches the host's actual call sites:

| Hook | When it fires | Semantics |
| :--- | :--- | :--- |
| `before_start` | Before the process is spawned | Returned vars are merged into the child environment. Returning `Err` **blocks the start** — the program is marked `Fatal` with your error message, visible in `super list` and the event ledger. |
| `after_start` | Immediately after spawn, once the PID exists (runs on a blocking thread) | `Err` is **fail-secure**: the just-started child is killed immediately and the program marked `Fatal`. Return `Ok` unless setup genuinely failed. |
| `after_stop` | Every process exit **and** program removal (fire-and-forget, blocking thread) | Cleanup of per-program resources (e.g. the licensed `isolation` plugin removes the cgroup here). |
| `on_event` | On every system event, synchronously on the event path | No `Result` — cannot fail the operation. Keep the handler fast: record/enqueue and let a background thread do heavy work. |
| `on_update` | A config update that changes a program's `resource_limits` | `Err` fails the update. `pid` is `Some` while the program is running, so limits can be re-applied live. |
| `on_reload` | Host configuration reload | `Err` is logged, not fatal. |
| `on_shutdown` | Graceful daemon shutdown, before the final `system_shutdown` event | `Err` is logged, not fatal. |
| `collect_metrics` | Each `/metrics` scrape | Returned text (Prometheus format) is appended under `# --- Extension Metrics ---`. |
| `supports_resource_limits` | Startup advertisement | Return `true` only if your extension actually enforces `resource_limits`; otherwise those values are stored but not enforced. |

> [!NOTE]
> The trait also declares `before_stop`. It is **reserved and currently not invoked** by the host — do not rely on it for drain/deregister logic. For stop-adjacent reactions, observe events in `on_event`, or use the per-program `pre_stop` [lifecycle hook](/docs/03-orchestration/lifecycle-hooks), which runs reliably before the stop signal.

## Use cases

### 1. Configuration injection (`before_start`)

**Scenario**: Your app needs database credentials, but they are stored in a central config server, not in static files.

**Extension logic**:

1.  Intercept the start request (you get the program name and config).
2.  Fetch the secrets from the central store (Nacos, Consul, Vault, …).
3.  Return them as a `HashMap` — Super merges the variables (e.g. `DB_PASSWORD=...`) into the process environment.

**Result**: The application starts with fresh credentials, with no wrapper scripts inside the container.

### 2. Specialized auditing (`on_event`)

**Scenario**: You work in a regulated industry (Finance/Healthcare). A generic webhook isn't enough; you need audit records written to a local encrypted queue or hardware security module (HSM) whenever a process crashes.

**Extension logic**:

1.  In `on_event`, filter for fatal process events.
2.  Serialize the event details and append to your audit sink.

Because `on_event` runs synchronously on the event path, do the cheap part inline (append to a local queue) and flush/encrypt/upload from your own background thread.

### 3. Pre-flight start gate (`before_start`)

**Scenario**: Your service requires infrastructure that must be ready before the process launches — a migration tool must run only against a reachable primary, or a worker may start only when its license file is valid. A crash loop "discover and retry" wastes resources; you want the start refused outright.

**Extension logic**:

1.  Check the program name (or its config) to decide whether a gate applies.
2.  Verify the precondition — probe the database, check the license file, validate a certificate expiry.
3.  If the check fails, return an `Err` — Super aborts the start and marks the program `Fatal` with your error message, so the failure is visible in `super list` and the event ledger instead of surfacing as a child crash.

## Building your own

Link `super-core` and pass your extension to `bootstrap()` — the same call `superd` makes with its plugin stack:

```toml
[dependencies]
super-core = { git = "https://github.com/schiplat/super", rev = "main" }
common     = { git = "https://github.com/schiplat/super", rev = "main" }
```

```rust
use super_core::extension::Extension;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let core = super_core::bootstrap(Box::new(MyExtension)).await?;

    // Ctrl-C → graceful shutdown: state flush + on_shutdown hooks run.
    tokio::signal::ctrl_c().await?;
    core.manager_handle.shutdown().await?;
    Ok(())
}
```

To compose several extensions, chain them with `ExtensionStack` (hooks run in registration order; `before_start` env maps merge left-to-right, later layers win) — `superd` uses the same stack internally to bridge licensed plugins.

A complete walkthrough — full hook signatures, a working example, `SUPER_ROOT` layout, and the licensed-runtime boundary — is in [Writing Extensions](/docs/09-development/writing-extensions).
