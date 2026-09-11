//! Auth API DTOs for optional `security` plugin routes (`/api/v1/auth/…`).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Auth role
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Viewer,
    Operator,
    Admin,
}

impl UserRole {
    pub fn is_admin(&self) -> bool {
        matches!(self, Self::Admin)
    }
}

/// Persisted auth token record (includes hash; never send hash to clients).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthRecord {
    pub id: String,
    pub name: String,
    pub token_hash: String,
    pub token_prefix: String,
    pub role: UserRole,
    pub created_at: u64,
}

/// Public token metadata (safe for API responses).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct AuthTokenInfo {
    pub id: String,
    pub name: String,
    pub token_prefix: String,
    pub role: UserRole,
    pub created_at: u64,
}

impl From<&AuthRecord> for AuthTokenInfo {
    fn from(r: &AuthRecord) -> Self {
        Self {
            id: r.id.clone(),
            name: r.name.clone(),
            token_prefix: r.token_prefix.clone(),
            role: r.role.clone(),
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct CreateTokenRequest {
    pub name: String,
    pub role: UserRole,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateTokenResponse {
    pub token: String,
    pub record: AuthTokenInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    pub token_id: String,
    pub name: String,
    pub role: UserRole,
}

/// `GET /api/v1/auth/status` payload.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthStatusResponse {
    /// True when an Admin has explicitly disabled config `auth_secret`.
    pub auth_secret_disabled: bool,
    /// At least one Admin Access Token exists.
    pub has_admin_token: bool,
    /// Caller may call `POST /api/v1/auth/secret/disable`.
    pub can_disable_auth_secret: bool,
    /// Whether Bearer `auth_secret` is accepted right now.
    pub auth_secret_login_allowed: bool,
    /// True when multi-user token CRUD (`/auth/tokens`) is available.
    /// OSS core auth sets this to `false`; the security plugin sets `true`.
    #[serde(default)]
    pub token_management: bool,
}
