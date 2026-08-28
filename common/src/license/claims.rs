use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Signed license claims. The entire struct is covered by the license signature.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, ToSchema)]
pub struct LicenseClaims {
    /// Product this key belongs to (e.g. `super-pro`). Absent on legacy keys = Super Pro.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    /// Signing-key id (e.g. `k_0ac64a3f`) — required on all new licenses; must match a key in superd's compile-time ring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kid: Option<String>,
    pub issued_to: String,
    pub issued_at: u64,
    pub major_version: u32,
    /// Licensed minor line at issuance (e.g. `2` for Super 1.2.x).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minor_version: Option<u32>,
    /// Highest allowed semver **minor** within `major_version` (cap = `{major}.{max_super_minor}.*`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_super_minor: Option<u32>,
    /// Legacy signed delta above `minor_version`; prefer `max_super_minor` in new keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minor_ahead: Option<u32>,
    /// Super product version this license was issued for (e.g. `"1.2.0"`). Anchors renewals and upgrades.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_super_version: Option<String>,
    /// Authorized grant IDs (plugin stems for Super Pro, e.g. `security`, `notify`, `isolation`).
    pub grants: Vec<String>,
    /// Unix timestamp when the license expires. Omitted = no expiration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    /// When `false`, superd rejects expired keys at runtime. Omitted/`true` = keep grants offline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retain_grants_after_expiry: Option<bool>,
    /// Stable identifier for support / renewal tracking (optional in legacy licenses).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_id: Option<String>,
}

/// API / dashboard view derived from verified claims.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, ToSchema)]
pub struct LicenseInfo {
    pub issued_to: String,
    pub issued_at: u64,
    pub major_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minor_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_super_version: Option<String>,
    pub grants: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_id: Option<String>,
    /// UI feature codes (optional; clients may derive from `grants`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    /// Loaded plugin release versions (runtime; not part of signed claims).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub plugin_versions: HashMap<String, String>,
    /// `active`, `expired`, or `perpetual` — expired keys keep existing plugins offline.
    pub subscription_status: String,
    /// Product version when the license was issued (e.g. `1.2.0`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_for_version: Option<String>,
    /// Highest superd release permanently allowed after subscription ends (e.g. `1.3.x`).
    pub max_superd_version: String,
    /// Deprecated alias for `max_superd_version` (older dashboards).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub supported_super_version: String,
    /// Running superd version (runtime).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superd_version: Option<String>,
    /// Whether the running superd is within `max_superd_version` (runtime).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_in_range: Option<bool>,
    /// Renewal / upgrade page.
    pub upgrade_url: String,
    /// Highest allowed semver minor within `major_version` (from signed claims).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_super_minor: Option<u32>,
}
