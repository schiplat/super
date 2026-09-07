---
title: "Writing Extensions"
weight: 2
description: "The in-process Extension trait: hook custom Rust logic into the super-core lifecycle."
---

`super-core` exposes one supported, in-process extension point: the **`Extension` trait**. It is middleware-style — hooks are invoked by the `Manager` at key points of the process lifecycle, and every method has a default implementation so you only override what you need.

A typical use case is "do something special around every managed process": inject secrets fetched from a central store, block a start when the environment is not ready, apply per-process tuning right after spawn (with fail-secure semantics), or observe crash events for custom auditing.

## The trait

```rust
pub trait Extension: Send + Sync {
    fn before_start(&self, id: Uuid, config: &ProgramConfig)
        -> anyhow::Result<Option<HashMap<String, String>>>;
    fn after_start(&self, id: Uuid, pid: u32, config: &ProgramConfig) -> anyhow::Result<()>;
    fn before_stop(&self, id: Uuid, config: &ProgramConfig) -> anyhow::Result<()>;
    fn after_stop(&self, id: Uuid, config: &ProgramConfig) -> anyhow::Result<()>;
    fn on_event(&self, event: SystemEvent);
    fn on_reload(&self) -> anyhow::Result<()>;
    fn on_shutdown(&self) -> anyhow::Result<()>;
    fn on_update(
        &self,
        id: Uuid,
        pid: Option<u32>,
        old_config: &ProgramConfig,
        new_config: &ProgramConfig,
    ) -> anyhow::Result<()>;
    fn collect_metrics(&self) -> String;
    fn supports_resource_limits(&self) -> bool;
}
```

(Signatures match `core/src/extension/mod.rs`; default bodies are omitted above.)

> [!NOTE]
> The trait declares `before_stop`, and the code block above mirrors it for accuracy, but the host **never invokes it today** (the plugin C-ABI has no slot for it either). It is shown here because this page documents the trait as it exists in the source; treat it as reserved. See the note in the hook table below.

| Hook | Timing | Notes |
| :--- | :--- | :--- |
| `before_start` | Before a process is spawned | Returned vars are merged into the program environment. Returning `Err` **aborts the start** and marks the program `Fatal` with your error message. |
| `after_start` | Immediately after the PID is assigned (blocking thread) | Apply post-spawn work (e.g. cgroup limits, process tuning). **Fail-secure:** returning `Err` kills the just-started child immediately and marks the program `Fatal`. |
| `before_stop` | — | **Reserved; not currently invoked by the host.** Do not rely on it. For stop-adjacent reactions, use the per-program `pre_stop` [lifecycle hook](/docs/03-orchestration/lifecycle-hooks) (runs before the stop signal), or observe related lifecycle events such as `process_fatal` / `process_backoff` in `on_event`. |
| `after_stop` | Every process exit (`handle_exited`) **and** program removal from the registry; fire-and-forget on a blocking thread | Cleanup and resource release (e.g. the licensed `isolation` plugin removes the program's cgroup here). Errors are swallowed on the exit path, logged on removal. |
| `on_event` | On any system event, synchronously on the event path | Observe start / stop / crash events; the callback is synchronous (no `Result`) — keep it fast, enqueue heavy work. See [System Events](/docs/03-orchestration/events/types). |
| `on_reload` | When the host reloads configuration | Errors are logged, not fatal. |
| `on_update` | When a config update changes a program's `resource_limits` | `pid` is `Some` while the program is running, so the extension can re-apply limits live. |
| `on_shutdown` | During graceful host shutdown | Runs before the final shutdown event is emitted. |
| `collect_metrics` | When `/metrics` is scraped | Output (Prometheus text format) is appended under `# --- Extension Metrics ---`. |
| `supports_resource_limits` | Advertised to the host | Return `true` only when your extension actually enforces `ProgramConfig::resource_limits`. Without a supporting extension, `resource_limits` values are stored but **not enforced** (the host logs a warning when they are set). |

## A minimal extension

Because every hook has a default, a useful extension can be very small:

```rust
use common::{ProgramConfig, SystemEvent};
use std::collections::HashMap;
use super_core::extension::Extension;
use uuid::Uuid;

#[derive(Clone)]
struct SecretInjector {
    token: String,
}

impl Extension for SecretInjector {
    fn before_start(
        &self,
        _id: Uuid,
        _config: &ProgramConfig,
    ) -> anyhow::Result<Option<HashMap<String, String>>> {
        // Errors abort the start; returned vars join the child's environment.
        Ok(Some(HashMap::from([(
            "APP_TOKEN".to_string(),
            self.token.clone(),
        )])))
    }

    fn on_event(&self, event: SystemEvent) {
        // Observe lifecycle events as they happen. This runs synchronously on
        // the event path — keep it fast; enqueue, don't block.
        println!("super event: {}", event.event_type());
    }
}
```

## Embedding `super-core`

Extensions are compiled into the process: you link `super-core` and pass your extension to `bootstrap()`, which starts the `Manager` actor, logging, and state recovery. `superd` is itself a thin embedding of `super-core`.

`Cargo.toml`:

```toml
[dependencies]
super-core = { git = "https://github.com/schiplat/super", rev = "main" }
common     = { git = "https://github.com/schiplat/super", rev = "main" }
anyhow     = "1"
uuid       = { version = "1", features = ["v4"] }
tokio      = { version = "1", features = ["rt-multi-thread", "macros", "signal"] }
```

> [!NOTE]
> Pin the **same revision** for both crates (they come from one repository), and prefer a release tag for production builds. The repository requires Rust **1.85+** (edition 2024).

`main.rs`:

```rust
use super_core::extension::Extension;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let core = super_core::bootstrap(Box::new(SecretInjector {
        token: std::env::var("APP_TOKEN").unwrap_or_default(),
    }))
    .await?;

    // Wait for Ctrl-C, then ask the Manager to shut down gracefully:
    // state is flushed and `on_shutdown` hooks run.
    tokio::signal::ctrl_c().await?;
    core.manager_handle.shutdown().await?;
    Ok(())
}
```

The instance layout is the usual one rooted at `SUPER_ROOT` (`conf/super.toml`, `data/`, `logs/`, `plugins/`), so run your binary the same way you run `superd`:

```sh
export SUPER_ROOT=/tmp/super-dev
./target/debug/my-host
```

> [!NOTE]
> `bootstrap()` starts the process manager but does not serve HTTP. A full daemon additionally builds the axum router from the `SystemCore` handle (as `superd/src/main.rs` does). For headless tooling you can drive everything through `core.manager_handle` (`list_programs`, `start_program`, `stop_program`, `shutdown`, …) without any network server.

## Extensions vs runtime plugins

There are two distinct surfaces, and they must not be confused:

*   **In-process `Extension` (this page).** Rust code compiled into a binary that embeds `super-core`. Available to anyone, OSS.
*   **Runtime plugins.** Separate native libraries delivered with a licensed subscription and loaded by `superd` from `$SUPER_ROOT/plugins/` after license verification. `superd` bridges each authorized plugin onto the same `Extension` interface internally, so plugin behavior follows the semantics in the table above — but arbitrary third-party libraries are **not** loaded unless the license authorizes them.

If you want to add custom behavior to the stock `superd` binary, the options are: contribute the change upstream, or embed `super-core` in your own binary with your own extensions (this page). See [Editions](/docs/07-editions) for the OSS / licensed split.

## Learn more

*   [Custom Extensions](/docs/04-production-scenarios/extensibility/custom-extensions) — extension use cases in production scenarios.
*   [Lifecycle Hooks](/docs/03-orchestration/lifecycle-hooks) — the scriptable, per-program hooks that complement extensions.
*   [System Events](/docs/03-orchestration/events/types) — the event catalog delivered to `on_event`.
*   [Metrics](/docs/04-production-scenarios/observability/instant-metrics) — where `collect_metrics` output appears.
