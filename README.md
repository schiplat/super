# 🦸 Project Super

**The API-First, Lightweight Process Orchestrator for the Edge.**

Super is a modern replacement for tools like [Supervisor](https://supervisord.org/) or [PM2](https://pm2.keymetrics.io/), built with **Rust**. It is designed for edge computing, IoT devices, and high-performance servers.

> **Public beta**
>
> Super `1.x` is feature-complete and in active hardening. The core process-management paths (start/stop/restart, auto-recovery, health checks, OTA rollback) are covered by integration tests and run in the maintainers' own deployments. We recommend it for staging and non-critical workloads today; see [below](#toward-ga) for what we require before calling it production-ready (GA).
>
> - **OSS core** (`superd` + `super`) is free under MIT — install and try anytime.
> - **Super Pro plugins** (Dashboard UI, API auth/RBAC/audit, notifications, Linux cgroup isolation) are available with a **free 1-month license** during the beta. No payment required.
>
> **Request a free Pro trial:** open a [GitHub Issue](https://github.com/schiplat/super/issues/new?template=pro-trial.yml) (use the **Pro trial request** template). Include a contact email — we will send the license key and plugin package to that address.

> **Documentation:** [https://super.docs.sconts.com/docs/](https://super.docs.sconts.com/docs/)

## Core Features

* **Single binary** — Rust `superd` process manager; TOML or REST config; CLI and HTTP API (Dashboard via optional `ui` plugin)
* **Declarative orchestration** — stacks, dependencies, health checks
* **Lifecycle hooks** — `pre_start`, `post_start`, `post_stop`, and global event hooks
* **Observability** — WebSocket logs, historical logs API, system metrics
* **Auto-recovery** — Supervisor-compatible `autorestart`, `exitcodes`, `startsecs`

Licensed under the **[MIT License](LICENSE)**. Optional **licensed plugins** (`.so` / `.dylib` under `$SUPER_ROOT/plugins/`) add API auth, RBAC, notifications, and cgroup limits — same `superd` binary, no separate commercial build. Compare editions in the [feature matrix](https://super.docs.sconts.com/docs/07-editions/feature-matrix/).

## Quick Start

### Install script (Linux / macOS / FreeBSD)

```bash
curl -fsSL https://raw.githubusercontent.com/schiplat/super/master/install.sh | sh
```

Installs `superd` and `super` into `/usr/local/bin` (or `~/.local/bin` when not writable), verifying the SHA-256 checksum of the release archive. Options: `--version X.Y.Z`, `--prefix DIR`, `--no-sudo`.

### Docker

Docker image (`linux/amd64`, `linux/arm64`). The OSS image has **no API authentication** — bind to loopback on the host unless you add the `security` plugin and a license:

```bash
docker pull containerpi/super:latest
docker run --rm -p 127.0.0.1:9002:9002 containerpi/super:latest
```

With a custom config directory:

```bash
docker run --rm -p 127.0.0.1:9002:9002 -v ./dockerbuild/conf:/app/super/conf containerpi/super:latest
```

### From source

Requires **Rust 1.85+** (stable):

```bash
git clone https://github.com/schiplat/super.git && cd super
make build
./target/release/superd              # foreground (default; use under systemd/Docker)
# ./target/release/superd --daemon   # optional Unix self-daemonize without systemd
```

### CLI

```bash
super add --name redis --autostart /usr/bin/redis-server
super list
super logs <id> --tail
super shutdown                       # stop superd (foreground or --daemon)
```

Diagnose a setup (config, daemon connectivity, license, daemon/pidfile hints) in one shot:

```bash
super doctor
```

See [Installation](https://super.docs.sconts.com/docs/01-getting-started/installation/) for systemd units (`superd --foreground`) and optional `--daemon` / `[server] daemon`.

## Toward GA

We will call Super production-ready (GA) when the following are true. If you rely on Super today, this is the contract we are working against — feedback on any of these is the most valuable contribution right now.

- **Stability** — no known panic paths in the daemon on malformed config or API input; graceful degradation when a plugin fails.
- **Upgrade safety** — OTA updates are transactional (backup → verify → commit/rollback) and covered by integration tests.
- **Security defaults** — fail-closed network binding, signed-plugin verification, and no secrets in API/CLI output; `cargo audit` clean on release branches.
- **Operability** — `super doctor` diagnoses a deployment end-to-end; logs and metrics are sufficient to triage without a debugger.
- **API stability** — the REST API and the plugin C ABI (`PLUGIN_API_VERSION`) are versioned; breaking changes ship only with a major bump and migration notes.

Track progress in the [changelog](https://super.docs.sconts.com/docs/08-changelog/).

## Documentation

| Topic | Link |
|-------|------|
| Getting started | [Docs](https://super.docs.sconts.com/docs/01-getting-started/) |
| Configuration | [Config reference](https://super.docs.sconts.com/docs/06-internals/config-reference/) |
| API | [API reference](https://super.docs.sconts.com/docs/06-internals/api-reference/) |
| Changelog | [v1.3.2](https://super.docs.sconts.com/docs/08-changelog/) |
| Editions / Pro plugins | [Feature matrix](https://super.docs.sconts.com/docs/07-editions/feature-matrix/) |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Security issues: [SECURITY.md](SECURITY.md). Community standards: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
