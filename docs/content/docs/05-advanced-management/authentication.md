---
title: "Authentication"
weight: 1
description: "Securing the Daemon with Access Tokens."
---

The **default OSS deployment has no API authentication**. By default, `superd` binds to loopback and **refuses to start** on a non-loopback address unless you explicitly set `allow_insecure_public_bind = true` in `[server]` or load the optional **`security` plugin** for token-based auth.

> **Public beta:** Super Pro plugins are available during the beta with a **free 1-year license** ([request via GitHub Issue](https://github.com/hzbd/super/issues/new?template=pro-trial.yml)). We recommend licensed deployments for staging and non-critical workloads today; see the [feature matrix](/docs/07-editions/feature-matrix/) and the [Toward GA checklist](https://github.com/hzbd/super#toward-ga) on GitHub.

OSS deployments without a valid `[license].key` have no API auth; public bind requires explicit opt-in via `allow_insecure_public_bind` as described above.

## Licensed deployments require `security`

**Every subscription includes the `security` plugin at no extra charge.** If `[license].key` verifies successfully, `superd` **refuses to start** unless:

1. **`security` is listed in the signed license claims** (re-issue legacy keys that omit it).
2. **`security.so` / `security.dylib` loads successfully** from `$SUPER_ROOT/plugins/`.
3. **`auth_secret` is set** in `conf/super.toml` (root Admin Bearer for bootstrap).
4. **HTTP auth middleware is active** (the security plugin exports `authenticate`).

Other licensed plugins (`ui`, `notify`, `isolation`, …) load only after these checks pass. OSS deployments (no valid license) are unchanged.

| Mode | API auth | Startup if `security` missing |
| :--- | :--- | :--- |
| OSS | ❌ Open (loopback-first) | N/A — runs without plugins |
| **Licensed** | ✅ Required (via `security`) | **Hard fail** |

> **Legacy keys** without `security` in claims must be re-issued. **Partial installs** (license OK, `ui.so` present, `security.so` missing) also fail fast with an actionable error.

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

> **Without the security plugin**: OSS `superd` has no `/api/v1/auth/*` routes. `super login` will fail with 404 until the plugin is loaded.

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
