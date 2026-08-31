---
title: "Authentication"
weight: 1
description: "Securing the Daemon with Access Tokens."
---

The **default OSS deployment has no API authentication**. By default, `superd` binds to loopback and **refuses to start** on a non-loopback address unless you explicitly set `allow_insecure_public_bind = true` in `[server]` or load the optional **`security` plugin** for token-based auth.

> [!TIP] Free 1-month beta trial
> Super Pro plugins are available during the beta with a **free 1-month trial license** ([request via GitHub Issue](https://github.com/schiplat/super/issues/new?template=pro-trial.yml)). We recommend licensed deployments for staging and non-critical workloads today; see the [feature matrix](/docs/07-editions/feature-matrix/) and the [Toward GA checklist](https://github.com/schiplat/super#toward-ga) on GitHub.

OSS deployments without a valid `[license].key` have no API auth; public bind requires explicit opt-in via `allow_insecure_public_bind` as described above.

## Licensed deployments require `security`

**Every subscription includes the `security` plugin at no extra charge.** If `[license].key` verifies successfully, `superd` **refuses to start** unless:

1. **`security` is listed in the signed license claims** (re-issue legacy keys that omit it).
2. **`security.so` / `security.dylib` loads successfully** from `$SUPER_ROOT/plugins/`.
3. **`auth_secret` is set** in `conf/super.toml` (root Admin Bearer for bootstrap).
4. **HTTP auth middleware is active** (the security plugin exports `authenticate`).

Other licensed plugins (`ui`, `notify`, `isolation`, …) load only after these checks pass. OSS deployments (no valid license) are unchanged.

### Invalid or incompatible license key

If `[license].key` is set but verification fails (bad signature, expired with `retain_grants_after_expiry = false`, or superd version outside the signed range), `superd` does **not** treat the deployment as licensed:

| Signal | Behavior |
| :--- | :--- |
| Dev-style OSS (loopback, no plugins, no `auth_secret`) | **Degrade** — run OSS without plugins; stderr banner + `super check` / `super doctor` warnings |
| Licensed intent (plugins under `plugins/`, `auth_secret` set, or non-loopback bind) | **Refuse startup** — avoids silent loss of API auth or Pro features |
| `[license].strict = true` or `SUPER_LICENSE_STRICT=1` | **Refuse startup** always |

Production subscription templates ship with `strict = true`. Fix the key, renew, or remove licensed-only configuration to run in OSS mode.

| Mode | API auth | Startup if `security` missing |
| :--- | :--- | :--- |
| OSS | ❌ Open (loopback-first) | N/A — runs without plugins |
| **Licensed** | ✅ Required (via `security`) | **Hard fail** |
| **Invalid key + licensed intent / strict** | — | **Hard fail** (no OSS fallback) |

> [!CAUTION]
> **Legacy keys** without `security` in claims must be re-issued. **Partial installs** (license OK, `ui.so` present, `security.so` missing) also fail fast with an actionable error.

#### Troubleshooting license verification

When startup, `super check`, or `super doctor` reports a bad or incompatible license, try these steps locally (no daemon required for `check` / `keyring`):

1. **`super check`** — re-validates `conf/super.toml`, including the license string and licensed-mode requirements.
2. **`super doctor`** — runs the same config check, then probes a running daemon; prints a **Verifying keys** line (embedded signing key ids in this CLI binary).
3. **`super keyring`** — lists every verifying key id (`kid`) compiled into this build; use `--json` for scripts.

Typical messages and what to do:

| Symptom | Likely cause | What to try |
| :--- | :--- | :--- |
| Missing signing key id (`kid`) | License predates the current format | Ask your vendor to **re-issue** the license |
| Unknown / unrecognized `kid` | License signed with a key this `superd` build does not embed yet (common after key rotation) | Run `super keyring` on the **same** `super` / `superd` version you deploy; upgrade to an official release that includes that `kid`, or keep your previous license file until you upgrade |
| Signature mismatch for a listed `kid` | Wrong, truncated, or tampered key string | Restore the exact key from your vendor portal; avoid editing `[license].key` |
| Expired or version out of range | Policy or Super version span | Renew or upgrade per your subscription terms — see [feature matrix](/docs/07-editions/feature-matrix/) |

Official release binaries may embed **more** verifying keys than a local `cargo build` from git alone. Compare against the release you actually run in production, not only a dev build.

## Enabling Authentication (Subscription)

1. Add a valid `[license].key` in `conf/super.toml` (must authorize `security` — included with every subscription).
2. Install **`security.so`** from your subscription delivery package into `$SUPER_ROOT/plugins/` (required for startup).
3. Set `auth_secret` in `super.toml` (required for startup):

```toml
# super.toml (subscription)
auth_secret = "my-super-secure-root-password"
```

Once the `security` plugin is active:

1. All API requests require an `Authorization: Bearer <token>` header (except `/health`, `/metrics`, and docs whitelist).
2. The Web UI prompts for an **Access Token** when `auth_required` is injected.

## Bootstrap with `auth_secret`

Sign in with config `auth_secret` (Dashboard or `super login`), then create Access Tokens. Creating a token does **not** end the current root session:

```bash
curl -X POST http://127.0.0.1:9002/api/v1/auth/tokens \
  -H "Authorization: Bearer my-super-secure-root-password" \
  -H "Content-Type: application/json" \
  -d '{"name":"ci-bot","role":"operator"}'
```

By default **`auth_secret` stays usable** even after tokens exist (with a Dashboard warning). Prefer generated `sk-...` tokens for day-to-day access.

### Optional: disable `auth_secret`

An **Admin** (including a root session still using `auth_secret`) can explicitly disable config `auth_secret` after **at least one Admin Access Token** exists:

- Dashboard → Access Tokens → **Disable auth_secret**
- Or `POST /api/v1/auth/secret/disable`

State is persisted in `$SUPER_ROOT/data/auth_settings.json`. While disabled, Bearer/`auth_secret` login is rejected.

**Recovery:** revoke **all Admin** Access Tokens — `auth_secret` is re-enabled automatically. Startup still requires `auth_secret` to be set in `super.toml`.

> [!WARNING]
> Without the security plugin: OSS `superd` has no `/api/v1/auth/*` routes. `super login` will fail with 404 until the plugin is loaded.

## Managing Tokens (HTTP API)

### Login / logout / status

```bash
curl -X POST http://127.0.0.1:9002/api/v1/auth/login \
  -H "Authorization: Bearer <token-or-auth_secret>"

curl -X POST http://127.0.0.1:9002/api/v1/auth/logout \
  -H "Authorization: Bearer <token-or-auth_secret>"

curl -H "Authorization: Bearer <token>" http://127.0.0.1:9002/api/v1/auth/status
```

### List Tokens

Admins see all tokens. Viewer/Operator see only their own token metadata (no secret).

```bash
curl -H "Authorization: Bearer <token>" http://127.0.0.1:9002/api/v1/auth/tokens
```

### Renew (rotate) a Token

Same id/name/role; old secret is invalidated immediately. Non-admins may renew only their own token.

```bash
curl -X POST -H "Authorization: Bearer <token>" \
  http://127.0.0.1:9002/api/v1/auth/tokens/<id>/renew
```

### Revoke a Token

Admin only.

```bash
curl -X DELETE -H "Authorization: Bearer <admin-token>" \
  http://127.0.0.1:9002/api/v1/auth/tokens/<id>
```

## Roles

| Role | Permissions |
|------|-------------|
| **Viewer** | Read-only (list, info, logs, stack/notify with secrets redacted). Own token list + renew. |
| **Operator** | Create programs; manage notification channels; start/stop/restart/signal; read stack redacted; own token list + renew. |
| **Admin** | Full access including token management, plaintext config, and disabling `auth_secret`. |
