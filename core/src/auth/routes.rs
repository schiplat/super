//! Core `/api/v1/auth/login` and `/auth/status` (OSS only; plugin overrides when loaded).

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use common::AuthStatusResponse;

use super::middleware::AuthState;

async fn login(State(_auth): State<AuthState>) -> impl IntoResponse {
    // Middleware already verified Bearer == secret before we get here.
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}

async fn logout() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn status() -> Json<AuthStatusResponse> {
    Json(AuthState::status_response())
}

/// Routes nested under the API router. Status is auth-exempt; login is not.
pub fn auth_routes(state: AuthState) -> Router {
    Router::new()
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/status", get(status))
        .with_state(state)
}
