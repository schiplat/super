//! SSO (OIDC) configuration model, validation, and role resolution.
//!
//! This module is pure: no I/O, no network, no storage. The store in
//! `super-core` reads and writes it, the OIDC client fills in the identity,
//! and both sides share the same validation so a config that cannot work is
//! rejected at save time and never silently degrades at login time.

use serde::{Deserialize, Serialize};
use url::Url;
use utoipa::ToSchema;

use crate::auth::UserRole;

/// Scopes requested when the config does not say otherwise.
pub const DEFAULT_SCOPES: &str = "openid profile email";
/// ID-token claim carrying group membership.
pub const DEFAULT_GROUPS_CLAIM: &str = "groups";
/// Absolute session lifetime (12 hours).
pub const DEFAULT_SESSION_TTL_SECS: u64 = 12 * 60 * 60;
/// Idle timeout (60 minutes).
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 60 * 60;
pub const MIN_SESSION_TTL_SECS: u64 = 5 * 60;
pub const MAX_SESSION_TTL_SECS: u64 = 30 * 24 * 60 * 60;
pub const MIN_IDLE_TIMEOUT_SECS: u64 = 5 * 60;
pub const MAX_IDLE_TIMEOUT_SECS: u64 = 7 * 24 * 60 * 60;
/// Ordered role rules; the first match wins.
pub const MAX_ROLE_RULES: usize = 200;
pub const MAX_GROUPS_PER_RULE: usize = 50;
pub const MAX_SCOPES: usize = 20;
pub const MAX_CLIENT_ID_LEN: usize = 256;
pub const MAX_CLIENT_SECRET_LEN: usize = 4096;
pub const MIN_CLIENT_SECRET_LEN: usize = 8;
pub const MAX_SELECTOR_LEN: usize = 320;

/// Path the IdP redirects back to; must be registered verbatim.
pub const SSO_CALLBACK_PATH: &str = "/api/v1/auth/oidc/callback";
/// Entry point the SPA links to.
pub const SSO_LOGIN_PATH: &str = "/api/v1/auth/oidc/login";

/// Active authentication mode, as reported by `GET /api/v1/auth/status`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    #[default]
    None,
    Secret,
    Plugin,
    Sso,
}

/// Public SSO facts the login page needs. Deliberately excludes issuer and
/// client id: an anonymous caller learns only that SSO exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SsoStatus {
    pub enabled: bool,
    pub available: bool,
    pub login_path: String,
}

/// How the client authenticates at the token endpoint.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TokenAuthMethod {
    #[default]
    ClientSecretBasic,
    ClientSecretPost,
}

impl TokenAuthMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClientSecretBasic => "client_secret_basic",
            Self::ClientSecretPost => "client_secret_post",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "client_secret_basic" => Some(Self::ClientSecretBasic),
            "client_secret_post" => Some(Self::ClientSecretPost),
            _ => None,
        }
    }
}

/// One entry of the ordered role mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RoleRule {
    #[serde(rename = "match")]
    pub r#match: RuleMatch,
    pub role: UserRole,
}

/// Selectors of a role rule. Every specified selector must match (AND);
/// `groups_any` matches when the identity carries any listed group (OR).
/// An `email` selector only matches a verified email address.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RuleMatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groups_any: Option<Vec<String>>,
}

impl RuleMatch {
    /// A rule without selectors must never match; it would otherwise be a
    /// catch-all granted admin.
    pub fn is_empty(&self) -> bool {
        self.sub.is_none()
            && self.email.is_none()
            && self.groups_any.as_ref().is_none_or(Vec::is_empty)
    }
}

/// Full SSO configuration as stored in `auth.db` (one row) and accepted by
/// `PUT /api/v1/auth/oidc/config`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SsoConfig {
    pub enabled: bool,
    pub issuer: String,
    pub client_id: String,
    /// Externally reachable origin of this superd (scheme + host + port only).
    pub public_base_url: String,
    #[serde(default = "default_scopes")]
    pub scopes: String,
    #[serde(default = "default_groups_claim")]
    pub groups_claim: String,
    #[serde(default)]
    pub token_auth_method: TokenAuthMethod,
    #[serde(default)]
    pub role_rules: Vec<RoleRule>,
    #[serde(default = "default_session_ttl")]
    pub session_ttl_secs: u64,
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
    /// Allow a plain-http issuer. Only honored for loopback hosts.
    #[serde(default)]
    pub allow_insecure_issuer: bool,
}

fn default_scopes() -> String {
    DEFAULT_SCOPES.to_string()
}

/// Scopes accept comma or space separation in the UI and CLI; the
/// authorization request itself is space-separated (OAuth 2.0). Stored
/// configs are normalized so a comma never leaks into the wire format.
pub fn normalize_scopes(raw: &str) -> String {
    raw.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn default_groups_claim() -> String {
    DEFAULT_GROUPS_CLAIM.to_string()
}

fn default_session_ttl() -> u64 {
    DEFAULT_SESSION_TTL_SECS
}

fn default_idle_timeout() -> u64 {
    DEFAULT_IDLE_TIMEOUT_SECS
}

impl SsoConfig {
    /// Callback URL that must be registered at the IdP.
    pub fn redirect_uri(&self) -> String {
        format!(
            "{}{}",
            self.public_base_url.trim_end_matches('/'),
            SSO_CALLBACK_PATH
        )
    }

    /// Issuer with the trailing slash removed, for exact comparisons.
    pub fn normalized_issuer(&self) -> String {
        normalize_issuer(&self.issuer)
    }

    /// All problems with this configuration; empty means it may be stored.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        match parse_https_url(&self.issuer, self.allow_insecure_issuer) {
            Ok(url) => {
                if url.path().len() > 1 && url.path().ends_with('/') {
                    errors.push("issuer must not end with '/'".into());
                }
            }
            Err(e) => errors.push(format!("issuer: {e}")),
        }

        if self.client_id.trim().is_empty() {
            errors.push("client_id must not be empty".into());
        } else if self.client_id.len() > MAX_CLIENT_ID_LEN {
            errors.push(format!(
                "client_id must be at most {MAX_CLIENT_ID_LEN} bytes"
            ));
        } else if self
            .client_id
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        {
            errors.push("client_id must not contain whitespace or control characters".into());
        }

        match parse_origin(&self.public_base_url, "public_base_url") {
            Ok(_) => {}
            Err(e) => errors.push(e),
        }

        let scopes: Vec<&str> = self.scopes.split_whitespace().collect();
        if scopes.is_empty() {
            errors.push("scopes must not be empty".into());
        } else if scopes.len() > MAX_SCOPES {
            errors.push(format!("scopes must have at most {MAX_SCOPES} entries"));
        } else {
            for scope in &scopes {
                if scope.len() > 64
                    || !scope
                        .chars()
                        .all(|c| c.is_ascii_graphic() && c != '"' && c != '\\')
                {
                    errors.push(format!("invalid scope {scope:?}"));
                }
            }
            if !scopes.contains(&"openid") {
                errors.push("scopes must include `openid`".into());
            }
        }

        if self.groups_claim.trim().is_empty() || self.groups_claim.len() > 64 {
            errors.push("groups_claim must be 1-64 characters".into());
        } else if self
            .groups_claim
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        {
            errors.push("groups_claim must not contain whitespace or control characters".into());
        }

        if !(MIN_SESSION_TTL_SECS..=MAX_SESSION_TTL_SECS).contains(&self.session_ttl_secs) {
            errors.push(format!(
                "session_ttl_secs must be between {MIN_SESSION_TTL_SECS} and {MAX_SESSION_TTL_SECS}"
            ));
        }
        if !(MIN_IDLE_TIMEOUT_SECS..=MAX_IDLE_TIMEOUT_SECS).contains(&self.idle_timeout_secs) {
            errors.push(format!(
                "idle_timeout_secs must be between {MIN_IDLE_TIMEOUT_SECS} and {MAX_IDLE_TIMEOUT_SECS}"
            ));
        } else if self.idle_timeout_secs > self.session_ttl_secs {
            errors.push("idle_timeout_secs must not exceed session_ttl_secs".into());
        }

        if self.role_rules.len() > MAX_ROLE_RULES {
            errors.push(format!("at most {MAX_ROLE_RULES} role rules are allowed"));
        }
        for (index, rule) in self.role_rules.iter().enumerate() {
            let label = format!("role_rules[{index}]");
            if rule.r#match.is_empty() {
                errors.push(format!("{label} must have at least one selector"));
            }
            if let Some(sub) = &rule.r#match.sub
                && (sub.trim().is_empty() || sub.len() > MAX_SELECTOR_LEN)
            {
                errors.push(format!(
                    "{label}.match.sub must be 1-{MAX_SELECTOR_LEN} characters"
                ));
            }
            if let Some(email) = &rule.r#match.email
                && (email.len() > MAX_SELECTOR_LEN || !email.contains('@'))
            {
                errors.push(format!(
                    "{label}.match.email must look like an email address"
                ));
            }
            if let Some(groups) = &rule.r#match.groups_any {
                if groups.is_empty() {
                    errors.push(format!("{label}.match.groups_any must not be empty"));
                } else if groups.len() > MAX_GROUPS_PER_RULE {
                    errors.push(format!(
                        "{label}.match.groups_any must have at most {MAX_GROUPS_PER_RULE} entries"
                    ));
                }
                for group in groups {
                    if group.trim().is_empty() || group.len() > 256 {
                        errors.push(format!(
                            "{label}.match.groups_any entries must be 1-256 characters"
                        ));
                    }
                }
            }
        }
        if self.enabled && self.role_rules.is_empty() {
            errors.push("enabled SSO needs at least one role rule; nobody could sign in".into());
        }

        errors
    }
}

/// True when `raw` is a plain-http URL pointing at loopback. Only then is
/// `allow_insecure_issuer` meaningful.
pub fn is_loopback_url(raw: &str) -> bool {
    let Ok(url) = Url::parse(raw.trim()) else {
        return false;
    };
    if url.scheme() != "http" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    crate::is_loopback_bind_host(host)
}

fn normalize_issuer(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

/// Parse an absolute http(s) URL with no credentials, query, or fragment.
fn parse_https_url(raw: &str, allow_insecure: bool) -> Result<Url, String> {
    let url = Url::parse(raw.trim()).map_err(|e| format!("not a valid URL: {e}"))?;
    match url.scheme() {
        "https" => {}
        "http" => {
            let host = url.host_str().unwrap_or_default();
            let host = host.trim_start_matches('[').trim_end_matches(']');
            if !(allow_insecure && crate::is_loopback_bind_host(host)) {
                return Err(
                    "must use https (plain http is only allowed for a loopback issuer with \
                     allow_insecure_issuer = true)"
                        .into(),
                );
            }
        }
        other => return Err(format!("unsupported scheme {other:?}")),
    }
    if url.host_str().is_none() {
        return Err("missing host".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("must not contain credentials".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("must not contain a query or fragment".into());
    }
    Ok(url)
}

/// Origin-only URL (scheme + host + port); no path, query, or fragment.
fn parse_origin(raw: &str, field: &str) -> Result<Url, String> {
    let url = Url::parse(raw.trim()).map_err(|e| format!("{field}: not a valid URL: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("{field}: must be http or https"));
    }
    if url.scheme() == "http" {
        let host = url.host_str().unwrap_or_default();
        let host = host.trim_start_matches('[').trim_end_matches(']');
        if !crate::is_loopback_bind_host(host) {
            return Err(format!("{field}: http is only allowed for loopback"));
        }
    }
    if url.host_str().is_none() {
        return Err(format!("{field}: missing host"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(format!("{field}: must not contain credentials"));
    }
    if url.path() != "/" && !url.path().is_empty() {
        return Err(format!(
            "{field}: must be an origin without a path (got {:?})",
            url.path()
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(format!("{field}: must not contain a query or fragment"));
    }
    Ok(url)
}

/// Identity facts used for role resolution. `groups` is whatever the
/// configured claim carried; it is an input to local rules, never a role.
#[derive(Debug, Clone, Copy)]
pub struct IdentityClaims<'a> {
    pub sub: &'a str,
    pub email: Option<&'a str>,
    pub email_verified: bool,
    pub groups: &'a [String],
}

/// First matching rule wins; `None` means default-deny.
///
/// Returns the rule index as well so the caller can log which rule granted
/// the role (the single most useful field when an operator asks "why is this
/// user a viewer?").
pub fn resolve_role(rules: &[RoleRule], claims: &IdentityClaims<'_>) -> Option<(usize, UserRole)> {
    rules
        .iter()
        .enumerate()
        .find(|(_, rule)| rule_matches(&rule.r#match, claims))
        .map(|(index, rule)| (index, rule.role.clone()))
}

fn rule_matches(selector: &RuleMatch, claims: &IdentityClaims<'_>) -> bool {
    // Defence in depth: validation rejects selector-less rules, but a config
    // loaded from an older/corrupted file must not become "allow everyone".
    if selector.is_empty() {
        return false;
    }
    if let Some(sub) = &selector.sub
        && sub != claims.sub
    {
        return false;
    }
    if let Some(email) = &selector.email {
        if !claims.email_verified {
            return false;
        }
        match claims.email {
            Some(actual) if actual.eq_ignore_ascii_case(email) => {}
            _ => return false,
        }
    }
    if let Some(groups) = &selector.groups_any
        && !groups.iter().any(|wanted| claims.groups.contains(wanted))
    {
        return false;
    }
    true
}

#[cfg(test)]
#[path = "tests/auth_sso_tests.rs"]
mod tests;
