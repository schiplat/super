---
title: "Custom Extensions"
weight: 1
description: "Inject custom logic: Config fetching, Audit logging, and Hardware initialization."
---

Most process managers are closed systems. If you want them to do something they weren't designed for (like fetching secrets from Vault before starting a process), you usually have to write complex wrapper scripts.

Super takes a different approach. It features an **Open Kernel Architecture**.

## The `Extension` Trait

At the heart of Super Core is the `Extension` trait. Licensed plugins (cgroups isolation, webhooks, audit) implement this interface and are loaded from `plugins/` at runtime when licensed.

You can write your own Rust plugins to hook into the lifecycle.

```rust
pub trait Extension: Send + Sync {
    // Inject environment variables before spawn
    fn before_start(&self, id: Uuid, config: &ProgramConfig)
        -> Result<Option<HashMap<String, String>>>;

    // Run logic immediately after spawn (e.g., set Cgroups)
    fn after_start(&self, id: Uuid, pid: u32, config: &ProgramConfig)
        -> Result<()>;

    // Handle system events (e.g., custom logging)
    fn on_event(&self, event: SystemEvent);
}
```

## Use Cases

### 1. Configuration Injection (e.g., Nacos/Consul)

**Scenario**: Your app needs database credentials, but they are stored in a central config server (Nacos), not in static files.

**Extension Logic (`before_start`)**:
1.  Intercept the start request.
2.  Connect to Nacos HTTP API using the program name.
3.  Fetch the config JSON.
4.  Return them as a `HashMap`.
5.  Super injects them as environment variables (`DB_PASSWORD=...`) into the process.

**Result**: The application starts with fresh credentials, without any wrapper scripts inside the container.

### 2. Specialized Auditing

**Scenario**: You work in a regulated industry (Finance/Healthcare). A generic Webhook isn't enough; you need to write audit logs to a local encrypted Kafka queue or a hardware security module (HSM) whenever a process crashes.

**Extension Logic (`on_event`)**:
1.  Listen for `ProcessFatal` events.
2.  Serialize the event details.
3.  Push directly to your internal Kafka topic using a Rust client.

### 3. Hardware Initialization (IoT)

**Scenario**: You are running Super on an embedded Linux device. Before starting the `motor-control` binary, you must ensure the GPIO pins are exported and set to specific modes.

**Extension Logic (`before_start`)**:
1.  Check if the program name is `motor-control`.
2.  Write to `/sys/class/gpio/...` to initialize hardware.
3.  If initialization fails, return an `Err`.
4.  Super aborts the start, preventing the app from running in an undefined hardware state.

## Building Your Own

> [!NOTE]
> Built-in licensed plugins (cgroups, webhooks, audit, etc.) ship as separate `.so` libraries, not in this repository. OSS `superd` loads them at runtime when `[license].key` in `conf/super.toml` authorizes them. The pattern below shows how you would link `super-core` for a custom binary.

To build a custom extension, compile your own binary linking against `super-core`.

```toml
# Cargo.toml
[dependencies]
super-core = { git = "https://github.com/schiplat/super" }
```

In your `main.rs`:

```rust
use super_core::bootstrap;

struct MyCustomExtension;
impl Extension for MyCustomExtension { ... }

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Inject your extension into the core
    let core = bootstrap(Box::new(MyCustomExtension)).await?;
    // ...
}
```

This architecture ensures Super can adapt to your specific enterprise needs while keeping the core lightweight and stable.
