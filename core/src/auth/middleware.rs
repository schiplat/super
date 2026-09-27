//! Axum middleware: require `Authorization: Bearer <secret>` (or `?token=`).

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use common::{AuthStatusResponse, UserContext, UserRole};
use std::sync::Arc;

/// Shared state for the OSS auth middleware and `/auth/*` handlers.
#[derive(Clone)]
pub struct AuthState {
    pub secret: Arc<str>,
}

impl AuthState {
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: Arc::from(secret.into()),
        }
    }

    pub fn root_context() -> UserContext {
        UserContext {
            token_id: "root".into(),
            name: "Administrator".into(),
            role: UserRole::Admin,
        }
    }

    pub fn status_response() -> AuthStatusResponse {
        AuthStatusResponse {
            auth_secret_disabled: false,
            has_admin_token: false,
            can_disable_auth_secret: false,
            auth_secret_login_allowed: true,
            token_management: false,
        }
    }

    pub fn secret_matches(&self, token: &str) -> bool {
        common::secrets_equal(token, self.secret.as_ref())
    }
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

/// Extract Bearer token from `Authorization` or `?token=` query (WebSocket).
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
            // Query values may be percent-encoded; keep raw match for secrets
            // that are hex/base64 without reserved chars.
            return Some(v.to_string());
        }
    }
    None
}

pub async fn core_auth_middleware(
    State(state): State<AuthState>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    if is_auth_exempt(&path) {
        return next.run(req).await;
    }

    let query = req.uri().query().map(str::to_string);
    let token = extract_token(req.headers(), query.as_deref());

    let Some(token) = token else {
        return unauthorized();
    };
    if !state.secret_matches(&token) {
        return unauthorized();
    }

    let mut req = req;
    req.extensions_mut().insert(AuthState::root_context());
    next.run(req).await
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

/// Layer core auth middleware onto an API router. Caller must only invoke this
/// when [`super::core_auth_should_activate`] is true.
pub fn attach_core_auth(router: axum::Router, state: AuthState) -> axum::Router {
    router.layer(axum::middleware::from_fn_with_state(
        state,
        core_auth_middleware,
    ))
}

#[cfg(test)]
#[path = "../tests/middleware_tests.rs"]
mod tests;
