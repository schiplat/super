---
title: "Building from Source"
weight: 1
description: "Toolchain, workspace layout, build and test commands, and running a local dev instance."
---

Building from source is the fastest way to try changes, inspect internals, or run a custom build. The OSS repository is a single Cargo workspace that compiles both binaries from one `make build`.

## Prerequisites

*   **Rust** stable **1.85 or newer** (the workspace uses edition 2024).
*   **Git** — the docs preview also needs the Hextra theme submodule (see [Docs preview](#docs-preview)).

## Clone and workspace layout

```sh
git clone https://github.com/schiplat/super.git
cd super
```

The workspace (`Cargo.toml`) has four crates:

| Crate | Path | Type | Role |
| :--- | :--- | :--- | :--- |
| `common` | `common/` | lib | Shared types: config schema, license verification, plugin ABIs, paths, program validation |
| `super-core` | `core/` | lib | The daemon engine: `Manager` actor, axum REST/WS API, runtime plugin host, scheduler, health checks, hooks, event history (SQLite), snapshot store |
| `superd` | `superd/` | bin `superd` | The daemon entry point: CLI args, plugin discovery and license checks, then `bootstrap()` of `super-core`, serving HTTP / WS / metrics |
| `super-cli` | `cli/` | bin `super` | Command-line client for the daemon's HTTP API |

`superd` is intentionally thin: nearly all logic lives in `super-core`, and embedding `super-core` in your own binary is a supported pattern (see [Writing Extensions](/docs/09-development/writing-extensions)).

## Build the binaries

```sh
make build            # cargo build --release --bin superd --bin super
```

Artifacts land in `target/release/`:

```text
target/release/superd   # the daemon
target/release/super    # the CLI
```

For an unoptimized dev build use `cargo build --bin superd --bin super`.

## Run tests, lint, and audit

CI runs the same gates on every PR:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Integration tests live in `core/tests/` and cover restart policy, config loading, health checks, hooks, logs, OTA rollback, snapshots, scheduling, and more.

Dependency vulnerabilities are checked with [`cargo audit`](https://github.com/rustsec/rustsec) (install once with `cargo install cargo-audit`, then run with `cargo audit`). A clean `cargo audit` is a stated release-branch gate for the project — run it locally before opening a PR that touches `Cargo.toml` / `Cargo.lock` or when verifying a release branch.

## Run a local dev instance

The daemon resolves its layout from `SUPER_ROOT` (default: exe-relative/cwd). Point it at a scratch directory so logs and data never land inside the repository:

```sh
export SUPER_ROOT=/tmp/super-dev
mkdir -p "$SUPER_ROOT/conf"
./target/release/superd            # foreground; API on 127.0.0.1:9002 by default
```

If `conf/super.toml` is missing, `superd` starts with defaults. In another shell, drive it with the CLI (same `SUPER_ROOT`):

```sh
export SUPER_ROOT=/tmp/super-dev
./target/release/super add --name demo --autostart /usr/bin/sleep 3600
./target/release/super list
./target/release/super shutdown     # graceful stop
```

Useful checks while iterating:

*   `curl http://127.0.0.1:9002/health` — liveness.
*   `super doctor` — diagnose config, daemon connectivity, and `SUPER_ROOT` layout in one shot.
*   Logs: `$SUPER_ROOT/logs/app.log.YYYY-MM-DD` for the daemon, `$SUPER_ROOT/logs/{uuid}.out` / `{uuid}.err` for managed children.

> [!NOTE]
> Building from source always yields **OSS mode**. Licensed capabilities are separate signed plugin libraries delivered with a subscription and dropped into `$SUPER_ROOT/plugins/`; they are not built from this repository. See [Editions](/docs/07-editions).

## Docs preview

Docs preview requires [Hugo Extended](https://gohugo.io/installation/) **0.163.x+** and the Hextra submodule:

```sh
git submodule update --init --recursive
make docs-serve          # → http://localhost:1313/
```

## Next steps

*   [Writing Extensions](/docs/09-development/writing-extensions) — hook custom logic into the process lifecycle.
*   [Design Philosophy](/docs/06-internals/design-philosophy) — how the core is architected.
*   [API Reference](/docs/06-internals/api-reference) and [Config Reference](/docs/06-internals/config-reference) — what a running instance exposes.
