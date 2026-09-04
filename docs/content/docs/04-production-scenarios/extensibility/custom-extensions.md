---
title: "Custom Extensions"
weight: 1
description: "Inject custom logic: config fetching, audit logging, and hardware initialization."
---

Most process managers are closed systems. If you want them to do something they weren't designed for (like fetching secrets from a central store before starting a process), you usually have to write complex wrapper scripts.

Super takes a different approach. The core exposes an **`Extension` trait**: a middleware-style interface whose hooks run around every managed process. Licensed plugins (cgroups isolation, notifications, audit) and your own compiled-in logic all plug into this same interface — you can adapt Super to your needs without forking the core.

> [!NOTE]
> There are **two extension surfaces**:
>
> * **In-process `Extension`** — Rust code compiled into a binary that embeds `super-core`. This is the OSS, always-available path.
> * **Runtime plugins** — separate native libraries delivered with a licensed subscription. `superd` verifies the license, then loads authorized libraries from `$SUPER_ROOT/plugins/` and bridges them onto the same `Extension` interface internally.
>
> For the full trait reference, a buildable example, and embedding instructions, see [Writing Extensions](/docs/09-development/writing-extensions).

## How it works

The trait provides hooks with default implementations, so an extension only implements the moments it cares about:

| Hook | When it runs | What it lets you do |
| :--- | :--- | :--- |
| `before_start` | Before a process is spawned | Inject environment variables, or return an error to **abort** the start |
| `after_start` | Right after the PID is assigned | Apply per-process setup (e.g. limits) |
| `before_stop` / `after_stop` | Around process stop | Drain, deregister, clean up |
| `on_event` | On system events | Observe start / stop / crash events for custom handling |
| `on_reload` / `on_shutdown` / `on_update` | Host lifecycle moments | React to reload, graceful shutdown, and config updates |

## Use cases

### 1. Configuration injection (e.g. Nacos/Consul)

**Scenario**: Your app needs database credentials, but they are stored in a central config server, not in static files.

**Extension logic (`before_start`)**:

1.  Intercept the start request.
2.  Connect to the central config HTTP API using the program name.
3.  Fetch the config JSON and return it as a `HashMap`.
4.  Super merges the variables (e.g. `DB_PASSWORD=...`) into the process environment.

**Result**: The application starts with fresh credentials, with no wrapper scripts inside the container.

### 2. Specialized auditing

**Scenario**: You work in a regulated industry (Finance/Healthcare). A generic webhook isn't enough; you need audit records written to a local encrypted queue or hardware security module (HSM) whenever a process crashes.

**Extension logic (`on_event`)**:

1.  Listen for fatal process events.
2.  Serialize the event details.
3.  Push them to your audit sink from Rust.

### 3. Hardware initialization (IoT)

**Scenario**: You are running Super on an embedded Linux device. Before starting the `motor-control` binary, you must ensure the GPIO pins are exported and set to specific modes.

**Extension logic (`before_start`)**:

1.  Check whether the program name is `motor-control`.
2.  Write to `/sys/class/gpio/...` to initialize hardware.
3.  If initialization fails, return an `Err` — Super aborts the start, preventing the app from running in an undefined hardware state.

## Building your own

Extensions are compiled into a host binary that links `super-core` and passes the extension to `bootstrap()`. `superd` is itself a thin embedding of `super-core`, so this pattern is well-trodden.

```toml
# Cargo.toml
[dependencies]
super-core = { git = "https://github.com/schiplat/super" }
common = { git = "https://github.com/schiplat/super" }
```

```rust
use super_core::extension::Extension;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let core = super_core::bootstrap(Box::new(MyExtension)).await?;
    // ... drive core.manager_handle or serve an API, like superd does ...
    Ok(())
}
```

A complete, accurate walkthrough — full hook semantics, a working example, and the licensed-runtime boundary — is in [Writing Extensions](/docs/09-development/writing-extensions).
