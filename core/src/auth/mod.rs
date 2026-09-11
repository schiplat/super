//! Built-in (OSS) HTTP authentication — static Bearer secret.
//!
//! When the licensed `security` plugin is loaded, that plugin owns auth and
//! this module stays dormant. Otherwise superd can activate this gate for
//! non-loopback binds (always) or when `[server].auth_required = true`.

mod middleware;
mod routes;
mod secret;

#[cfg(test)]
#[path = "policy_tests.rs"]
mod policy_tests;

pub use middleware::{AuthState, attach_core_auth, is_auth_exempt};
pub use secret::{AuthSecretSource, resolve_auth_secret};

use axum::Router;
use common::is_loopback_bind_host;

/// Whether the OSS core auth middleware should wrap the API router.
///
/// Plugin auth always wins: when the security plugin's `authenticate` hook is
/// active, core auth stays off so there is exactly one gate.
pub fn core_auth_should_activate(
    host: &str,
    socket_only: bool,
    auth_required_config: bool,
    plugin_auth_active: bool,
) -> bool {
    if plugin_auth_active {
        return false;
    }
    if auth_required_config {
        return true;
    }
    // Unix-socket-only: filesystem mode bits already restrict access.
    if socket_only {
        return false;
    }
    !is_loopback_bind_host(host)
}

/// Mount `/api/v1/auth/{login,status}` and wrap the router with Bearer middleware.
pub fn install_core_auth(router: Router, state: AuthState) -> Router {
    let router = router.merge(routes::auth_routes(state.clone()));
    attach_core_auth(router, state)
}

/// Log how the secret was resolved. Prints the plaintext **only** on first
/// generation so the operator can copy it; subsequent boots only mention the path.
pub fn log_auth_secret_source(source: &AuthSecretSource, secret: &str) {
    match source {
        AuthSecretSource::Config => {
            tracing::info!("API auth using auth_secret from conf/super.toml");
        }
        AuthSecretSource::File { path } => {
            tracing::info!(
                "API auth using secret from {} (set auth_secret in conf/super.toml to override)",
                path.display()
            );
        }
        AuthSecretSource::Generated { path } => {
            tracing::warn!(
                "Generated admin secret (shown once): {secret} — stored in {} (mode 0600). \
                 Use it as the Bearer token / dashboard login. Copy it now; it will not be printed again.",
                path.display()
            );
        }
    }
}
