//! Built-in (OSS) HTTP authentication — static Bearer from `auth_secret`.
//!
//! When the licensed `security` plugin is loaded, that plugin owns auth and
//! this module stays dormant. Otherwise core auth activates when
//! `auth_secret` in `conf/super.toml` is non-empty.
//!
//! Non-loopback TCP binds without auth refuse to start.

mod middleware;
mod routes;

#[cfg(test)]
#[path = "policy_tests.rs"]
mod policy_tests;

pub use middleware::{AuthState, attach_core_auth, is_auth_exempt};

use axum::Router;

/// Trimmed non-empty `auth_secret` from config, if any.
pub fn non_empty_auth_secret(config_secret: Option<&str>) -> Option<String> {
    config_secret
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Whether the OSS core auth middleware should wrap the API router.
///
/// Plugin auth always wins: when the security plugin's `authenticate` hook is
/// active, core auth stays off so there is exactly one gate.
pub fn core_auth_should_activate(config_secret: Option<&str>, plugin_auth_active: bool) -> bool {
    if plugin_auth_active {
        return false;
    }
    non_empty_auth_secret(config_secret).is_some()
}

/// Mount `/api/v1/auth/{login,status}` and wrap the router with Bearer middleware.
pub fn install_core_auth(router: Router, state: AuthState) -> Router {
    let router = router.merge(routes::auth_routes(state.clone()));
    attach_core_auth(router, state)
}
