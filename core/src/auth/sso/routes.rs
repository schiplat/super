//! SSO HTTP surface: browser login flow, one-time exchange, and the admin
//! endpoints behind the settings page.
//!
//! Public endpoints are exempt from the gate only while the SSO gate is
//! active (see `is_sso_public`). Admin endpoints sit behind the normal
//! middleware + RBAC and therefore require an Admin session.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, Extension, Form, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL_NO_PAD;
use common::{IdentityClaims, SsoConfig, UserContext, resolve_role};
use serde::Deserialize;
use serde_json::json;

use super::client::{OidcClient, validate_browser_endpoint};
use super::rate_limit::{self, Bucket};
use super::runtime::now_secs;
use crate::auth::store::{AuditEvent, NewSession, PendingAuth};
use crate::auth::{AuthGate, AuthState};

const COOKIE_NAME: &str = "super_sso";
const COOKIE_PATH: &str = "/api/v1/auth/sso/exchange";
/// Carries the last sign-in's raw `id_token` as the RP-logout hint
/// (`OpenID Connect RP-initiated logout` expects the RP to retain it).
/// HttpOnly, scoped to the SSO paths, expires with the id_token itself;
/// losing it only degrades sign-out to "local only".
const HINT_COOKIE_NAME: &str = "super_sso_hint";
const HINT_COOKIE_PATH: &str = "/api/v1/auth/sso";
const PENDING_TTL_SECS: u64 = 10 * 60;
const EXCHANGE_WINDOW_SECS: u64 = 120;

type Peer = Option<Extension<ConnectInfo<SocketAddr>>>;

fn peer_ip(peer: &Peer) -> Option<IpAddr> {
    peer.as_ref().map(|p| p.0.ip())
}

pub fn sso_routes() -> Router<AuthState> {
    Router::new()
        .route("/api/v1/auth/sso/login", get(login))
        .route("/api/v1/auth/sso/callback", get(callback))
        .route("/api/v1/auth/sso/logout", get(logout))
        .route(
            "/api/v1/auth/sso/backchannel-logout",
            post(backchannel_logout),
        )
        .route("/api/v1/auth/sso/exchange", post(exchange))
        .route("/api/v1/auth/sso/cli/start", post(cli_start))
        .route("/api/v1/auth/sso/cli/poll", post(cli_poll))
        .route("/api/v1/auth/sso/cli/approve", post(cli_approve))
        .route("/api/v1/auth/sso/config", get(get_config).put(put_config))
        .route("/api/v1/auth/sso/secret", post(put_secret))
        .route("/api/v1/auth/sso/test", post(test))
        .route("/api/v1/auth/sso/disable", post(disable))
        .route("/api/v1/auth/sso/sessions", get(list_sessions))
        .route("/api/v1/auth/sso/sessions/revoke-all", post(revoke_all))
        .route("/api/v1/auth/sso/sessions/{id}", delete(revoke_session))
        .route("/api/v1/auth/sso/audit", get(list_audit))
}

// ------------------------------------------------------------------ browser

#[derive(Debug, Deserialize)]
struct LoginQuery {
    #[serde(rename = "return")]
    return_to: Option<String>,
}

async fn login(
    State(state): State<AuthState>,
    Query(query): Query<LoginQuery>,
    peer: Peer,
) -> Response {
    if state.gate() != AuthGate::Sso {
        return not_found();
    }
    let Some(sso) = state.sso() else {
        return not_found();
    };
    let Some(config) = sso.config().filter(|config| config.enabled) else {
        return not_found();
    };
    if let Some(ip) = peer_ip(&peer)
        && !rate_limit::allow(ip, Bucket::Login)
    {
        return too_many_requests();
    }

    let client = match sso.client().await {
        Ok(client) => client,
        Err(e) => {
            tracing::error!(error = %e, "cannot start SSO login");
            return redirect_auth_error("server");
        }
    };

    let state_token = super::pkce::random_token();
    let nonce = super::pkce::random_token();
    let verifier = super::pkce::random_token();
    let challenge = super::pkce::pkce_challenge(&verifier);
    let now = now_secs();

    if let Err(e) = sso
        .store()
        .insert_pending(&PendingAuth {
            state: state_token.clone(),
            nonce: nonce.clone(),
            verifier,
            return_to: sanitize_return(query.return_to.as_deref()),
            created_at: now,
            expires_at: now + PENDING_TTL_SECS,
        })
        .await
    {
        tracing::error!(error = %e, "cannot store pending SSO authorization");
        return redirect_auth_error("server");
    }

    match client
        .authorization_url(&state_token, &nonce, &challenge, &config.redirect_uri())
        .await
    {
        Ok(url) => Redirect::to(&url).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "cannot build the authorization URL");
            redirect_auth_error("idp")
        }
    }
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn callback(
    State(state): State<AuthState>,
    Query(query): Query<CallbackQuery>,
    headers: HeaderMap,
    peer: Peer,
) -> Response {
    if state.gate() != AuthGate::Sso {
        return not_found();
    }
    let Some(sso) = state.sso() else {
        return not_found();
    };
    let Some(config) = sso.config().filter(|config| config.enabled) else {
        return not_found();
    };
    if let Some(ip) = peer_ip(&peer)
        && !rate_limit::allow(ip, Bucket::Callback)
    {
        return too_many_requests();
    }

    if let Some(error) = &query.error {
        tracing::warn!(
            error = %error,
            description = ?query.error_description,
            "IdP returned an authorization error"
        );
        return redirect_auth_error("idp");
    }
    let Some(state_token) = query.state.as_deref() else {
        return redirect_auth_error("state");
    };
    let Some(code) = query.code.as_deref() else {
        return redirect_auth_error("state");
    };

    let pending = match sso.store().take_pending(state_token).await {
        Ok(Some(pending)) => pending,
        Ok(None) => return redirect_auth_error("state"),
        Err(e) => {
            tracing::error!(error = %e, "cannot read pending SSO authorization");
            return redirect_auth_error("server");
        }
    };
    if pending.expires_at <= now_secs() {
        return redirect_auth_error("state");
    }

    let client = match sso.client().await {
        Ok(client) => client,
        Err(e) => {
            tracing::error!(error = %e, "cannot complete SSO login");
            return redirect_auth_error("server");
        }
    };
    let token = match client
        .exchange_code(code, &pending.verifier, &config.redirect_uri())
        .await
    {
        Ok(token) => token,
        Err(e) => {
            tracing::warn!(error = %e, "SSO code exchange failed");
            return redirect_auth_error("idp");
        }
    };
    let Some(id_token) = token.id_token.as_deref() else {
        tracing::warn!("token endpoint returned no id_token");
        return redirect_auth_error("claim");
    };
    let identity = match client.verify_id_token(id_token, &pending.nonce).await {
        Ok(identity) => identity,
        Err(e) => {
            tracing::warn!(error = %e, "ID token rejected");
            return redirect_auth_error("claim");
        }
    };

    let ip = peer_ip(&peer).map(|ip| ip.to_string());
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let claims = IdentityClaims {
        sub: &identity.sub,
        email: identity.email.as_deref(),
        email_verified: identity.email_verified,
        groups: &identity.groups,
    };
    let Some((rule_index, role)) = resolve_role(&config.role_rules, &claims) else {
        let _ = sso
            .store()
            .audit(AuditEvent {
                kind: "login_denied",
                sub: Some(&identity.sub),
                email: identity.email.as_deref(),
                ip: ip.as_deref(),
                ua: user_agent.as_deref(),
                detail: Some("no role rule matched"),
            })
            .await;
        return redirect_auth_error("denied");
    };

    let now = now_secs();
    let session_token = format!("su_{}", super::pkce::random_token());
    let token_sha256 = super::pkce::token_hash(&session_token);
    let session_id = uuid::Uuid::new_v4().simple().to_string();
    let groups_json = serde_json::to_string(&identity.groups).unwrap_or_else(|_| "[]".into());

    if let Err(e) = sso
        .store()
        .insert_session(&NewSession {
            id: session_id.clone(),
            token_sha256: token_sha256.to_vec(),
            sub: identity.sub.clone(),
            email: identity.email.clone(),
            name: identity.name.clone(),
            groups_json,
            email_verified: identity.email_verified,
            return_to: pending.return_to.clone(),
            created_at: now,
            expires_at: now + config.session_ttl_secs,
            ip: ip.clone(),
            user_agent: user_agent.clone(),
        })
        .await
    {
        tracing::error!(error = %e, "cannot create SSO session");
        return redirect_auth_error("server");
    }

    let detail = format!("rule {rule_index}: {role:?}");
    let _ = sso
        .store()
        .audit(AuditEvent {
            kind: "login_success",
            sub: Some(&identity.sub),
            email: identity.email.as_deref(),
            ip: ip.as_deref(),
            ua: user_agent.as_deref(),
            detail: Some(&detail),
        })
        .await;

    let secure = config.public_base_url.starts_with("https://");
    let mut response = Redirect::to("/login?sso=ok").into_response();
    // Two cookies: the one-time exchange ticket for the SPA, and the RP-logout
    // hint (the raw id_token) that only `GET /logout` ever reads.
    for cookie in [
        session_cookie(&session_token, secure),
        hint_cookie(id_token, secure),
    ] {
        if let Ok(value) = header::HeaderValue::from_str(&cookie) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response
}

async fn exchange(State(state): State<AuthState>, headers: HeaderMap, peer: Peer) -> Response {
    if state.gate() != AuthGate::Sso {
        return not_found();
    }
    let Some(sso) = state.sso() else {
        return not_found();
    };
    let Some(config) = sso.config().filter(|config| config.enabled) else {
        return not_found();
    };
    if let Some(ip) = peer_ip(&peer)
        && !rate_limit::allow(ip, Bucket::Exchange)
    {
        return too_many_requests();
    }

    let Some(token) = cookie_token(&headers) else {
        return unauthorized();
    };
    let token_sha256 = super::pkce::token_hash(&token);
    let now = now_secs();

    match sso.store().mark_exchange_used(&token_sha256, now).await {
        Ok(true) => {}
        Ok(false) => return unauthorized(),
        Err(e) => {
            tracing::error!(error = %e, "cannot consume the exchange ticket");
            return server_error();
        }
    }
    let row = match sso.store().find_session_by_token_hash(&token_sha256).await {
        Ok(Some(row)) => row,
        Ok(None) => return unauthorized(),
        Err(e) => {
            tracing::error!(error = %e, "cannot read the exchanged session");
            return server_error();
        }
    };
    if row.created_at + EXCHANGE_WINDOW_SECS < now || row.revoked_at.is_some() {
        return unauthorized();
    }

    let groups: Vec<String> = serde_json::from_str(&row.groups_json).unwrap_or_default();
    let claims = IdentityClaims {
        sub: &row.sub,
        email: row.email.as_deref(),
        email_verified: row.email_verified,
        groups: &groups,
    };
    let Some((_, role)) = resolve_role(&config.role_rules, &claims) else {
        return forbidden("no role is assigned to this account");
    };

    let secure = config.public_base_url.starts_with("https://");
    let mut response = Json(json!({
        "token": token,
        "expires_at": row.expires_at,
        "return_to": row.return_to,
        "user": {
            "name": row.name,
            "email": row.email,
            "role": role,
        },
    }))
    .into_response();
    if let Ok(value) = header::HeaderValue::from_str(&clear_cookie(secure)) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

/// `GET /api/v1/auth/sso/logout` — RP-initiated logout, reached as a top-level
/// GET navigation (the provider's SameSite=Lax session cookie rides along; a
/// POST would not, by the provider's logout-CSRF design).
///
/// - SSO off → straight to the SPA login page.
/// - Fresh hint cookie (unexpired `id_token` from the last sign-in) and the
///   provider advertises `end_session_endpoint` → redirect to
///   `{end_session}?client_id=…&id_token_hint=…&post_logout_redirect_uri=…`:
///   the provider destroys its session, fires its back-channel logout (which
///   revokes superd's sessions), and returns the browser to the landing page.
/// - No hint / expired hint / no advertised endpoint → purely local sign-out,
///   landing page directly. Every branch clears the hint cookie.
///
/// The *local* session is revoked by the SPA beforehand (`POST
/// /api/v1/auth/logout` with the bearer token), so sign-out never depends on
/// the back-channel being registered.
async fn logout(State(state): State<AuthState>, headers: HeaderMap) -> Response {
    let hint = hint_token(&headers);
    let mut response = if state.gate() != AuthGate::Sso {
        Redirect::to("/login").into_response()
    } else {
        rp_logout_redirect(&state, hint.as_deref()).await
    };
    if hint.is_some()
        && let Ok(value) = header::HeaderValue::from_str(&clear_hint_cookie())
    {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

/// Build the provider sign-out redirect, or fall back to the local landing.
async fn rp_logout_redirect(state: &AuthState, hint: Option<&str>) -> Response {
    let local = || {
        let mut response = Redirect::to("/login?signedout=1").into_response();
        no_store(&mut response);
        response
    };
    let Some(sso) = state.sso() else {
        return local();
    };
    let Some(config) = sso.config().filter(|config| config.enabled) else {
        return local();
    };
    // The hint is only superd's own fallback UX; the provider re-validates it.
    let Some(hint) = hint.filter(|token| hint_fresh(token)) else {
        return local();
    };
    if config.public_base_url.is_empty() {
        return local();
    }
    let client = match sso.client_without_secret().await {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!(error = %e, "cannot build a client for RP logout");
            return local();
        }
    };
    let Ok(discovery) = client.discovery().await else {
        return local();
    };
    let Some(end_session) = discovery.end_session_endpoint.clone() else {
        return local();
    };
    if validate_browser_endpoint(&end_session, "end_session_endpoint").is_err() {
        return local();
    }

    let mut url = match url::Url::parse(&end_session) {
        Ok(url) => url,
        Err(_) => return local(),
    };
    url.query_pairs_mut()
        .append_pair("client_id", &config.client_id)
        .append_pair("id_token_hint", hint)
        .append_pair(
            "post_logout_redirect_uri",
            &format!(
                "{}/login?signedout=1",
                config.public_base_url.trim_end_matches('/')
            ),
        );
    let mut response = Redirect::to(url.as_str()).into_response();
    no_store(&mut response);
    response
}

#[derive(Debug, Deserialize)]
struct BackchannelLogoutForm {
    logout_token: Option<String>,
}

/// `POST /api/v1/auth/sso/backchannel-logout` — provider-initiated session
/// revocation (`OpenID Connect Back-Channel Logout 1.0`). Public by design:
/// the signed logout token is the credential. Answers 200 for every verified
/// token, including subjects superd does not know (no account enumeration)
/// and sid-only tokens (nothing to map); 400 for anything that does not verify.
async fn backchannel_logout(
    State(state): State<AuthState>,
    peer: Peer,
    Form(form): Form<BackchannelLogoutForm>,
) -> Response {
    if state.gate() != AuthGate::Sso {
        return bad_request("SSO is not active");
    }
    let Some(sso) = state.sso() else {
        return bad_request("SSO is not configured");
    };
    if sso.config().is_none() {
        return bad_request("SSO is not configured");
    }
    if let Some(ip) = peer_ip(&peer)
        && !rate_limit::allow(ip, Bucket::Callback)
    {
        return too_many_requests();
    }
    let Some(logout_token) = form.logout_token.as_deref().filter(|t| !t.is_empty()) else {
        return bad_request("missing logout_token");
    };

    let client = match sso.client_without_secret().await {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!(error = %e, "cannot build a client for back-channel logout");
            return bad_request("logout token rejected");
        }
    };
    let sub = match client.validate_logout_token(logout_token).await {
        Ok(sub) => sub,
        Err(e) => {
            tracing::warn!(error = %e, "back-channel logout token rejected");
            return bad_request("logout token rejected");
        }
    };
    let Some(sub) = sub else {
        // sid-only: this deployment tracks users, not provider session ids.
        return StatusCode::OK.into_response();
    };

    match sso.store().revoke_by_sub(&sub).await {
        Ok(revoked) => {
            let _ = sso
                .store()
                .audit(AuditEvent {
                    kind: "sso_backchannel_logout",
                    sub: Some(&sub),
                    email: None,
                    ip: None,
                    ua: None,
                    detail: Some(&format!("{revoked} session(s) revoked by the provider")),
                })
                .await;
            Json(json!({ "revoked": revoked })).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "cannot revoke sessions for back-channel logout");
            server_error()
        }
    }
}

// -------------------------------------------------------------- CLI sign-in

/// User codes are read aloud and typed by hand: 32 symbols, no I/O/0/1, one
/// dash for readability. 32^8 combinations with a 10-minute TTL and a capped
/// table.
fn cli_user_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let bytes = super::pkce::random_bytes(8);
    let code: String = bytes
        .iter()
        .map(|byte| ALPHABET[*byte as usize % ALPHABET.len()] as char)
        .collect();
    format!("{}-{}", &code[..4], &code[4..])
}

fn normalize_user_code(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_uppercase();
    if cleaned.len() == 8 {
        format!("{}-{}", &cleaned[..4], &cleaned[4..])
    } else {
        cleaned
    }
}

async fn cli_start(State(state): State<AuthState>, peer: Peer) -> Response {
    if state.gate() != AuthGate::Sso {
        return not_found();
    }
    let Some(sso) = state.sso() else {
        return not_found();
    };
    let Some(config) = sso.config().filter(|config| config.enabled) else {
        return not_found();
    };
    if let Some(ip) = peer_ip(&peer)
        && !rate_limit::allow(ip, Bucket::CliStart)
    {
        return too_many_requests();
    }

    let device_code = super::pkce::random_token();
    let user_code = cli_user_code();
    let now = now_secs();
    let expires_in = 10 * 60;
    if let Err(e) = sso
        .store()
        .insert_cli_pending(
            &super::pkce::token_hash(&device_code),
            &user_code,
            now,
            now + expires_in,
        )
        .await
    {
        tracing::error!(error = %e, "cannot start a CLI sign-in");
        return server_error();
    }

    Json(json!({
        "device_code": device_code,
        "user_code": user_code,
        "verification_url": format!(
            "{}/auth/cli?code={user_code}",
            config.public_base_url.trim_end_matches('/')
        ),
        "expires_in": expires_in,
        "interval": 3,
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
struct CliPollBody {
    device_code: String,
}

async fn cli_poll(
    State(state): State<AuthState>,
    peer: Peer,
    Json(body): Json<CliPollBody>,
) -> Response {
    if state.gate() != AuthGate::Sso {
        return not_found();
    }
    let Some(sso) = state.sso() else {
        return not_found();
    };
    let Some(config) = sso.config().filter(|config| config.enabled) else {
        return not_found();
    };
    if let Some(ip) = peer_ip(&peer)
        && !rate_limit::allow(ip, Bucket::CliPoll)
    {
        return too_many_requests();
    }

    let device_hash = super::pkce::token_hash(&body.device_code);
    let now = now_secs();
    let row = match sso.store().find_cli_pending(&device_hash).await {
        Ok(Some(row)) => row,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json(json!({ "status": "unknown" }))).into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "cannot read a CLI sign-in request");
            return server_error();
        }
    };
    if row.expires_at <= now {
        let _ = sso.store().take_cli_pending(&device_hash).await;
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "status": "expired" })),
        )
            .into_response();
    }
    if row.approved_at.is_none() {
        return Json(json!({ "status": "pending" })).into_response();
    }

    // Single-use: the row disappears even if a second poll races this one.
    let row = match sso.store().take_cli_pending(&device_hash).await {
        Ok(Some(row)) => row,
        Ok(None) => {
            return (StatusCode::CONFLICT, Json(json!({ "status": "consumed" }))).into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "cannot consume a CLI sign-in request");
            return server_error();
        }
    };
    let Some(sub) = row.sub else {
        return bad_request("the request was approved without an identity");
    };
    let groups: Vec<String> =
        serde_json::from_str(row.groups_json.as_deref().unwrap_or("[]")).unwrap_or_default();
    let claims = IdentityClaims {
        sub: &sub,
        email: row.email.as_deref(),
        email_verified: row.email_verified.unwrap_or(false),
        groups: &groups,
    };
    let Some((_, role)) = resolve_role(&config.role_rules, &claims) else {
        return forbidden("no role is assigned to this account");
    };

    let session_token = format!("su_{}", super::pkce::random_token());
    let now = now_secs();
    let expires_at = now + config.session_ttl_secs;
    let user_agent = Some("super-cli".to_string());
    let ip = peer_ip(&peer).map(|ip| ip.to_string());
    if let Err(e) = sso
        .store()
        .insert_session(&NewSession {
            id: uuid::Uuid::new_v4().simple().to_string(),
            token_sha256: super::pkce::token_hash(&session_token).to_vec(),
            sub: sub.clone(),
            email: row.email.clone(),
            name: row.name.clone(),
            groups_json: row.groups_json.clone().unwrap_or_else(|| "[]".into()),
            email_verified: row.email_verified.unwrap_or(false),
            return_to: "/".into(),
            created_at: now,
            expires_at,
            ip: ip.clone(),
            user_agent: user_agent.clone(),
        })
        .await
    {
        tracing::error!(error = %e, "cannot create a CLI session");
        return server_error();
    }

    let _ = sso
        .store()
        .audit(AuditEvent {
            kind: "login_success",
            sub: Some(&sub),
            email: row.email.as_deref(),
            ip: ip.as_deref(),
            ua: user_agent.as_deref(),
            detail: Some("cli device code"),
        })
        .await;

    Json(json!({
        "status": "ok",
        "token": session_token,
        "expires_at": expires_at,
        "user": {
            "name": row.name,
            "email": row.email,
            "role": role,
        },
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
struct CliApproveBody {
    user_code: String,
}

async fn cli_approve(
    State(state): State<AuthState>,
    Extension(user): Extension<UserContext>,
    Json(body): Json<CliApproveBody>,
) -> Response {
    if state.gate() != AuthGate::Sso {
        return not_found();
    }
    let Some(sso) = state.sso() else {
        return not_found();
    };
    let Some(config) = sso.config().filter(|config| config.enabled) else {
        return not_found();
    };

    // Only an SSO session can approve: the CLI inherits its identity claims.
    let session = match sso.store().find_session_by_id(&user.token_id).await {
        Ok(Some(session)) => session,
        Ok(None) => {
            return bad_request("approve the CLI sign-in from an SSO browser session");
        }
        Err(e) => {
            tracing::error!(error = %e, "cannot read the approving session");
            return server_error();
        }
    };
    let now = now_secs();
    if session.revoked_at.is_some()
        || session.expires_at <= now
        || session
            .last_seen_at
            .saturating_add(config.idle_timeout_secs)
            <= now
    {
        return unauthorized();
    }

    let groups: Vec<String> = serde_json::from_str(&session.groups_json).unwrap_or_default();
    let claims = IdentityClaims {
        sub: &session.sub,
        email: session.email.as_deref(),
        email_verified: session.email_verified,
        groups: &groups,
    };
    if resolve_role(&config.role_rules, &claims).is_none() {
        return forbidden("no role is assigned to this account");
    }

    let user_code = normalize_user_code(&body.user_code);
    match sso.store().approve_cli(&user_code, &session, now).await {
        Ok(true) => {
            let _ = sso
                .store()
                .audit(AuditEvent {
                    kind: "cli_approved",
                    sub: Some(&session.sub),
                    email: session.email.as_deref(),
                    ip: session.ip.as_deref(),
                    ua: session.user_agent.as_deref(),
                    detail: Some(&format!("{user_code} approved by {}", user.name)),
                })
                .await;
            Json(json!({ "ok": true, "user_code": user_code })).into_response()
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "status": "error",
                "message": "unknown, expired, or already approved code",
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "cannot approve the CLI sign-in");
            server_error()
        }
    }
}

// -------------------------------------------------------------------- admin

async fn get_config(State(state): State<AuthState>) -> Response {
    let Some(sso) = state.sso() else {
        return not_found();
    };
    let stored = match sso.store().load_config().await {
        Ok(stored) => stored,
        Err(e) => {
            tracing::error!(error = %e, "cannot read SSO configuration");
            return server_error();
        }
    };
    let secret_updated_at = sso.store().secret_updated_at().await.ok().flatten();
    let base = stored
        .as_ref()
        .map(|s| s.config.public_base_url.trim_end_matches('/').to_string());
    Json(json!({
        "configured": stored.is_some(),
        "config": stored.as_ref().map(|s| &s.config),
        "redirect_uri": stored.as_ref().map(|s| s.config.redirect_uri()),
        "backchannel_logout_uri": base.as_ref()
            .map(|base| format!("{base}/api/v1/auth/sso/backchannel-logout")),
        "post_logout_redirect_uri": base.as_ref()
            .map(|base| format!("{base}/login?signedout=1")),
        // From the cached discovery only: no network on a settings-page read.
        // `POST /sso/test` reports the definitive answer.
        "end_session_available": sso.cache().cached_end_session().await,
        "secret_updated_at": secret_updated_at,
        "updated_at": stored.as_ref().map(|s| s.updated_at),
        "updated_by": stored.as_ref().and_then(|s| s.updated_by.clone()),
        "gate": state.gate().mode(),
        "break_glass": state.secret_configured(),
    }))
    .into_response()
}

async fn put_config(
    State(state): State<AuthState>,
    Extension(user): Extension<UserContext>,
    Json(config): Json<SsoConfig>,
) -> Response {
    let Some(sso) = state.sso() else {
        return not_found();
    };
    // The UI may submit comma-separated scopes; the authorization request is
    // space-separated, so normalize before validating and storing.
    let mut config = config;
    config.scopes = common::auth_sso::normalize_scopes(&config.scopes);

    let errors = config.validate();
    if !errors.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "status": "error", "errors": errors })),
        )
            .into_response();
    }

    // Refuse a write that would leave a non-loopback instance unauthenticated.
    if !config.enabled && !state.secret_configured() && state.require_auth() {
        return conflict(
            "disabling SSO would leave this instance unauthenticated; configure \
             [server].auth_secret first",
        );
    }

    let stored = sso.store().load_config().await.ok().flatten();
    let identity_changed = stored.as_ref().is_none_or(|stored| {
        stored.config.issuer != config.issuer
            || stored.config.client_id != config.client_id
            || stored.config.public_base_url != config.public_base_url
    });

    if config.enabled {
        let secret = match sso.store().load_secret().await {
            Ok(Some(secret)) => secret,
            Ok(None) => {
                return bad_request("set the OIDC client secret before enabling SSO");
            }
            Err(e) => {
                tracing::error!(error = %e, "cannot read the SSO client secret");
                return server_error();
            }
        };
        let previous = secret.previous_if_valid(now_secs()).map(str::to_string);
        let client = match OidcClient::new(
            Arc::new(config.clone()),
            secret.current().to_string(),
            previous,
        ) {
            Ok(client) => client,
            Err(e) => return bad_request(e.to_string()),
        };
        if let Err(e) = client.probe().await {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "status": "error",
                    "errors": [format!("discovery check failed: {e}")],
                })),
            )
                .into_response();
        }
    }

    let revision = match sso.store().save_config(&config, Some(&user.name)).await {
        Ok(revision) => revision,
        Err(e) => {
            tracing::error!(error = %e, "cannot store SSO configuration");
            return server_error();
        }
    };
    sso.set_config(Arc::new(config.clone()), revision);

    if identity_changed
        && let Ok(revoked) = sso.store().revoke_all(None).await
        && revoked > 0
    {
        tracing::info!(
            revoked,
            "SSO identity configuration changed; revoked all sessions"
        );
    }

    let detail = if config.enabled {
        format!("enabled by {}", user.name)
    } else {
        format!("disabled by {}", user.name)
    };
    let _ = sso
        .store()
        .audit(AuditEvent {
            kind: "config_changed",
            sub: None,
            email: None,
            ip: None,
            ua: None,
            detail: Some(&detail),
        })
        .await;

    if let Err(e) = state.refresh_gate() {
        tracing::error!(error = %e, "cannot apply the SSO gate");
        return conflict(&e.to_string());
    }

    get_config(State(state)).await
}

#[derive(Debug, Deserialize)]
struct SecretBody {
    secret: String,
}

async fn put_secret(
    State(state): State<AuthState>,
    Extension(user): Extension<UserContext>,
    Json(body): Json<SecretBody>,
) -> Response {
    let Some(sso) = state.sso() else {
        return not_found();
    };
    if let Err(e) = sso.store().put_secret(&body.secret).await {
        return bad_request(e.to_string());
    }
    let detail = format!("rotated by {}", user.name);
    let _ = sso
        .store()
        .audit(AuditEvent {
            kind: "secret_rotated",
            sub: None,
            email: None,
            ip: None,
            ua: None,
            detail: Some(&detail),
        })
        .await;
    StatusCode::NO_CONTENT.into_response()
}

async fn test(State(state): State<AuthState>) -> Response {
    let Some(sso) = state.sso() else {
        return not_found();
    };
    let Some(config) = sso.config() else {
        return bad_request("SSO is not configured");
    };
    let client = match OidcClient::for_probe(config.clone()) {
        Ok(client) => client,
        Err(e) => return bad_request(e.to_string()),
    };
    match client.probe().await {
        Ok(discovery) => {
            // Discovery validation already rejected a mismatched issuer, so a
            // successful probe always reports issuer_match: true.
            let issuer_match =
                discovery.issuer.trim().trim_end_matches('/') == config.normalized_issuer();
            Json(json!({
                "ok": true,
                "issuer": discovery.issuer,
                "issuer_match": issuer_match,
                "discovered_issuer": discovery.issuer,
                "authorization_endpoint": discovery.authorization_endpoint,
                "token_endpoint": discovery.token_endpoint,
                "jwks_uri": discovery.jwks_uri,
                "end_session_available": discovery.end_session_endpoint.is_some(),
                "client_auth": config.token_auth_method.as_str(),
            }))
            .into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "ok": false, "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn disable(
    State(state): State<AuthState>,
    Extension(user): Extension<UserContext>,
) -> Response {
    let Some(sso) = state.sso() else {
        return not_found();
    };
    if !state.secret_configured() && state.require_auth() {
        return conflict(
            "disabling SSO would leave this instance unauthenticated; configure \
             [server].auth_secret first",
        );
    }
    match sso.store().set_enabled(false, Some(&user.name)).await {
        Ok(true) => {}
        Ok(false) => return not_found(),
        Err(e) => {
            tracing::error!(error = %e, "cannot disable SSO");
            return server_error();
        }
    }
    if let Err(e) = sso.reload_force().await {
        tracing::warn!(error = %e, "cannot refresh the SSO configuration after disable");
    }
    let revoked = sso.store().revoke_all(None).await.unwrap_or(0);
    let _ = sso
        .store()
        .audit(AuditEvent {
            kind: "gate_changed",
            sub: None,
            email: None,
            ip: None,
            ua: None,
            detail: Some(&format!("disabled by {}", user.name)),
        })
        .await;
    if let Err(e) = state.refresh_gate() {
        tracing::error!(error = %e, "cannot apply the gate after disabling SSO");
        return conflict(&e.to_string());
    }
    Json(json!({ "ok": true, "revoked_sessions": revoked })).into_response()
}

async fn list_sessions(
    State(state): State<AuthState>,
    Extension(user): Extension<UserContext>,
) -> Response {
    let Some(sso) = state.sso() else {
        return not_found();
    };
    let config = sso.config();
    let rows = match sso.store().list_sessions(now_secs()).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(error = %e, "cannot list SSO sessions");
            return server_error();
        }
    };
    let sessions: Vec<_> = rows
        .into_iter()
        .map(|row| {
            let groups: Vec<String> = serde_json::from_str(&row.groups_json).unwrap_or_default();
            let claims = IdentityClaims {
                sub: &row.sub,
                email: row.email.as_deref(),
                email_verified: row.email_verified,
                groups: &groups,
            };
            let role = config
                .as_ref()
                .and_then(|config| resolve_role(&config.role_rules, &claims))
                .map(|(_, role)| role);
            let current = row.id == user.token_id;
            json!({
                "id": row.id,
                "sub": row.sub,
                "email": row.email,
                "name": row.name,
                "role": role,
                "created_at": row.created_at,
                "last_seen_at": row.last_seen_at,
                "expires_at": row.expires_at,
                "ip": row.ip,
                "user_agent": row.user_agent,
                "current": current,
            })
        })
        .collect();
    Json(json!({ "sessions": sessions })).into_response()
}

async fn revoke_session(
    State(state): State<AuthState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<String>,
) -> Response {
    let Some(sso) = state.sso() else {
        return not_found();
    };
    match sso.store().revoke_session(&id).await {
        Ok(true) => {
            let _ = sso
                .store()
                .audit(AuditEvent {
                    kind: "session_revoked",
                    sub: None,
                    email: None,
                    ip: None,
                    ua: None,
                    detail: Some(&format!("{id} revoked by {}", user.name)),
                })
                .await;
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => not_found(),
        Err(e) => {
            tracing::error!(error = %e, "cannot revoke SSO session");
            server_error()
        }
    }
}

#[derive(Debug, Deserialize)]
struct RevokeAllBody {
    #[serde(default)]
    keep_current: bool,
}

async fn revoke_all(
    State(state): State<AuthState>,
    Extension(user): Extension<UserContext>,
    Json(body): Json<RevokeAllBody>,
) -> Response {
    let Some(sso) = state.sso() else {
        return not_found();
    };
    let keep = body.keep_current.then_some(user.token_id.as_str());
    match sso.store().revoke_all(keep).await {
        Ok(revoked) => {
            let _ = sso
                .store()
                .audit(AuditEvent {
                    kind: "sessions_revoked_all",
                    sub: None,
                    email: None,
                    ip: None,
                    ua: None,
                    detail: Some(&format!(
                        "{revoked} revoked by {} (keep_current: {})",
                        user.name, body.keep_current
                    )),
                })
                .await;
            Json(json!({ "revoked": revoked })).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "cannot revoke SSO sessions");
            server_error()
        }
    }
}

#[derive(Debug, Deserialize)]
struct AuditQuery {
    /// Legacy alias for `page_size` (page 1); prefer page/page_size.
    limit: Option<i64>,
    page: Option<i64>,
    page_size: Option<i64>,
}

async fn list_audit(State(state): State<AuthState>, Query(query): Query<AuditQuery>) -> Response {
    let Some(sso) = state.sso() else {
        return not_found();
    };
    let page_size = query.page_size.or(query.limit).unwrap_or(25).clamp(1, 500);
    let page = query.page.unwrap_or(1).max(1);
    let offset = (page - 1) * page_size;
    match sso.store().list_audit(page_size, offset).await {
        Ok(rows) => match sso.store().count_audit().await {
            Ok(total) => Json(json!({
                "events": rows.into_iter().map(|row| json!({
                    "at": row.at,
                    "kind": row.kind,
                    "sub": row.sub,
                    "email": row.email,
                    "ip": row.ip,
                    "user_agent": row.ua,
                    "detail": row.detail,
                })).collect::<Vec<_>>(),
                "page": page,
                "page_size": page_size,
                "total": total,
            }))
            .into_response(),
            Err(e) => {
                tracing::error!(error = %e, "cannot count the auth audit log");
                server_error()
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "cannot read the auth audit log");
            server_error()
        }
    }
}

// ------------------------------------------------------------------ helpers

/// Only same-site absolute paths; anything else falls back to `/` so the
/// callback cannot be used as an open redirect.
fn sanitize_return(raw: Option<&str>) -> String {
    let Some(raw) = raw else {
        return "/".into();
    };
    let raw = raw.trim();
    if raw.is_empty()
        || raw.len() > 512
        || !raw.starts_with('/')
        || raw.starts_with("//")
        || raw.contains('\\')
        || raw.chars().any(char::is_control)
    {
        return "/".into();
    }
    raw.to_string()
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        if let Some((name, value)) = part.trim().split_once('=')
            && name == COOKIE_NAME
            && !value.is_empty()
        {
            return Some(value.to_string());
        }
    }
    None
}

fn session_cookie(token: &str, secure: bool) -> String {
    let mut cookie =
        format!("{COOKIE_NAME}={token}; Path={COOKIE_PATH}; HttpOnly; SameSite=Lax; Max-Age=60");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

fn clear_cookie(secure: bool) -> String {
    let mut cookie =
        format!("{COOKIE_NAME}=; Path={COOKIE_PATH}; HttpOnly; SameSite=Lax; Max-Age=0");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// The RP-logout hint: the raw id_token from the last sign-in. It is never
/// used for anything but `GET /logout`, and losing it only degrades sign-out
/// to "local only".
fn hint_cookie(id_token: &str, secure: bool) -> String {
    let max_age = id_token_exp(id_token).unwrap_or(0);
    let mut cookie = format!(
        "{HINT_COOKIE_NAME}={id_token}; Path={HINT_COOKIE_PATH}; HttpOnly; SameSite=Lax; Max-Age={max_age}"
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

fn clear_hint_cookie() -> String {
    format!("{HINT_COOKIE_NAME}=; Path={HINT_COOKIE_PATH}; HttpOnly; SameSite=Lax; Max-Age=0")
}

fn hint_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        if let Some((name, value)) = part.trim().split_once('=')
            && name == HINT_COOKIE_NAME
            && !value.is_empty()
        {
            return Some(value.to_string());
        }
    }
    None
}

/// Payload-only decode of the id_token's `exp`, deciding the hint's freshness
/// (cookie Max-Age and the logout fallback). The signature was verified at
/// sign-in; the provider re-validates the hint for real.
fn id_token_exp(token: &str) -> Option<u64> {
    let payload = token.split('.').nth(1)?;
    let bytes = BASE64_URL_NO_PAD.decode(payload).ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    claims.get("exp")?.as_u64()
}

fn hint_fresh(token: &str) -> bool {
    id_token_exp(token).is_some_and(|exp| exp > now_secs())
}

/// `Cache-Control: no-store` for every logout response: a cached redirect
/// would replay a stale hint.
fn no_store(response: &mut Response) {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "status": "error", "message": "not found" })),
    )
        .into_response()
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "status": "error", "message": "unauthorized" })),
    )
        .into_response()
}

fn forbidden(reason: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "status": "error", "message": format!("forbidden: {reason}") })),
    )
        .into_response()
}

fn bad_request(message: impl Into<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "status": "error", "message": message.into() })),
    )
        .into_response()
}

fn conflict(message: &str) -> Response {
    (
        StatusCode::CONFLICT,
        Json(json!({ "status": "error", "message": message })),
    )
        .into_response()
}

fn too_many_requests() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({ "status": "error", "message": "too many attempts; retry later" })),
    )
        .into_response()
}

fn server_error() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "status": "error", "message": "authentication store error" })),
    )
        .into_response()
}

fn redirect_auth_error(code: &str) -> Response {
    Redirect::to(&format!("/login?auth_error={code}")).into_response()
}

#[cfg(test)]
mod tests {
    use super::sanitize_return;

    #[test]
    fn return_paths_stay_on_site() {
        assert_eq!(sanitize_return(None), "/");
        assert_eq!(sanitize_return(Some("/programs/x")), "/programs/x");
        assert_eq!(sanitize_return(Some("  /programs  ")), "/programs");
        // Open-redirect attempts fall back to the dashboard root.
        assert_eq!(sanitize_return(Some("//evil.example")), "/");
        assert_eq!(sanitize_return(Some("/\\evil")), "/");
        assert_eq!(sanitize_return(Some("https://evil.example")), "/");
        assert_eq!(sanitize_return(Some("javascript:alert(1)")), "/");
        assert_eq!(sanitize_return(Some("/a\nb")), "/");
    }
}
