//! Core `/api/v1/auth/*` routes (OSS only; the security plugin overrides).

use axum::{
    Json, Router,
    extract::{Extension, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use common::{AuthStatusResponse, UserContext};

use super::AuthGate;
use super::middleware::{AuthState, extract_token};

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

async fn status(State(auth): State<AuthState>) -> Json<AuthStatusResponse> {
    Json(auth.status_response())
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
        .merge(super::sso::routes::sso_routes())
        .with_state(state)
}
