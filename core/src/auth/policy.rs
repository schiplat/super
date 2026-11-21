//! Coarse RBAC for the core API.
//!
//! Core auth historically had exactly one identity (root/Admin), so handlers
//! never checked permissions. SSO sessions can be Viewer/Operator, so the
//! middleware has to enforce a policy or the roles would be decorative.
//!
//! The policy is deliberately coarse and centralized: read-only requests need
//! any session, mutations need Operator, and the few destructive/admin
//! endpoints need Admin. Fine-grained rules (per program, per action) are the
//! commercial plugin's job; this is the OSS floor.

use axum::http::Method;
use common::UserRole;

fn rank(role: &UserRole) -> u8 {
    match role {
        UserRole::Viewer => 0,
        UserRole::Operator => 1,
        UserRole::Admin => 2,
    }
}

/// Minimum role for a request. Anything not listed as admin and not a
/// read-only method is a mutation.
pub fn required_role(method: &Method, path: &str) -> UserRole {
    // Logging out must never require more than the session that is being
    // revoked, or a demoted user could not end their own session.
    if path == "/api/v1/auth/logout" {
        return UserRole::Viewer;
    }
    // Approving a CLI sign-in issues a session for the approver's own role, so
    // every authenticated role may do it (the CLI is then bound to that role).
    if path == "/api/v1/auth/sso/cli/approve" {
        return UserRole::Viewer;
    }
    if path.starts_with("/api/v1/auth/sso/")
        // Token management is admin-only: a lower-privileged token must
        // neither enumerate nor mint credentials (buz denies its identity
        // plane to API tokens for the same reason).
        || path.starts_with("/api/v1/auth/tokens")
        || path == "/api/v1/system/shutdown"
        || path == "/api/v1/system/reload"
    {
        return UserRole::Admin;
    }
    match *method {
        Method::GET | Method::HEAD | Method::OPTIONS => UserRole::Viewer,
        _ => UserRole::Operator,
    }
}

pub fn is_allowed(role: &UserRole, method: &Method, path: &str) -> bool {
    rank(role) >= rank(&required_role(method, path))
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
