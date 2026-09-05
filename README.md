# Project Super

**The API-First, Lightweight Process Orchestrator for the Edge.**

Super is a modern replacement for tools like [Supervisor](https://supervisord.org/) or [PM2](https://pm2.keymetrics.io/), built with **Rust**. It is designed for edge computing, IoT devices, and high-performance servers.

> **Public beta**
>
> Super `1.x` is feature-complete and in active hardening. The core process-management paths (start/stop/restart, auto-recovery, health checks, OTA rollback) are covered by integration tests and run in the maintainers' own deployments. We recommend it for staging and non-critical workloads today; see [below](#toward-ga) for what we require before calling it production-ready (GA).
>
> - **OSS core** (`superd` + `super`) is free under MIT — install and try anytime.
> - **Super Pro plugins** (Dashboard UI, API auth/RBAC/audit, notifications with storm suppression, Linux cgroup isolation) are available with a **free 90-day license** during the beta. No payment required.
>
> **Request a free Pro trial:** open a [GitHub Issue](https://github.com/schiplat/super/issues/new?template=pro-trial.yml) (use the **Pro trial request** template). Include a contact email — we will send the license key and plugin package to that address.

> **Documentation:** [https://super.docs.sconts.com/docs/](https://super.docs.sconts.com/docs/)

## Core Features

* **Single binary** — Rust `superd` process manager; TOML or REST config; CLI and HTTP API (Dashboard via optional `ui` plugin)
* **Declarative orchestration** — stacks, dependencies, health checks
* **Lifecycle hooks** — `pre_start`, `post_start`, `post_stop`, and global event hooks
* **Observability** — WebSocket logs, historical logs API, system metrics
* **Auto-recovery** — Supervisor-compatible `autorestart`, `exitcodes`, `startsecs`

Licensed under the **[MIT License](LICENSE)**. Optional **licensed plugins** (`.so` / `.dylib` under `$SUPER_ROOT/plugins/`) add API auth, RBAC, notifications ([storm suppression](https://super.docs.sconts.com/docs/05-advanced-management/event-notifications/#storm-suppression)), and cgroup limits — same `superd` binary, no separate commercial build. Compare editions in the [feature matrix](https://super.docs.sconts.com/docs/07-editions/feature-matrix/).

## Quick Start

### Install script (Linux / macOS / FreeBSD)

```bash
curl -fsSL https://github.com/schiplat/super/releases/latest/download/install.sh | sh
```

Installs `superd` and `super`, creates a minimal instance root (`/opt/super` or `~/.super`), and enables an OS service (**systemd** on Linux, **launchd** on macOS, **rc.d** on FreeBSD) with boot start. Verifies the SHA-256 of the release archive.

Options: `--version`, `--prefix`, `--root`, `--user` / `--system`, `--no-service`, `--no-start`, `--no-init`, `--no-sudo`.

Bleeding-edge (may differ from the latest tagged binaries): `curl -fsSL https://raw.githubusercontent.com/schiplat/super/master/install.sh | sh`.

### Docker

Docker image (`linux/amd64`, `linux/arm64`). **This is the supported path on Windows** (Docker Desktop or WSL2) — there is no native `superd.exe` release. The OSS image has **no API authentication** — bind to loopback on the host unless you add the `security` plugin and a license:

```bash
docker pull schiplat/super:latest
docker run --rm -p 127.0.0.1:9002:9002 schiplat/super:latest
```

With a custom config directory:

```bash
docker run --rm -p 127.0.0.1:9002:9002 -v ./packaging/docker/conf:/app/super/conf schiplat/super:latest
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

See [Installation](https://super.docs.sconts.com/docs/01-getting-started/installation/) for `install.sh` (systemd / launchd / rc.d), manual units (`superd --foreground`), and optional `--daemon`.

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
| Changelog | [Changelog](https://super.docs.sconts.com/docs/08-changelog/) |
| Editions / Pro plugins | [Feature matrix](https://super.docs.sconts.com/docs/07-editions/feature-matrix/) |

### AI skills

Want your AI assistant (Cursor, Claude, Copilot, …) to configure and troubleshoot Super correctly? Point it at [`docs/SKILL.md`](docs/SKILL.md) (e.g. from `CLAUDE.md`, `.cursor/rules`, or pasted into the prompt). It covers everyday commands, the `super.toml` / stack JSON schema, cron & health semantics, and common failure modes — and helps avoid the usual supervisor/PM2 semantic mix-ups.

## Repository layout

| Path | Role |
|------|------|
| `common/` `core/` `cli/` `superd/` | Cargo workspace crates (`superd` + `super` CLI) |
| `packaging/docker/` | Official image Dockerfile + baked-in conf (was `dockerbuild/`) |
| `packaging/contrib/` | Default `super.toml` + systemd / launchd / rc.d templates (copied into release tarballs as `contrib/`) |
| `examples/demo/` | Sample `SUPER_ROOT`-style conf and stack files |
| `tools/benchmark/` | Peer benchmark suite |
| `tools/scripts/` | Install smoke + OTA e2e helpers |
| `docs/` | Public Hugo documentation site |
| `install.sh` | Release install script (kept at repo root for stable download URLs) |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Security issues: [SECURITY.md](SECURITY.md). Community standards: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
