---
title: "Changelog"
weight: 8
description: "All notable changes to Project Super will be documented in this file."
---

All notable changes to **Project Super** will be documented in this page.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [1.4.0] - 2026-08-31

### Breaking

- `resource_limits.memory_limit` is now in **MB** (binary, `1 MB = 1024² bytes`) instead of bytes, and `resource_limits.cpu_quota` is now in **cores** (`1.0` = one full core) instead of percent. Old configs written in bytes/percent are **not** auto-converted — update them for the new semantics. Event payloads (`usage_bytes`, `limit_bytes`, `anon_bytes`) keep byte precision. CLI flags `--memory <MB>` / `--cpu <cores>` match.
- Plugin ABI `PLUGIN_API_VERSION` bumped to `2`: `super_plugin_v1.init` now receives a `SuperPluginHostV1` callback table (plugins can emit `SystemEvent`s into superd's pipeline). Third-party plugins built for API v1 must be rebuilt.

### Added

- **Three-tier memory warning & visibility** (licensed `isolation` plugin, Linux): `memory_pressure` pre-kill warning (Tier 1) when anonymous memory crosses `memory_warn_percent` / `memory_warn_headroom`; optional `memory_high` kernel soft limit (Tier 2, `memory.high` throttling); `memory_oom_kill` post-kill confirmation (Tier 3) that distinguishes a cgroup OOM kill from a manual `kill -9`.
- Plugin → host event bridge: the `isolation` plugin emits `memory_pressure` / `memory_oom_kill` through the same event pipeline as lifecycle events (event history, OSS event hooks, licensed notifications).
- **Readiness-aware reload**: `super reload --wait [--timeout]` (and `POST /api/v1/system/reload?wait=&timeout=`) waits for every affected program to pass its health checks before reporting success, and exits non-zero if any program is not ready in time. `super start`/`restart` also gain `--wait-healthy` (wait for `Healthy` rather than just `Running`).
- **Batch operation safety**: `super start|stop|restart|remove|signal` on `@group` / `all` targets print the list of affected programs and ask for confirmation before proceeding. Global `--yes`/`-y` skips the prompt for scripts, and `--dry-run` prints the preview list and exits without executing anything. Single-target operations never prompt.
- **OTA verification window**: new `[server] ota_verify_timeout` (default `60`, `0` disables). After an OTA update restarts a program, the new version must pass its health checks within the window; on timeout the daemon automatically rolls back to the previous version (file rollback → WAL recovery → old version restarted), so a bad artifact cannot leave the service down.
- **Unix socket transport**: `[server] socket = "run/superd.sock"` exposes the API on a Unix domain socket with owner-only permissions by default (`socket_mode = "0600"`; `"0660"` / `"0640"` grant group access, world-writable modes are refused). `socket_only = true` disables the TCP listener entirely — zero network exposure for local-only management. The CLI connects via `super --server unix:///path/to/superd.sock` (REST and `super logs --follow` WebSocket both ride the socket); when neither `--server` nor a persisted `~/.super/cli.json` endpoint is given, the CLI auto-discovers `$SUPER_ROOT/run/superd.sock` (a real socket file) and prefers it over TCP, falling back to the default `http://127.0.0.1:9002`. superd refuses to start on a path that is a symlink, a non-socket file, or a socket held by another live process; stale sockets are cleaned up on restart.

### Changed

- `super update` hot-updates `memory_warn_percent`, `memory_warn_headroom`, and `memory_high` on running programs alongside `memory_limit` / `cpu_quota`.
- Dashboard create/edit forms expose the three new memory fields; process detail shows MB/cores units.
- Program event history raised from 50 to **100 events per program** (oldest dropped); every record carries a Unix `ts` timestamp (docs now state it explicitly).
- Captured child `stdout`/`stderr` lines are now prefixed with a timestamp by default: `[YYYY-MM-DD HH:MM:SS]` (daemon's local time). New `[child_logging] timestamp = "local" | "utc" | "none"` controls the prefix (`none` restores the previous raw format). The WebSocket log stream still carries the raw, un-prefixed line.

### Fixed

- Managed children no longer inherit superd's `oom_score_adj=-1000`: the kernel can now actually OOM-kill a program that exceeds `memory_limit` (previously the hard cap turned into a livelock). The daemon's own OOM protection is unchanged.
- `memory_pressure` now fires immediately on the first threshold crossing (cooldown no longer swallows the first warning).
- `memory_oom_kill` is no longer lost when the cgroup is torn down right after the process exits (`after_stop` does a final synchronous `oom_kill` counter check).
- `super-pro` build no longer warns about a missing legacy `super/common/keys/public.key` when named `*.public.key` verifying keys are present.

### Notes

- Workspace **1.4.0**; pair with commercial plugin packages `super-plugins-1.4.0-…`.

---

## [1.3.3] - 2026-08-31

### Added
- **OSS webhook event hooks**: `[[event_hooks]]` with `type = "webhook"` posts `SystemEvent` JSON (HMAC-SHA256 signed) to external URLs without any Pro plugin. `type = "command"` hooks also capture the program `signal`.
- **Event history**: program lifecycle events are persisted to `data/events.json`; the new `super events` CLI shows recent events (`--tail`, `--program`, `--json`).
- Cron jobs are exempt from flapping detection — short-interval schedules no longer flip to `Fatal` / disable `autostart`.

### Changed
- `depends_on` now **starts** non-running dependencies automatically (cycle-safe; waiting/restarting/crashed deps are not forced).
- `super remove` is the primary command name; `rm` remains a clap alias. CLI help and docs updated throughout.
- `super signal --sig` help now lists `quit` (server already supported it); API reference batch `Signal` payload aligned.
- Config docs aligned with code: `super.toml` lookup paths, health-check interval (fixed 5s), and isolation plugin log wording.
- Pro beta trial shortened from 1 year to **1 month** (GitHub Issue template + docs); purchased annual keys keep the 365-day term.

### Fixed
- `super token list` no longer fails on deserialization — the CLI now expects the API's `AuthTokenInfo` shape (no `token_hash`).
- Batch-action confirmation prompt (`super stop/start/restart/remove/signal all` and `@group`) now shows the real target count.

### Notes
- Workspace **1.3.3**; pair with commercial plugin packages `super-plugins-1.3.3-…`.

---

## [1.3.2] - 2026-08-29

### Added
- `super keyring` lists license verifying key ids embedded in the CLI binary; `super doctor` prints the same summary line.
- `[license].strict` and deployment-intent detection: invalid keys refuse startup when `strict = true`, plugin libraries are present, `auth_secret` is set, or bind is non-loopback; otherwise OSS degrade with warnings (`SUPER_LICENSE_STRICT` env override).

### Changed
- Create / update program (HTTP, CLI `add`/`update`, dashboard, stack apply) run a shared structural check. Failures return **400** with a `message` that names the field (`command:`, `health_check.url:`), the program or `services[i] (name=…)`, and for JSON syntax / unknown keys `JSON line N column M`. `super check` reports include problems as `path:line:col:` or `path: services[i] (name=…): field:`.
- `super check` reports invalid license as an error when strict or licensed deployment signals apply.
- License verification requires a `kid` claim on every license; the legacy `v1` kid and compile-time fallback are removed. Signing key ids use `k_<8hex>` (derived from the Ed25519 public key). Re-issue licenses from your vendor if verification fails with “missing signing key id”.
- Docs: [Troubleshooting license verification](/docs/05-advanced-management/authentication#troubleshooting-license-verification) — suggested `super check`, `super doctor`, and `super keyring` for self-service diagnosis.

### Notes
- Workspace **1.3.2**; pair with commercial plugin packages `super-plugins-1.3.2-…`.

---

## [1.3.1] - 2026-08-27

### Added
- `CODE_OF_CONDUCT.md` (Contributor Covenant).

### Changed
- **Public beta** messaging aligned across README, docs, and GitHub issue templates.
- CI: `cargo fmt --check`, **macOS** test matrix, `benchmark/` workspace build.
- Docker Hub publishes **`linux/amd64`** and **`linux/arm64`** again; docs use loopback port mapping.
- Document minimum **Rust 1.85+**; workspace `rust-version = "1.85"`.
- License help links point to the [feature matrix](https://super.docs.sconts.com/docs/07-editions/feature-matrix/) and [Pro trial](https://github.com/schiplat/super/issues/new?template=pro-trial.yml).
- Remove unsupported `[webhook]` from `super.toml` (use `[[event_hooks]]` or `conf/notify.toml`).
- Invalid license: prominent startup warnings; improved `super doctor` triage.
- `/go/pro/` license examples updated for the **1.3.x** release line.

### Fixed
- `h2` and `lru` RustSec advisories via dependency updates.
- `/go/pro/` checkout CTA uses configured purchase URL.

### Notes
- Workspace **1.3.1**; pair with commercial plugin packages `super-plugins-1.3.1-…`.

---

## [1.3.0] - 2026-08-04

### Fixed
- Release CI Manager keyring fetch: require explicit env (no defaults); harden HTTP/JSON errors; **upsert** verifying keys without deleting existing `common/keys/*.public.key` (hand-curated git keys + Release-time Manager ring).

### Notes
- Workspace 1.3.0; pair with commercial plugin packages `super-plugins-1.3.0-…`.
- Git `common/keys/` stays hand-curated; `fetch-verifying-keys.sh` runs in Release CI only (not routinely committed).

---

## [1.2.5] - 2026-07-26

### Added
- `superd --daemon` / `[server] daemon` (Unix): optional self-daemonize with pidfile (`run/superd.pid` by default, overridable via `[server] pidfile` / `--pidfile`). Refuses to start under systemd or as PID 1. `super doctor` reports conflicts and stale pidfiles. Keep foreground under systemd (`superd --foreground`).

### Notes
- Pair OSS `1.2.5` with matching commercial plugin packages (`super-plugins-1.2.5-…`).

---

## [1.2.4] - 2026-07-26

### Changed
- License docs/API wording: version span → **version scope**.
- [Web UI](/docs/05-advanced-management/web-ui/): tour screenshots refreshed (notify/stack, program config).

### Notes
- Pair OSS `1.2.4` with matching commercial plugin packages (`super-plugins-1.2.4-…`).

---

## [1.2.3] - 2026-07-25

### Changed
- [Web UI](/docs/05-advanced-management/web-ui/): dashboard tour screenshots refreshed (overview, program detail/logs, inhibition rules).

### Notes
- Pair OSS `1.2.3` with matching commercial plugin packages (`super-plugins-1.2.3-…`).

---

## [1.2.2] - 2026-07-25

### Added
- Notification storm suppression docs and dashboard UX: Webhooks + Inhibition (When → Mute targets → For), Delivery Strategy defaults.
- OSS `/` notice page points to [Get Super Pro](https://super.docs.sconts.com/go/pro/).

### Changed
- Public docs homepage redesigned (wide layout) for all languages (en, zh-cn, ja, es, ru).
- [Get Super Pro](/go/pro/): license version coverage (issued line → free newer minors → renew) plus Terms alignment.
- Dashboard Notification Settings tabs renamed to **Webhooks** / **Inhibition rules**.

### Notes
- Pair OSS `1.2.2` with matching commercial plugin packages (`super-plugins-1.2.2-…`).

---

## [1.2.1] - 2026-07-14

### Added
- Docs CI: offline HTML link check (site-relative crawl) after Hugo build.
- Licensed-mode startup checks: deployments with a subscription key require the `security` plugin and `auth_secret` (hard-fail).

### Changed
- Public docs messaging aligned with a **single subscription plan**; documentation URLs use `https://super.docs.sconts.com/`.
- Removed public “pre-release / not for customer delivery” banners for the licensed plugin model.
- Docker Hub image CI publishes **`linux/amd64` only** (arm64 manifest removed from the publish workflow).
- Open-source edition license is **MIT** (historically GPL-3.0).

### Security

- Stricter defaults for network exposure and configuration validation in OSS deployments.
- Improved validation for user-supplied paths and outbound fetch URLs.
- Reduced sensitive data exposure in API and export responses.

### Notes
- Subscription plugin archives remain vendor-delivered (not built from this OSS repository). Pair OSS `1.2.1` with matching commercial plugin packages.

---

## [1.2.0] - 2026-07-10

> Runtime plugin architecture merged. Linux cgroup isolation QA (aarch64) signed off.

### Added
- **Runtime plugin host** — `superd` discovers `plugins/*.{so,dylib}`, verifies the signed license key, and dlopens authorized plugins.
- **HTTP plugin ABI** — generic `attach_http_plugins()` in OSS core; plugins register routes and auth middleware without linking `super-core`.
- **Lifecycle plugin ABI** — `on_event`, `after_stop`, metrics, and manager hooks via `ExtensionStack`.
- **`[license].key` in `conf/super.toml`** — replaces legacy `license.key` file; `SUPER_LICENSE` env override supported.
- **`auth_secret`** in `ServerConfig` (typed in OSS config; enforced when `security` plugin is loaded).
- **Unified CLI** — `login` / `token` subcommands in OSS `super` when `security` plugin is active.
- **`common::plugin_async`** — shared worker for cdylib async boundaries.

### Changed
- Commercial capabilities ship as **plugins + license**, not a separate `superd-premium` binary.
- **Cron scheduled tasks** remain in OSS `superd` (not plugin-gated).

### Notes
- Plugin libraries ship with subscription delivery; they are not built from this OSS repository.
- **Web dashboard** ships as an optional UI plugin with embedded static assets; OSS `superd` has no built-in web UI.
- Linux **cgroup isolation** signed off on aarch64 (2026-07-14).

---

## [1.1.9] - 2026-07-08

### Added
- GitHub Releases **multi-platform binaries** (Linux amd64/arm64, macOS Intel/ARM, FreeBSD) with README archives and `SHA256SUMS`.
- Docker image **multi-arch** publish (`linux/amd64`, `linux/arm64`).
- `gh-pages` branch README (auto-deployed with documentation).

### Changed
- Docker image: **Debian 13 (trixie)** build stages and **distroless `cc-debian13`** runtime.
- Release CI uses native `ubuntu-24.04-arm` for Linux ARM64 builds.

### Notes
- **Windows** pre-built binaries are not published; use Docker or build on Unix-like systems.

### Fixed
- FreeBSD release packaging (version passed into VM).
- CLI `check.rs` explicit `Vec<String>` types.

---

## [1.1.8] - 2026-07-07

### Added
- Official Docker image **`containerpi/super`** with default config under `dockerbuild/conf/`.
- GitHub Actions workflow to build and push the Docker image.
- Documentation homepage with OSS capabilities, licensed plugin features, and API example.

### Changed
- Docker image namespace from `schiplat/super` to `containerpi/super`.
- Installation docs, README, and `make docker` target for `dockerbuild/Dockerfile`.

### Fixed
- Dashboard `ProcessList.vue` syntax error breaking `vue-tsc` build.
- Doc screenshot paths for GitHub Pages (`/super/images/...`).

---

## [1.1.7] - 2026-07-07

### Added
- Event hooks — run scripts on [system events](/docs/03-orchestration/system-events).
- `post_stop` lifecycle hook.
- Supervisor-compatible fields: `stopsecs`, `priority`, log file paths, `autorestart` / `exitcodes` / `startsecs`.
- Historical logs API and `super logs --tail`.
- OTA updates via API and `super update --artifact-*`.
- System stats API and dashboard metrics panel.

### Changed
- OSS API no longer uses `auth_secret`; bind to `127.0.0.1` or use a firewall for exposure control.
- Documentation updates across config, API, and feature matrix.

### Fixed
- Historical logs API now reads from the correct log directory when `[storage]` is omitted.
