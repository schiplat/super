//! Core RBAC: read-only for any session, mutations for Operator, admin
//! endpoints for Admin.

use super::{is_allowed, required_role};
use axum::http::Method;
use common::UserRole;

fn method(name: &str) -> Method {
    Method::from_bytes(name.as_bytes()).unwrap()
}

#[test]
fn viewers_can_read_but_not_mutate() {
    assert!(is_allowed(
        &UserRole::Viewer,
        &method("GET"),
        "/api/v1/programs"
    ));
    assert!(is_allowed(
        &UserRole::Viewer,
        &method("GET"),
        "/api/v1/programs/x/logs"
    ));
    assert!(!is_allowed(
        &UserRole::Viewer,
        &method("POST"),
        "/api/v1/programs/x/start"
    ));
    assert!(!is_allowed(
        &UserRole::Viewer,
        &method("PUT"),
        "/api/v1/stack"
    ));
}

#[test]
fn operators_can_mutate_but_not_administer() {
    assert!(is_allowed(
        &UserRole::Operator,
        &method("POST"),
        "/api/v1/programs/x/restart"
    ));
    assert!(is_allowed(
        &UserRole::Operator,
        &method("POST"),
        "/api/v1/stack"
    ));
    assert!(!is_allowed(
        &UserRole::Operator,
        &method("POST"),
        "/api/v1/system/reload"
    ));
    assert!(!is_allowed(
        &UserRole::Operator,
        &method("PUT"),
        "/api/v1/auth/oidc/config"
    ));
}

#[test]
fn admins_can_do_everything() {
    for (m, path) in [
        ("GET", "/api/v1/programs"),
        ("POST", "/api/v1/programs"),
        ("POST", "/api/v1/system/shutdown"),
        ("PUT", "/api/v1/auth/oidc/config"),
    ] {
        assert!(is_allowed(&UserRole::Admin, &method(m), path), "{m} {path}");
    }
}

#[test]
fn admin_endpoints_require_admin_even_for_reads() {
    assert_eq!(
        required_role(&method("GET"), "/api/v1/auth/oidc/sessions"),
        UserRole::Admin
    );
    assert_eq!(
        required_role(&method("GET"), "/api/v1/auth/oidc/config"),
        UserRole::Admin
    );
    assert!(!is_allowed(
        &UserRole::Operator,
        &method("GET"),
        "/api/v1/auth/oidc/sessions"
    ));
}

#[test]
fn logout_is_available_to_any_session() {
    assert_eq!(
        required_role(&method("POST"), "/api/v1/auth/logout"),
        UserRole::Viewer
    );
    assert!(is_allowed(
        &UserRole::Viewer,
        &method("POST"),
        "/api/v1/auth/logout"
    ));
}

#[test]
fn metrics_and_websocket_are_read_only() {
    assert!(is_allowed(&UserRole::Viewer, &method("GET"), "/metrics"));
    assert!(is_allowed(&UserRole::Viewer, &method("GET"), "/ws"));
    assert!(!is_allowed(&UserRole::Viewer, &method("POST"), "/metrics"));
}

#[test]
fn plugin_ui_assets_stay_readable() {
    assert!(is_allowed(
        &UserRole::Viewer,
        &method("GET"),
        "/api/v1/plugins/ui/ui.js"
    ));
}

#[test]
fn token_management_is_admin_only() {
    assert_eq!(
        required_role(&method("GET"), "/api/v1/auth/tokens"),
        UserRole::Admin
    );
    assert_eq!(
        required_role(&method("POST"), "/api/v1/auth/tokens"),
        UserRole::Admin
    );
    assert_eq!(
        required_role(&method("DELETE"), "/api/v1/auth/tokens/x"),
        UserRole::Admin
    );
    assert!(!is_allowed(
        &UserRole::Operator,
        &method("GET"),
        "/api/v1/auth/tokens"
    ));
}
