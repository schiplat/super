---
title: "Authentication"
weight: 1
description: "Securing the Daemon with Access Tokens."
---

## OSS built-in auth (single admin secret)

OSS `superd` can require a Bearer secret without any plugin:

| Bind | Default | How to enable auth |
| :--- | :--- | :--- |
| Loopback (`127.0.0.1` / `::1`) | **Open** (scripts / local CLI keep working) | Set `[server].auth_required = true` |
| Non-loopback | **Auth required** | Automatic — core generates or loads a secret |
| Unix socket only (`socket_only`) | Open (filesystem mode bits) | Set `[server].auth_required = true` if desired |

When core auth activates and `auth_secret` is **not** set in `conf/super.toml`, `superd` creates `$SUPER_ROOT/data/auth.key` (32 CSPRNG bytes, hex, mode `0600`) if missing, prints the plaintext **once** on first generation, and on later boots only logs the file path.

```toml
[server]
auth_required = true   # force login even on loopback
```

Optional override (skips `data/auth.key`):

```toml
auth_secret = "your-own-long-random-string"
```

Use the secret as `Authorization: Bearer <secret>` or paste it into the dashboard login page. `data/auth.key` is the OSS admin secret; licensed multi-user tokens still live in `data/auth.json` (security plugin).

`[server].allow_insecure_public_bind` is **deprecated** — non-loopback binds now require authentication (core secret or security plugin) instead of an insecure opt-in.

---

> [!IMPORTANT] Licensed feature — `security` plugin
> The sections below cover **multi-user Access Tokens**, RBAC, and audit — provided by the **`security` plugin** (included with every subscription and **required for licensed startup**). It needs a valid `[license].key`, the plugin library in `$SUPER_ROOT/plugins/`, and `auth_secret`. When the plugin is loaded it **replaces** the OSS core auth gate (one middleware, not two).

The **default OSS loopback deployment has no API authentication**. Non-loopback binds activate core auth automatically (see above). Multi-user tokens require the licensed **`security` plugin**.

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
| Licensed intent (plugins under `plugins/`, `auth_secret` set, or non-loopback bind) | **Refuse startup** — avoids silent loss of API auth or licensed features |
| `[license].strict = true` or `SUPER_LICENSE_STRICT=1` | **Refuse startup** always |

The `SUPER_LICENSE` / `SUPER_LICENSE_STRICT` env overrides are documented in [Environment Variables](/docs/06-internals/environment-variables#license-licensed-deployments).

Production subscription templates ship with `strict = true`. Fix the key, renew, or remove licensed-only configuration to run in OSS mode.

| Mode | API auth | Startup if `security` missing |
| :--- | :--- | :--- |
| OSS (loopback, default) | Open; optional via `auth_required` | N/A |
| OSS (non-loopback / `auth_required`) | Core random/`auth_secret` | N/A |
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
| Expired or version out of range | Policy or Super version span | Renew or upgrade per your subscription terms — see [Get Super Pro](/go/pro/) |

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
2. The Dashboard prompts for an **Access Token** when `auth_required` is injected.

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
> Without core auth and without the security plugin, OSS `superd` has no `/api/v1/auth/*` routes. Enable `[server].auth_required` (or bind non-loopback) for a single admin secret, or load the security plugin for multi-user tokens.

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
