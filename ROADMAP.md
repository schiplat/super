# Project Super — Roadmap / Backlog

Open, OSS-level planning. Items here are **not** GA blockers — they are prioritized
improvements we track publicly. Anything that is an accepted security trade-off
lives in [SECURITY.md](SECURITY.md); this file is for feature work.

Priorities: **P0** (next in line) · **P1** (soon) · **P2** (backlog).

## P0 — Migration importers (`super import`)

**Status:** agreed, not yet implemented.

Super's docs have dedicated PM2 / supervisor migration pages, but there is no
tooling to actually move a config over. Adding a CLI importer lowers the
migration barrier and is a strong onboarding hook for users coming from other
process managers.

Planned scope (MVP):

- `super import supervisor supervisord.conf` — INI `[program:x]` → `ProgramConfig`
  fields map almost 1:1 (`command` / `directory` / `user` / `environment` /
  `autostart` / `autorestart` / `startsecs` / `stopsecs` / `numprocs` / …).
- `super import pm2 ecosystem.config.js` — PM2's ecosystem is a **JS file**, not
  JSON. MVP parses only **literal** `module.exports = { apps: [...] }` objects
  (strip `module.exports =`, tolerate single quotes / trailing commas / unquoted
  keys via a JSON5-style preprocessing). `require()` / dynamic logic → clear error
  suggesting `pm2 save` or manual migration. No embedded JS engine.
- Reuse the existing stack API (`POST /api/v1/stack`, cf. `super apply`) for the
  actual write, and the batch-confirmation pattern (`--yes` / `--dry-run`) to
  preview the program list before applying.
- Docs: update the migration pages with the import workflow.

Implementation notes: lives in the OSS CLI as a normal subcommand (no plugin/ABI
involvement — it is pure config translation, so it must stay OSS and free).
Rough estimate 0.5–1.5 dev-days for the CLI + literal parser, 2–3 with tests and
docs. The PM2 JS-literal preprocessing is the main cost driver; do not escalate
to a full JS engine.

## P2 — Credential-channel env injection (opt-in)

**Status:** evaluated, deliberately deferred. See
[SECURITY.md](SECURITY.md#known-limitation-secrets-in-child-procpidenviron) for
the accepted limitation and rationale. If implemented: per-program opt-in
(`secrets = "fd" | "dir" | "env"`), Linux memfd/fd or a credentials directory,
matching the existing key-based masking heuristics plus an explicit
`sensitive_env` list, and the same handling for hooks.
