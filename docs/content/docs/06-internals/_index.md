---
title: "Internals & Reference"
weight: 6
description: "Architecture design, technical FAQ, and complete API references."
---

Super is built on the philosophy of **Transparency**. We believe you should understand how your process manager works under the hood so you can trust it with your critical infrastructure.

### In this section

#### Architecture
*   [**Design Philosophy**](./design-philosophy): System overview diagram, Rust rationale, Actor Model, WAL, and defensive defaults.
*   [**FAQ**](./faq): Technical deep-dives into Zombies, Signals, and Systemd comparisons.

#### Reference Manuals
*   [**What's New in 1.4.0**](./whats-new-1.4.0): Readiness-aware reload, Unix socket transport, timestamped child logs.
*   [**Config Reference**](./config-reference): Complete `super.toml` schema.
*   [**CLI Reference**](./cli-reference): Command-line arguments and flags.
*   [**Environment Variables**](./environment-variables): Public env vars for `superd` and the `super` CLI.
*   [**API Reference**](./api-reference): HTTP endpoints and JSON schemas.
