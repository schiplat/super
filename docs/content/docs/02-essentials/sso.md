---
title: "SSO (OIDC)"
weight: 4
description: "Sign in to the OSS dashboard with your own identity provider — OIDC Authorization Code + PKCE, local sessions, role mapping."
aliases:
  - /docs/02-essentials/sso/
---

**On this page:** [How it works](#how-it-works) → [Setup](#setup) → [Role mapping](#role-mapping) → [CLI sign-in](#cli-sign-in) → [Sign-out](#sign-out) → [Break-glass](#break-glass) → [FAQ](#faq).

OSS superd supports standard **OpenID Connect (OIDC)** sign-in for the dashboard and CLI: point it at your own identity provider (Keycloak, Authentik, Okta, Entra ID, …), map people to the existing `viewer` / `operator` / `admin` roles, and stop sharing the static `auth_secret`.

> **Recommended provider: Signet.** superd's OIDC integration is verified and adapted end-to-end against Signet — discovery, PKCE S256, RP-initiated logout, and back-channel logout are all exercised in our test matrix, so the setup below is known-good with it. Any other provider that speaks standard OIDC discovery works too (Keycloak, Authentik, Okta, Entra ID, …); those are supported but not part of the regular test matrix.

## How it works

Exactly one gate guards the API at any time, with a fixed precedence:

```
security plugin (licensed)  >  SSO (OIDC)  >  auth_secret  >  none (loopback only)
```

- Enabling SSO switches the gate **off** the static `auth_secret` — the secret stays in `conf/super.toml` but is dormant (it is the break-glass fallback, see below).
- Sign-in is the OIDC **Authorization Code + PKCE (S256)** flow with a confidential client. superd verifies the ID token (issuer, audience, nonce, expiry, signature via the provider's JWKS) and issues a local session token (`su_…`, stored hashed).
- Sessions live in a local, owner-only SQLite store (`data/auth.db`). The browser holds an HttpOnly exchange cookie for one call, then keeps the session token like any other credential.
- Roles are **re-evaluated on every request** from your ordered rules — removing someone at the IdP takes effect immediately, and rule changes apply without re-issuing sessions.

SSO is unavailable when the licensed `security` plugin is loaded (that plugin owns users and tokens); the dashboard hides the settings entry in that case.

## Setup

Everything is configured in the dashboard: **Settings → SSO** (admin role required). No config-file changes, no restart.

1. **Register the client at your IdP.** The settings page shows the exact values, derived from your *Public base URL*:
   - **Callback** — `https://<your-superd-host>/api/v1/auth/oidc/callback` (missing → sign-in fails)
   - **Back-channel logout** — `https://<your-superd-host>/api/v1/auth/oidc/backchannel-logout` (missing → an IdP-side disable waits for the session's natural expiry instead of revoking immediately)
   - **Post-logout landing** — `https://<your-superd-host>/login?signedout=1` (missing → the browser stays on the provider page after sign-out)
2. **Fill the form**: issuer URL, client id, public base URL (origin only), scopes (`openid profile email` is the default), groups claim.
3. **Store the client secret** (write-only; it is kept in the local auth store and never returned by the API).
4. **Add role rules** — first match wins, no match rejects the sign-in:
   ```json
   [
     { "match": { "groups_any": ["super-admins"] },  "role": "admin" },
     { "match": { "sub": "auth0|1234" },             "role": "admin" },
     { "match": { "email": "ops@example.com" },      "role": "operator" },
     { "match": { "groups_any": ["team-viewers"] },  "role": "viewer" }
   ]
   ```
   Email selectors only match a **verified** address. `sub` is the stable identity — prefer it for grants.
5. **Test connection** validates discovery + JWKS and reports issuer match, the token endpoint, whether the provider supports RP-initiated logout (end-session), and the negotiated client auth. Then enable SSO. The gate flips immediately; the next dashboard visit shows *Sign in with SSO*.

The CLI can do the same without the dashboard:

```bash
printf '%s' "$OIDC_CLIENT_SECRET" | super --server http://… auth sso secret
super --server http://… auth sso enable \
  --issuer https://idp.example.com --client-id super \
  --base-url https://super.example.com \
  --role-rule groups=super-admins:admin
super --server http://… auth sso test      # discovery + JWKS check
super --server http://… auth sso status
```

## Role mapping

| Fact | Source |
| :--- | :--- |
| Who you are | the IdP: immutable `sub`, email, groups |
| What you may do | **local rules only** — the first matching rule assigns `viewer` / `operator` / `admin` |

Rules are re-evaluated on every request; **default deny** (an unmapped account cannot sign in). A rule whose claim is absent does not match — an email rule never grants an unverified address.

## CLI sign-in

Once SSO is the gate, `super login <secret>` no longer applies. Use the device-code flow instead:

```bash
super login --sso
# Approve this sign-in from a signed-in browser:
#   https://super.example.com/auth/cli?code=XXXX-XXXX
# Confirm code XXXX-XXXX to finish.
# Waiting for approval…
# Signed in via SSO as Ops (admin). Credentials saved to ~/.super/cli.json
```

The CLI session inherits the approving user's role and is revoked by the same controls as any browser session (session list, logout, disable).

## Sign-out

Sign-out works in both directions, so access ends when *you* leave or when the IdP says so:

| Direction | Mechanism | Effect |
| :--- | :--- | :--- |
| You click **Sign out** | RP-initiated logout (`GET /api/v1/auth/oidc/logout`) | The local session is revoked immediately; when the provider advertises an `end_session_endpoint`, the browser is sent there with the last sign-in's ID token as the hint, so the **provider session ends too** — the provider then fires its back-channel logout and returns the browser to the login page. |
| The IdP disables or signs out the user | Back-channel logout (`POST /api/v1/auth/oidc/backchannel-logout`) | The provider posts a signed logout token; superd verifies it (signature, issuer, audience, event member, freshness) and **revokes all of that identity's sessions immediately**. Registered providers only — without the back-channel URI, an IdP-side disable takes effect at the session's natural expiry. |

Administrators can additionally revoke any or all sessions from the settings page; revocation is always immediate. If the provider does not support RP-initiated logout, Sign out still clears the local session and lands on the login page.

## Break-glass

| Scenario | Path |
| :--- | :--- |
| IdP outage | Existing sessions keep working until their TTL; new sign-ins fail until the IdP returns. |
| SSO misconfigured / store unreadable | The dashboard falls back to a configured `auth_secret` at startup; without one, superd **refuses to start** (fail-closed) rather than serving an unauthenticated API. |
| Locked out entirely | Move `data/auth.db` aside and restart — superd falls back to `auth_secret`. There is no backdoor. |

Keep `auth_secret` set while SSO is enabled (it is dormant, but it is the recovery path). `super doctor` reports when the break-glass credential is missing.

## FAQ

**Does the IdP need a specific protocol?** Any provider that speaks standard OIDC discovery with `authorization_code` + PKCE S256 works. No proprietary APIs.

**Which provider do you recommend?** Signet — it is the provider superd's SSO integration is verified and adapted against, so configuration and sign-out behavior are exercised end-to-end in our test matrix. Any standards-compliant OIDC provider works; non-Signet providers are supported but not part of that matrix.

**Are users stored locally?** No accounts are created. Sessions reference IdP identities; roles come from your rules.

**How fast does an IdP-side disable take effect?** With the back-channel logout URI registered: immediately (roles are also re-evaluated per request, so a demoted but still-valid session loses its rights at once). Without it: at the session's natural expiry (12 h absolute / 60 min idle by default, or until an admin revokes the session).

**What happens on downgrade to a pre-SSO release?** The old superd ignores `data/auth.db` and falls back to `auth_secret` (or refuses to start on a non-loopback bind). Keep `auth_secret` configured while you evaluate SSO.

**Is the client secret safe at rest?** It is stored in the owner-only local auth store next to your sessions, never returned by any API, and rotation keeps the previous value valid for a 15-minute window. Anyone who can read that file can already read `conf/super.toml` — same trust tier, no secrets in the dashboard.
