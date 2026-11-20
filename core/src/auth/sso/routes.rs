//! SSO HTTP surface: browser login flow, one-time exchange, and the admin
//! endpoints behind the settings page.
//!
//! Public endpoints are exempt from the gate only while the SSO gate is
//! active (see `is_sso_public`). Admin endpoints sit behind the normal
//! middleware + RBAC and therefore require an Admin session.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, Extension, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use common::{IdentityClaims, SsoConfig, UserContext, resolve_role};
use serde::Deserialize;
use serde_json::json;

use super::client::OidcClient;
use super::rate_limit::{self, Bucket};
use super::runtime::now_secs;
use crate::auth::store::{AuditEvent, NewSession, PendingAuth};
use crate::auth::{AuthGate, AuthState};

const COOKIE_NAME: &str = "super_sso";
const COOKIE_PATH: &str = "/api/v1/auth/sso/exchange";
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
    if let Ok(value) = header::HeaderValue::from_str(&session_cookie(&session_token, secure)) {
        response.headers_mut().insert(header::SET_COOKIE, value);
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
    Json(json!({
        "configured": stored.is_some(),
        "config": stored.as_ref().map(|s| &s.config),
        "redirect_uri": stored.as_ref().map(|s| s.config.redirect_uri()),
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
    let client = match OidcClient::for_probe(config) {
        Ok(client) => client,
        Err(e) => return bad_request(e.to_string()),
    };
    match client.probe().await {
        Ok(discovery) => Json(json!({
            "ok": true,
            "issuer": discovery.issuer,
            "authorization_endpoint": discovery.authorization_endpoint,
            "token_endpoint": discovery.token_endpoint,
            "jwks_uri": discovery.jwks_uri,
        }))
        .into_response(),
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
    limit: Option<i64>,
}

async fn list_audit(State(state): State<AuthState>, Query(query): Query<AuditQuery>) -> Response {
    let Some(sso) = state.sso() else {
        return not_found();
    };
    match sso.store().list_audit(query.limit.unwrap_or(50)).await {
        Ok(rows) => Json(json!({
            "events": rows.into_iter().map(|row| json!({
                "at": row.at,
                "kind": row.kind,
                "sub": row.sub,
                "email": row.email,
                "ip": row.ip,
                "user_agent": row.ua,
                "detail": row.detail,
            })).collect::<Vec<_>>(),
        }))
        .into_response(),
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
