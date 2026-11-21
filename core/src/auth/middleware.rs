//! Axum middleware: require the active gate's credential and enforce RBAC.

use std::sync::{Arc, RwLock};

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use common::{AuthStatusResponse, IdentityClaims, SsoStatus, UserContext, UserRole, resolve_role};

use super::policy;
use super::sso::{SsoRuntime, pkce};

/// Shared state for the OSS auth middleware, `/auth/*` handlers, and the SSO
/// routes. The gate is swappable so enabling/disabling SSO in the Web UI (or
/// an external `super auth sso disable --offline`) needs no restart.
#[derive(Clone)]
pub struct AuthState {
    inner: Arc<AuthInner>,
}

struct AuthInner {
    gate: RwLock<super::AuthGate>,
    /// Static secret from `conf/super.toml`; `None` when none is configured.
    /// Kept dormant while the SSO gate is active — it is the break-glass.
    secret: Option<Arc<str>>,
    /// Display-only: where the secret came from (`config`, `generated`,
    /// `placeholder`) so the settings page can say where it lives.
    secret_source: Option<String>,
    /// SSO store + configuration. Present in OSS mode even before SSO is
    /// enabled, so the UI can turn it on without a restart. The store is the
    /// shared auth.db — SSO sessions and API tokens both live there.
    sso: Option<Arc<SsoRuntime>>,
    /// True on a non-loopback bind: a gate change must never resolve to
    /// `None`, or a reload would expose the API.
    require_auth: bool,
}

/// Result of evaluating one credential against the active gate.
enum GateOutcome {
    Authenticated(UserContext),
    /// The credential is missing, unknown, expired, or revoked.
    Unauthorized,
    /// The credential is valid but the account has no usable role.
    Forbidden(&'static str),
    /// SSO is configured but cannot currently authenticate anyone.
    Unavailable(String),
}

impl AuthState {
    /// Secret-only state (tests, and callers that do not own a store).
    pub fn new(secret: impl Into<String>) -> Self {
        let secret: String = secret.into();
        Self {
            inner: Arc::new(AuthInner {
                gate: RwLock::new(super::AuthGate::CoreSecret),
                secret: Some(Arc::from(secret.as_str())),
                secret_source: None,
                sso: None,
                require_auth: false,
            }),
        }
    }

    pub fn with_sso(
        secret: Option<String>,
        sso: Option<Arc<SsoRuntime>>,
        gate: super::AuthGate,
        require_auth: bool,
        // Display-only secret provenance (`/auth/status` → `secret_source`).
        secret_source: Option<String>,
    ) -> Self {
        Self {
            inner: Arc::new(AuthInner {
                gate: RwLock::new(gate),
                secret: secret.map(|s| Arc::from(s.as_str())),
                secret_source,
                sso,
                require_auth,
            }),
        }
    }

    pub fn gate(&self) -> super::AuthGate {
        *self.inner.gate.read().unwrap_or_else(|p| p.into_inner())
    }

    pub fn require_auth(&self) -> bool {
        self.inner.require_auth
    }

    pub fn secret_configured(&self) -> bool {
        self.inner.secret.is_some()
    }

    pub fn sso(&self) -> Option<&SsoRuntime> {
        self.inner.sso.as_deref()
    }

    pub fn root_context() -> UserContext {
        UserContext {
            token_id: "root".into(),
            name: "Administrator".into(),
            role: UserRole::Admin,
        }
    }

    /// `/api/v1/auth/status` payload, built from the live gate so the login
    /// page knows whether the static secret still works and whether SSO is on.
    pub async fn status_response(&self) -> AuthStatusResponse {
        let gate = self.gate();
        // OSS core serves token CRUD whenever it owns the auth store (the
        // security plugin sets this true from its own side in licensed mode).
        let token_management = self.inner.sso.is_some();
        let has_admin_token = match self.inner.sso.as_ref() {
            Some(sso) => sso.store().has_active_admin_token().await.unwrap_or(false),
            None => false,
        };
        AuthStatusResponse {
            auth_secret_disabled: false,
            has_admin_token,
            can_disable_auth_secret: false,
            auth_secret_login_allowed: gate == super::AuthGate::CoreSecret,
            token_management,
            secret_source: self.inner.secret_source.clone(),
            mode: gate.mode(),
            sso: self.inner.sso.as_ref().map(|sso| SsoStatus {
                enabled: sso.enabled(),
                available: true,
                login_path: common::SSO_LOGIN_PATH.to_string(),
            }),
        }
    }

    pub fn secret_matches(&self, token: &str) -> bool {
        self.inner
            .secret
            .as_deref()
            .is_some_and(|secret| common::secrets_equal(token, secret))
    }

    /// Recompute the gate from the live configuration. Refuses to leave a
    /// non-loopback instance unauthenticated, and refuses an SSO gate without
    /// a store.
    pub fn refresh_gate(&self) -> anyhow::Result<super::AuthGate> {
        let sso_enabled = self.inner.sso.as_ref().is_some_and(|sso| sso.enabled());
        let gate = super::AuthGate::resolve(self.inner.secret.as_deref(), false, sso_enabled);
        self.set_gate(gate)?;
        Ok(gate)
    }

    pub fn set_gate(&self, gate: super::AuthGate) -> anyhow::Result<()> {
        if gate == super::AuthGate::None && self.inner.require_auth {
            anyhow::bail!("refusing to disable authentication on a non-loopback bind");
        }
        if gate == super::AuthGate::Sso && self.inner.sso.is_none() {
            anyhow::bail!("SSO gate requested without an auth store");
        }
        *self.inner.gate.write().unwrap_or_else(|p| p.into_inner()) = gate;
        Ok(())
    }

    /// Revoke the session behind a `su_` token (logout). No-op for non-SSO
    /// gates.
    pub async fn revoke_token(&self, token: &str) -> anyhow::Result<bool> {
        let Some(sso) = self.inner.sso.as_ref() else {
            return Ok(false);
        };
        let hash = pkce::token_hash(token);
        let Some(row) = sso.store().find_session_by_token_hash(&hash).await? else {
            return Ok(false);
        };
        let revoked = sso.store().revoke_session(&row.id).await?;
        if revoked {
            let _ = sso
                .store()
                .audit(super::store::AuditEvent {
                    kind: "session_revoked",
                    sub: Some(&row.sub),
                    email: row.email.as_deref(),
                    ip: row.ip.as_deref(),
                    ua: row.user_agent.as_deref(),
                    detail: Some("logout"),
                })
                .await;
        }
        Ok(revoked)
    }

    async fn sso_authenticate(&self, token: &str) -> GateOutcome {
        let Some(sso) = self.inner.sso.as_ref() else {
            return GateOutcome::Unavailable("SSO is not initialized".into());
        };
        let Some(config) = sso.config() else {
            return GateOutcome::Unavailable("SSO configuration is missing".into());
        };

        let hash = pkce::token_hash(token);
        let row = match sso.store().find_session_by_token_hash(&hash).await {
            Ok(row) => row,
            Err(e) => return GateOutcome::Unavailable(format!("auth store: {e}")),
        };
        let Some(row) = row else {
            return GateOutcome::Unauthorized;
        };

        let now = now_secs();
        if row.revoked_at.is_some()
            || row.expires_at <= now
            || row.last_seen_at.saturating_add(config.idle_timeout_secs) <= now
        {
            return GateOutcome::Unauthorized;
        }

        // Role is recomputed per request from the current rules, so a demotion
        // takes effect immediately instead of at the next login.
        let groups: Vec<String> = serde_json::from_str(&row.groups_json).unwrap_or_default();
        let claims = IdentityClaims {
            sub: &row.sub,
            email: row.email.as_deref(),
            email_verified: row.email_verified,
            groups: &groups,
        };
        let Some((rule_index, role)) = resolve_role(&config.role_rules, &claims) else {
            tracing::warn!(
                session = %row.id,
                sub = %row.sub,
                "SSO session matches no role rule; denying"
            );
            return GateOutcome::Forbidden("no role is assigned to this account");
        };

        if now.saturating_sub(row.last_seen_at) >= 60
            && let Err(e) = sso.store().touch_session(&row.id, now).await
        {
            tracing::debug!(error = %e, "could not update session last_seen");
        }

        tracing::debug!(
            session = %row.id,
            sub = %row.sub,
            rule_index,
            role = ?role,
            "SSO session authenticated"
        );
        GateOutcome::Authenticated(UserContext {
            token_id: row.id.clone(),
            name: row
                .name
                .clone()
                .or_else(|| row.email.clone())
                .unwrap_or_else(|| row.sub.clone()),
            role,
        })
    }

    /// Authenticate a named API token under the static-secret gate. Tokens
    /// are additional credentials, checked **only after** `secret_matches`
    /// fails, so the existing break-glass path is untouched. Each token
    /// carries its own role — the first time OSS RBAC applies to real
    /// credentials (a viewer token cannot mutate).
    async fn token_authenticate(&self, token: &str) -> GateOutcome {
        let Some(sso) = self.inner.sso.as_ref() else {
            return GateOutcome::Unauthorized;
        };
        let hash = pkce::token_hash(token);
        let row = match sso.store().find_api_token_by_hash(&hash).await {
            Ok(row) => row,
            Err(e) => {
                tracing::error!(error = %e, "cannot read the api token store");
                return GateOutcome::Unauthorized;
            }
        };
        let Some(row) = row else {
            return GateOutcome::Unauthorized;
        };

        if now_secs().saturating_sub(row.last_used_at.unwrap_or(0)) >= 60
            && let Err(e) = sso.store().touch_api_token(&row.id).await
        {
            tracing::debug!(error = %e, "could not update api token last_used");
        }

        tracing::debug!(token = %row.id, role = %row.role, "api token authenticated");
        let role = match row.role.as_str() {
            "admin" => UserRole::Admin,
            "operator" => UserRole::Operator,
            _ => UserRole::Viewer,
        };
        GateOutcome::Authenticated(UserContext {
            token_id: row.id.clone(),
            name: row.name.clone(),
            role,
        })
    }
}

fn now_secs() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

/// Paths that skip the Bearer check (UI discovery + liveness).
pub fn is_auth_exempt(path: &str) -> bool {
    // `/auth/login` is NOT exempt: the shell sends Bearer with the typed secret
    // (same contract as the security plugin). Status is exempt so capability
    // discovery works before login. `/metrics` requires Bearer when auth is on
    // (Prometheus: `authorization.credentials`).
    matches!(
        path,
        "/" | "/health" | "/api/v1/openapi.json" | "/api/v1/auth/status"
    ) || path.starts_with("/api/docs")
        || path.starts_with("/assets")
}

/// SSO endpoints a browser must reach without a session. Only exempt while
/// the SSO gate is active: in secret mode these paths require the secret like
/// everything else.
pub fn is_sso_public(path: &str) -> bool {
    matches!(
        path,
        "/api/v1/auth/sso/login"
            | "/api/v1/auth/sso/callback"
            | "/api/v1/auth/sso/logout"
            // The signed logout token is the credential (no session, no CSRF
            // token); it is verified against the provider's JWKS.
            | "/api/v1/auth/sso/backchannel-logout"
            | "/api/v1/auth/sso/exchange"
            | "/api/v1/auth/sso/cli/start"
            | "/api/v1/auth/sso/cli/poll"
    )
}

/// Extract Bearer token from `Authorization`, or — WebSocket only — from the
/// percent-encoded `?token=` query value (browser/CLI cannot set a header on
/// the upgrade request; keeping query tokens off REST paths keeps secrets out
/// of proxy/audit logs).
pub fn extract_token(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    if let Some(auth) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        && let Some(token) = auth.strip_prefix("Bearer ")
        && !token.is_empty()
    {
        return Some(token.to_string());
    }
    let q = query.unwrap_or("");
    for pair in q.split('&') {
        if let Some(v) = pair.strip_prefix("token=")
            && !v.is_empty()
        {
            return Some(percent_decode(v));
        }
    }
    None
}

/// Strict percent-decoding counterpart to `encodeURIComponent` (and the CLI's
/// identical encoder): `%XX` becomes the byte; a literal `+` stays `+`.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = bytes.get(i + 1..i + 3)
            && let Ok(byte) = u8::from_str_radix(std::str::from_utf8(hex).unwrap_or(""), 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out.clone()).unwrap_or_else(|_| String::from_utf8_lossy(&out).into_owned())
}

pub async fn core_auth_middleware(
    State(state): State<AuthState>,
    peer: Option<axum::Extension<axum::extract::ConnectInfo<std::net::SocketAddr>>>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    let gate = state.gate();

    if is_auth_exempt(&path) {
        return next.run(req).await;
    }
    // No gate means loopback development; superd refuses that on a public bind.
    // Inject the local root identity so admin-only endpoints (SSO settings,
    // reload/shutdown) work in that mode, where nobody can authenticate.
    if gate == super::AuthGate::None {
        let mut req = req;
        req.extensions_mut().insert(AuthState::root_context());
        return next.run(req).await;
    }
    if gate == super::AuthGate::Sso && is_sso_public(&path) {
        return next.run(req).await;
    }

    let peer_ip = peer.map(|ci| ci.0.ip());
    let throttle = super::throttle::AuthThrottle::shared();
    let query = req.uri().query().map(str::to_string);
    let query_for_auth = if path == "/ws" {
        query.as_deref()
    } else {
        None
    };

    if let Some(token) = extract_token(req.headers(), query_for_auth) {
        let outcome = match gate {
            super::AuthGate::CoreSecret => {
                // The static secret keeps priority: its success path (root
                // context, throttle accounting) is byte-for-byte unchanged.
                if state.secret_matches(&token) {
                    GateOutcome::Authenticated(AuthState::root_context())
                } else {
                    state.token_authenticate(&token).await
                }
            }
            super::AuthGate::Sso => state.sso_authenticate(&token).await,
            super::AuthGate::Plugin | super::AuthGate::None => GateOutcome::Unauthorized,
        };

        match outcome {
            GateOutcome::Authenticated(user) => {
                if gate == super::AuthGate::CoreSecret
                    && let Some(ip) = peer_ip
                {
                    throttle.record_success(ip);
                }
                if !policy::is_allowed(&user.role, req.method(), &path) {
                    return forbidden("this account is not allowed to perform that action");
                }
                let mut req = req;
                req.extensions_mut().insert(user);
                return next.run(req).await;
            }
            GateOutcome::Forbidden(reason) => return forbidden(reason),
            GateOutcome::Unavailable(reason) => {
                tracing::error!(reason = %reason, path = %path, "SSO authentication is unavailable");
                return service_unavailable(&reason);
            }
            GateOutcome::Unauthorized => {}
        }
    }

    // Failed-auth lockout applies to the static secret only. Expired SSO
    // sessions are normal traffic, and counting them would lock out everyone
    // behind one NAT address.
    if gate == super::AuthGate::CoreSecret
        && let Some(ip) = peer_ip
    {
        if !throttle.allowed(ip) {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "status": "error",
                    "message": "too many failed auth attempts; retry later",
                })),
            )
                .into_response();
        }
        throttle.record_failure(ip);
    }

    // REST requests that still carry `?token=` (accepted only on /ws since
    // 1.8.0) get an actionable hint instead of a bare 401, plus a server-side
    // trace so the upgrade regression shows up in logs instead of only in a
    // client's silent failure.
    if path != "/ws" && query_has_token(query.as_deref()) {
        tracing::warn!(
            peer = ?peer_ip,
            %path,
            "query-string token ignored on REST path; send Authorization: Bearer instead"
        );
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "status": "error",
                "message": "unauthorized: query-string tokens are accepted only on /ws; send Authorization: Bearer <secret>",
            })),
        )
            .into_response();
    }
    unauthorized()
}

/// True when the query string carries a `token` parameter.
fn query_has_token(query: Option<&str>) -> bool {
    query.is_some_and(|q| {
        q.split('&')
            .any(|kv| kv == "token" || kv.starts_with("token="))
    })
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "status": "error",
            "message": "unauthorized",
        })),
    )
        .into_response()
}

fn forbidden(reason: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "status": "error",
            "message": format!("forbidden: {reason}"),
        })),
    )
        .into_response()
}

fn service_unavailable(reason: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({
            "status": "error",
            "message": format!("authentication is temporarily unavailable: {reason}"),
        })),
    )
        .into_response()
}

/// Layer core auth middleware onto an API router.
pub fn attach_core_auth(router: axum::Router, state: AuthState) -> axum::Router {
    router.layer(axum::middleware::from_fn_with_state(
        state,
        core_auth_middleware,
    ))
}

#[cfg(test)]
#[path = "../tests/middleware_tests.rs"]
mod tests;
