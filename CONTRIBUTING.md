# Contributing to Project Super

Thank you for your interest in contributing to **Project Super** (OSS, MIT).

## Getting started

1. Fork [schiplat/super](https://github.com/schiplat/super) and clone your fork.
2. Install [Rust](https://rust.rust-lang.org/tools/install) **1.85 or newer** (stable; required for edition 2024).
3. Build: `make build` (`superd` + `super` CLI).
4. Run tests: `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`.

Docs site (optional): `make docs-serve` or `cd docs && hugo server -D --disableFastRender` (requires [Hugo Extended](https://gohugo.io/installation/) **0.163.x** and the `hextra` submodule). Open **http://localhost:1313/** — do not open `docs/public/` directly; `hugo.yaml` `baseURL` is for production builds (CI overrides it on deploy). The `docs` job in `.github/workflows/ci.yml` builds the site on every PR (Hugo pinned to `0.163.3`).

## Pull requests

- Keep changes focused; one logical change per PR.
- Update docs when you change user-visible behaviour or config.
- Ensure CI passes (clippy + tests).
- Write clear commit messages (what and why).

## Scope

This repository is the **open-source core** (`superd`, `super`, embedded Dashboard shell, MIT). Optional subscription capabilities (multi-user tokens / RBAC, notifications, cgroup limits, Dashboard Pro UI extensions) are loaded at **runtime** from signed plugin libraries — plugin **implementations** are out of scope for most OSS PRs.

## OSS vs subscription runtime

Same `superd` binary; subscription unlocks features via config + plugin files:

```
super (this repo, MIT)              subscription delivery (separate)
────────────────────────            ─────────────────────────────────
superd ── verifies ──► [license].key   signed key from your vendor
       ── dlopen ───► plugins/*.so     official plugin libraries
       ── OSS API ──► process control   optional: auth, Pro UI, notify, …
       ── Dashboard ─► embedded shell   Pro pages via ui plugin slots
```

**In scope here:** process manager, REST/WS API, embedded Dashboard shell, plugin host (verify + dlopen + ABIs), `[license]` **verification** only.

**Out of scope here:** plugin implementations, subscription key signing, plugin SKU catalogs.

### Dashboard slot contract (do not change casually)

The shell exposes named UI slots (`process.actions`, `process.detail.tabs`, `nav.*`) and `context.process` fields (`id`, `name`, `status`, `pid`, …) consumed by subscription UI plugins. **Do not rename slots, drop context fields, or rewrite `:context="{ … }"` at mount sites unless you know exactly what you are doing** — that breaks paid plugins without failing this repo’s TypeScript build. CI runs `cd dashboard && npm test` (see `dashboard/tests/slots.spec.ts` and `ui-bridge.d.ts`). Extending with new optional fields / new slot names is fine; removals need maintainer review.

Licensed-plugin fields in config and API are documented with a 💎 marker; see the public [Feature matrix](https://super.docs.sconts.com/docs/07-editions/feature-matrix/) on the docs site.

## Public docs vs this repo

The Hugo site under `docs/content/docs/` is **customer-facing**. Do not add contributor runbooks, internal repository boundaries, signing workflows, or “what not to publish” checklists there — keep those in this file or in the private plugins repository.

When updating public docs:

- Describe OSS and licensed **runtime behaviour** only (what users configure and what `superd` does).
- Use vendor-neutral wording: “subscription delivery package”, “your vendor”, “authorized plugin libraries”.
- Do not link to or name private repositories, signing tools, or internal monorepo paths.
- Use the 💎 marker only for fields that require a verified subscription at runtime.

Before opening a PR that touches docs or license-related code, confirm the change does not require documenting private trees or issuance policy internals.

## License trust anchor (OSS-friendly)

`common/root.public.key` is the **root Ed25519 public key** embedded at compile time. Every license carries a signing-key certificate signed by that root, so rotating a signing key does **not** require an OSS rebuild. The root key is public material; the corresponding private key is held offline by the maintainers and never enters this repository.

- Ordinary `make build` / PR CI work offline with the committed root key.
- Release builds need no Manager credentials.
- Do not commit private keys, certificates, or issued licenses.

## Questions

Open a [GitHub Discussion](https://github.com/schiplat/super/discussions) or an issue for design questions before large refactors.
