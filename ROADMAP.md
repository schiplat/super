# Project Super — Roadmap / Backlog

Open, OSS-level planning. Items here are **not** GA blockers — they are prioritized
improvements we track publicly. Anything that is an accepted security trade-off
lives in [SECURITY.md](SECURITY.md); this file is for feature work.

This is the **single** public roadmap source. Docs and the changelog link here
rather than duplicating a second page. **Shipped work** belongs in the
[Changelog](https://super.docs.sconts.com/docs/08-changelog/) (e.g. OSS Dashboard
shell in **1.5.7**) — not as lingering P0 entries here.

Priorities: **P0** (next in line) · **P1** (soon) · **P2** (backlog) · **Directions** (multi-release horizons).

## P0 — Migration importers (`super import`) — supervisor **done**

**Status:** supervisor importer **shipped** (`super import supervisor`). A
JS-ecosystem converter is **deferred** (large semantic gaps vs fork-only
configs; manual mapping on the migration page is enough for now). Track
details in the internal product plan.

Super's docs have dedicated migration pages. The Supervisor CLI importer
lowers the migration barrier for the high-fidelity path; other formats stay
manual until there is clear demand for a narrow, warning-heavy draft tool.

### Step 1 — Extract a stack-format parser layer — DONE (adjusted shape)

The TOML/JSON stack parsing remains funneled through
`common::parse_stack_from_str` (`common/src/program_validate.rs`), used by
`super apply`, `super check`, `[include].files`, and the raw-body API path —
**unchanged**, with all existing tests green. Import formats live in a separate
registry module, `common/src/import/`, instead of rewriting the built-in
dispatcher:

```rust
pub trait StackFormat: Send + Sync {
    fn id(&self) -> &'static str; // "supervisor" today; other formats later
    fn detect(&self, content: &str) -> bool;
    fn parse(&self, content: &str, ctx: &ParseCtx) -> anyhow::Result<StackDraft>;
}
```

- `StackDraft` carries the mapped services plus a structured warning list —
  every concept a format cannot express is reported, never silently dropped.
- Semantic validation and any manager dependency stay out of the parser layer
  (`common` remains tokio-free); `apply`/`import` share the same stack API.
- Formats carry a per-format policy for foreign concepts they cannot express
  (e.g. supervisor `[include]`/`[group]`, PM2 `instances`): ignore-with-warning
  or hard error, documented per format.
- Stays inside `common` as a module; a separate crate is only warranted if the
  parser is ever published for external toolchains.

### Step 2 — Import subcommands as `StackFormat` implementations

Shipped / deferred:

- ✅ `super import supervisor supervisord.conf` — INI `[program:x]` → stack
  services, mapping almost 1:1 (`command` / `directory` / `user` /
  `environment` with `%(ENV_…)`/`%(here)s` expansion / `autostart` /
  `autorestart` / `exitcodes` / `startsecs` / `stopwaitsecs` / `priority` /
  `startretries`→`retry_limit` / `numprocs` + `%(process_num)02d`→`{num}` /
  `redirect_stderr`). `[include] files=` recursion (glob + cycle detection),
  `[group:x]` membership → `group` field. Name collisions:
  `--on-collision skip|rename|override` (default skip; rename is the
  side-by-side path; override needs typed confirmation — `--yes` alone never
  escalates); imports never prune; `--no-start` imports everything stopped;
  `--dry-run` and `--emit-toml <file|-|>` work without a daemon. Programs are
  stamped `source = import:supervisor` (detail views only).
- ⏸ JS ecosystem converter (`ecosystem.config.js` style) — **deferred**.
  Semantic gaps (file watch, cluster / shared-listen, deploy blocks, dynamic
  `require()`) make a half-done importer harmful. Migrate manually with the
  mapping table on the migration docs page until demand justifies a
  **narrow fork-mode draft + strong warnings** tool (not a full migrator).
- Write path reuses the existing stack API (`PUT /api/v1/stack`, same as
  `super apply`) with the batch-confirmation pattern (`--yes` / global
  `--dry-run`).
- Docs: migration pages and the CLI reference document the supervisor
  workflow (`/docs/04-production-scenarios/migrations/vs-supervisor/`,
  `#import`); the other migration page states the converter is deferred and
  keeps a manual mapping table.

Implementation notes: lives in the OSS CLI as a normal subcommand (no plugin/ABI
involvement — it is pure config translation, so it must stay OSS and free).

## P1 — AI edge gateway & container scenarios

**Status:** direction agreed; design and sequencing open.

Actively adapt Super for **AI edge gateway** and **container** deployments —
single static binary, loopback-first defaults, declarative stacks, and lean
daemon RSS on small / intermittently connected hosts.

Themes (non-exhaustive): gateway/sidecar/edge defaults; container recipes
(foreground PID 1, `SUPER_ROOT` layout, health probes, fail-closed bind);
lifecycle fit for AI-adjacent edge workloads without a cluster control plane on
the device; and AI-agent infrastructure — agents autonomously spawning helper
processes, scheduling scripts via cron jobs, and querying process state through
the REST/WebSocket API and event ledger, with the plugin system as the
extension surface for agent-side tooling. OSS core stays standalone;
subscription plugins remain optional.

## Directions — Cloud control hub (SaaS / Hub)

**Status:** long-horizon product direction; not a near-term GA commitment.

Evolve beyond **single-host subscription plugins** toward a **cloud-hosted
control hub** (SaaS / Hub): multi-node inventory, optional host **connector**,
central access lifecycle, and a shared operator portal — while each host still
runs a lean local `superd`.

Single-host subscription plugins remain a separate tier from hub/enterprise
multi-node features: **`security` is required** for licensed startup; **`notify`**,
**`isolation`**, and **`ui`** are optional grants (`ui` adds Pro Dashboard
extensions — Tokens, Notification Settings, hot-reload — on top of the OSS
shell). Hub UI stays a later direction; the OSS single-host Dashboard already
ships without any of these plugins.

Principles: the local daemon stays authoritative for process lifecycle; the hub
coordinates and delivers policy/artifacts; OSS remains useful offline; public
docs stay at product level (no signing-key / issuance internals). Packaging
(hosted SaaS vs self-hosted Hub) and timeline are TBD.

## P2 — Browser Operation Audit page

**Status:** deferred; no near-term schedule.

Licensed `security` already writes an operation audit log on disk. There is no
Dashboard page to browse it. If built later, it stays behind the `security`
capability (OSS must not show an empty stub). Until then, operators use the log
files / CLI.

## P2 — Credential-channel env injection (opt-in)

**Status:** evaluated, deliberately deferred. See
[SECURITY.md](SECURITY.md#known-limitation-secrets-in-child-procpidenviron) for
the accepted limitation and rationale. If implemented: per-program opt-in
(`secrets = "fd" | "dir" | "env"`), Linux memfd/fd or a credentials directory,
matching the existing key-based masking heuristics plus an explicit
`sensitive_env` list, and the same handling for hooks.

## P2 — Extension trait: wire or remove `before_stop`

**Status:** documented as reserved; code decision pending.

`Extension::before_stop` is declared in the trait (and forwarded by
`ExtensionStack`), but the host never invokes it — `stop_program` runs only the
per-program `pre_stop` lifecycle hook, and the plugin C-ABI vtable has no
`before_stop` slot. Docs on both extension pages tell users it is reserved.
Pick one:

- **Wire it** — call it in `stop_program` after the stop request is accepted and
  before the stop signal (mirroring where the `pre_stop` hook runs), log-only
  error handling; expose it in the plugin ABI in a future `PLUGIN_API_VERSION`
  bump. Migration value: parity with the start-side hooks for compiled-in
  embedders.
- **Remove it** — delete the trait method (breaking change for the
  `Extension` trait; gate behind a minor version note). Keeps the surface
  honest; `on_event` + `pre_stop` hooks already cover the use cases.

Either way, update the two extension docs to drop the "reserved" caveat.

## P2 — Health probe `delay_secs` (defer first probe)

**Status:** specified, deferred until there is real demand.

A per-probe `delay_secs` knob (default `0` = today's behavior: probes begin
immediately) that waits N seconds after process start before the first probe.
Unlike `start_period_secs` — a grace window whose failures don't count while
probes still fire — `delay_secs` avoids firing probes at all during a known
startup phase. Useful when probes are expensive or noisy: heavyweight `exec`
checks (DB validation queries), HTTP endpoints that log errors while the app
boots, TCP probes that burn a full `timeout_secs` against a not-yet-listening
port. `start_period_secs` remains the tool for *unknown* startup duration
(failure shield); `delay_secs` targets *known* startup duration (probe
suppression). Semantics: counted from process start; before the first probe no
failures exist, so grace is irrelevant; a crash during the delay is handled by
normal exit handling (unrelated to the health task). Implementation is a single
sleep before the probe loop plus one serde field per probe variant with a `0`
default — fully backward compatible.

