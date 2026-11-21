//! Core `/api/v1/auth/*` routes (OSS only; the security plugin overrides).

use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use common::UserContext;
use serde::Deserialize;
use serde_json::json;

use super::AuthGate;
use super::middleware::{AuthState, extract_token};
use super::sso::runtime::now_secs;
use crate::auth::store::{AuditEvent, NewApiToken};

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "status": "error", "message": "not found" })),
    )
        .into_response()
}

fn server_error() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "status": "error", "message": "internal server error" })),
    )
        .into_response()
}

fn bad_request(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "status": "error", "message": message })),
    )
        .into_response()
}

async fn login(State(_auth): State<AuthState>) -> impl IntoResponse {
    // Middleware already verified the credential before we get here.
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}

/// Revoking the caller's own session must work for every role, hence the
/// special case in the RBAC policy.
async fn logout(State(state): State<AuthState>, headers: HeaderMap) -> StatusCode {
    if state.gate() == AuthGate::Sso
        && let Some(token) = extract_token(&headers, None)
    {
        let _ = state.revoke_token(&token).await;
    }
    StatusCode::NO_CONTENT
}

async fn status(State(auth): State<AuthState>) -> Response {
    // No gate means the API is open (loopback development). The pre-SSO
    // contract was 404 here, and the shell infers "no auth" from it; returning
    // 200 with mode=none would make the SPA demand a login on an open instance.
    if auth.gate() == AuthGate::None {
        return StatusCode::NOT_FOUND.into_response();
    }
    Json(auth.status_response().await).into_response()
}

/// The caller's own identity. Gateway-agnostic (`secret` → root, `sso` → the
/// session's name/role), and readable by every role, so the shell does not
/// have to infer the profile from an admin-only endpoint.
async fn me(Extension(user): Extension<UserContext>) -> Json<UserContext> {
    Json(user)
}

/// Routes nested under the API router. Status is auth-exempt; login is not.
/// The SSO routes carry their own visibility rules (public flow vs admin).
pub fn auth_routes(state: AuthState) -> Router {
    Router::new()
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/status", get(status))
        .route("/api/v1/auth/me", get(me))
        .route("/api/v1/auth/tokens", get(list_tokens).post(create_token))
        .route("/api/v1/auth/tokens/{id}", delete(revoke_token))
        .merge(super::sso::routes::sso_routes())
        .with_state(state)
}

// ------------------------------------------------------------------ api tokens

/// Named API tokens (`super token list/create/revoke`, CI credentials).
/// The same paths exist in licensed mode, served by the security plugin.
/// The list is a bare array: the CLI deserializes `Vec<AuthTokenInfo>`.
async fn list_tokens(State(state): State<AuthState>) -> Response {
    let Some(sso) = state.sso() else {
        return not_found();
    };
    match sso.store().list_api_tokens().await {
        // Superset of AuthTokenInfo (adds last_used_at for the dashboard);
        // the CLI deserializes the contract fields and ignores the rest.
        Ok(rows) => Json(
            rows.into_iter()
                .map(|row| {
                    json!({
                        "id": row.id,
                        "name": row.name,
                        "token_prefix": row.token_prefix,
                        "role": row.role,
                        "created_at": row.created_at,
                        "last_used_at": row.last_used_at,
                        "expires_at": row.expires_at,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "cannot list api tokens");
            server_error()
        }
    }
}

#[derive(Debug, Deserialize)]
struct CreateTokenBody {
    name: String,
    role: common::UserRole,
    /// Unix seconds; absent = never expires.
    #[serde(default)]
    expires_at: Option<u64>,
}

async fn create_token(
    State(state): State<AuthState>,
    Extension(user): Extension<UserContext>,
    Json(body): Json<CreateTokenBody>,
) -> Response {
    let Some(sso) = state.sso() else {
        return not_found();
    };
    let name = body.name.trim();
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return bad_request(
            "token name must be 1-64 characters of letters, digits, '.', '_' or '-'",
        );
    }

    let expires_at = match body.expires_at {
        Some(expires_at) if expires_at <= now_secs() => {
            return bad_request("expires_at must be in the future");
        }
        other => other,
    };

    let token = format!("su_{}", super::sso::pkce::random_token());
    let token_sha256 = super::sso::pkce::token_hash(&token);
    // Display prefix: first 10 chars of the plaintext + ellipsis (buz-style);
    // enough to tell tokens apart, useless for authentication.
    let token_prefix = format!("{}…", token.chars().take(10).collect::<String>());
    let id = uuid::Uuid::new_v4().simple().to_string();
    let role = match body.role {
        common::UserRole::Admin => "admin",
        common::UserRole::Operator => "operator",
        common::UserRole::Viewer => "viewer",
    };

    if let Err(e) = sso
        .store()
        .insert_api_token(NewApiToken {
            id: &id,
            token_sha256: &token_sha256,
            token_prefix: &token_prefix,
            name,
            role,
            created_by: &user.name,
            created_at: now_secs(),
            expires_at,
        })
        .await
    {
        tracing::error!(error = %e, "cannot create api token");
        return server_error();
    }
    let _ = sso
        .store()
        .audit(AuditEvent {
            kind: "token_created",
            sub: None,
            email: None,
            ip: None,
            ua: None,
            detail: Some(&format!("{name} ({role}) created by {}", user.name)),
        })
        .await;

    (
        StatusCode::CREATED,
        Json(common::CreateTokenResponse {
            token,
            record: common::AuthTokenInfo {
                id,
                name: name.to_string(),
                token_prefix,
                role: body.role,
                created_at: now_secs(),
            },
        }),
    )
        .into_response()
}

async fn revoke_token(
    State(state): State<AuthState>,
    Extension(user): Extension<UserContext>,
    Path(id): Path<String>,
) -> Response {
    let Some(sso) = state.sso() else {
        return not_found();
    };
    match sso.store().revoke_api_token(&id).await {
        Ok(revoked) => {
            if revoked {
                let _ = sso
                    .store()
                    .audit(AuditEvent {
                        kind: "token_revoked",
                        sub: None,
                        email: None,
                        ip: None,
                        ua: None,
                        detail: Some(&format!("{id} revoked by {}", user.name)),
                    })
                    .await;
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "cannot revoke api token");
            server_error()
        }
    }
}
