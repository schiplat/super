# Contributing to Project Super

Thank you for your interest in contributing to **Project Super** (OSS, MIT).

## Getting started

1. Fork [hzbd/super](https://github.com/hzbd/super) and clone your fork.
2. Install [Rust](https://rust.rust-lang.org/tools/install) (stable).
3. Build: `make build` (`superd` + `super` CLI).
4. Run tests: `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`.

Docs site (optional): `make docs-serve` or `cd docs && hugo server -D --disableFastRender` (requires [Hugo Extended](https://gohugo.io/installation/) **0.163.x** and the `hextra` submodule). Open **http://localhost:1313/** — do not open `docs/public/` directly; `hugo.yaml` `baseURL` is for production builds (CI overrides it on deploy). The `docs` job in `.github/workflows/ci.yml` builds the site on every PR (Hugo pinned to `0.163.3`).

## Pull requests

- Keep changes focused; one logical change per PR.
- Update docs when you change user-visible behaviour or config.
- Ensure CI passes (clippy + tests).
- Write clear commit messages (what and why).

## Scope

This repository is the **open-source core** (`superd`, `super`, MIT). Optional subscription capabilities (API auth, notifications, cgroup limits, dashboard UI) are loaded at **runtime** from signed plugin libraries — they are **not built from this repo** and are out of scope for most OSS PRs.

## OSS vs subscription runtime

Same `superd` binary; subscription unlocks features via config + plugin files:

```
super (this repo, MIT)              subscription delivery (separate)
────────────────────────            ─────────────────────────────────
superd ── verifies ──► [license].key   signed key from your vendor
       ── dlopen ───► plugins/*.so     official plugin libraries
       ── OSS API ──► process control   optional: auth, UI, notify, …
```

**In scope here:** process manager, REST/WS API, plugin host (verify + dlopen + ABIs), `[license]` **verification** only.

**Out of scope here:** plugin implementations, subscription key signing, plugin SKU catalogs, dashboard sources.

Licensed-plugin fields in config and API are documented with a 💎 marker; see the public [Feature matrix](https://super.docs.sconts.com/docs/07-editions/feature-matrix/) on the docs site.

## Public docs vs this repo

The Hugo site under `docs/content/docs/` is **customer-facing**. Do not add contributor runbooks, internal repository boundaries, signing workflows, or “what not to publish” checklists there — keep those in this file or in the private plugins repository.

When updating public docs:

- Describe OSS and licensed **runtime behaviour** only (what users configure and what `superd` does).
- Use vendor-neutral wording: “subscription delivery package”, “your vendor”, “authorized plugin libraries”.
- Do not link to or name private repositories, signing tools, or internal monorepo paths.
- Use the 💎 marker only for fields that require a verified subscription at runtime.

Before opening a PR that touches docs or license-related code, confirm the change does not require documenting private trees or issuance policy internals.

## Verifying keys (OSS-friendly)

| Path | Keyring source |
|------|----------------|
| `make build` / PR CI | Committed `common/keys/*.public.key` (offline; no Manager) |
| `make fetch-keys` | Optional maintainer sync from Manager → write keys (then commit) |
| GitHub **Release** (`v*` tag) | Fetches Manager keyring in CI, then builds binaries |

Public verifying keys are not secrets — keep a committed copy so anyone can build. Official release artifacts always embed the live Manager ring (`MANAGER_BASE`, `MANAGER_PATH_PREFIX`, `MANAGER_TOKEN`, `PRODUCT_ID` secrets on `hzbd/super`; **no script defaults**).

After rotate, also commit refreshed keys so self-built OSS binaries stay compatible:

```bash
# All four required — set via env or gitignored .env (no script defaults)
export MANAGER_BASE=… MANAGER_PATH_PREFIX=… MANAGER_TOKEN=… PRODUCT_ID=…
make fetch-keys
git add common/keys/*.public.key && git commit -m "Update verifying keyring"
```

## Questions

Open a [GitHub Discussion](https://github.com/hzbd/super/discussions) or an issue for design questions before large refactors.
