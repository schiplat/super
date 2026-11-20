//! Built-in (OSS) HTTP authentication.
//!
//! Exactly one gate guards the API at a time. The licensed `security` plugin
//! owns auth when loaded; otherwise SSO (OIDC) when configured;
//! otherwise the static `auth_secret` from `conf/super.toml`. With no gate the
//! API is open, which is why superd refuses non-loopback binds in that state.

mod middleware;
mod routes;
pub mod throttle;

#[cfg(test)]
#[path = "policy_tests.rs"]
mod policy_tests;

pub use middleware::{AuthState, attach_core_auth, is_auth_exempt};
pub use throttle::AuthThrottle;

use axum::Router;

/// Trimmed non-empty `auth_secret` from config, if any.
pub fn non_empty_auth_secret(config_secret: Option<&str>) -> Option<String> {
    config_secret
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// The single authentication gate in front of the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthGate {
    /// No authentication (loopback development only).
    None,
    /// Static Bearer secret from `[server].auth_secret`.
    CoreSecret,
    /// The `security` plugin's HTTP auth middleware.
    Plugin,
    /// SSO sessions persisted in `data/auth.db`.
    Sso,
}

impl AuthGate {
    /// Pick the one active gate.
    ///
    /// The order is a security decision, not a preference: a loaded security
    /// plugin always wins (it also owns tokens and RBAC), and an enabled SSO
    /// configuration beats the static secret so that enabling SSO in the UI
    /// actually switches the gate off the long-lived shared secret. A stale
    /// `auth_secret` left in `conf/super.toml` stays dormant — and is the
    /// fallback if the SSO store fails to load at startup.
    pub fn resolve(
        config_secret: Option<&str>,
        plugin_auth_active: bool,
        signet_enabled: bool,
    ) -> Self {
        if plugin_auth_active {
            Self::Plugin
        } else if signet_enabled {
            Self::Sso
        } else if non_empty_auth_secret(config_secret).is_some() {
            Self::CoreSecret
        } else {
            Self::None
        }
    }

    pub fn requires_auth(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// Mount `/api/v1/auth/{login,status}` and wrap the router with Bearer middleware.
pub fn install_core_auth(router: Router, state: AuthState) -> Router {
    let router = router.merge(routes::auth_routes(state.clone()));
    attach_core_auth(router, state)
}
