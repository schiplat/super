//! ID-token verification: signature, algorithm allowlist, and the claims we
//! actually depend on.
//!
//! Kept away from the network so the rules can be unit-tested against a fake
//! JWKS: the client fetches keys, this module decides whether the token is
//! acceptable.

use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use serde_json::{Map, Value};

/// Clock skew allowance for `exp` / `iat`.
pub const CLOCK_LEEWAY_SECS: u64 = 60;
/// The `events` member a back-channel logout token must carry
/// (`OpenID Connect Back-Channel Logout 1.0`).
pub const BACKCHANNEL_LOGOUT_EVENT: &str = "http://schemas.openid.net/event/backchannel-logout";
/// How old a back-channel logout token may be (the spec recommends a short
/// window; replay within it is harmless because revocation is idempotent).
pub const BACKCHANNEL_MAX_AGE_SECS: u64 = 300;
const MAX_SUB_LEN: usize = 255;
const MAX_GROUPS: usize = 200;
const MAX_GROUP_LEN: usize = 256;
const MAX_NAME_LEN: usize = 128;
const MAX_EMAIL_LEN: usize = 320;

/// Asymmetric algorithms only. `none` and every `HS*` variant are rejected:
/// an HMAC "signature" keyed with the client secret turns a public JWKS into a
/// forgeable signing oracle.
const ALLOWED_ALGORITHMS: &[Algorithm] = &[
    Algorithm::RS256,
    Algorithm::RS384,
    Algorithm::RS512,
    Algorithm::ES256,
    Algorithm::EdDSA,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// The token names a `kid` the cached JWKS does not contain; the caller
    /// should refetch once (IdPs rotate keys without warning).
    UnknownKid(String),
    Invalid(String),
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownKid(kid) => write!(f, "unknown signing key {kid:?}"),
            Self::Invalid(reason) => write!(f, "{reason}"),
        }
    }
}

pub struct ExpectedToken<'a> {
    pub issuer: &'a str,
    pub client_id: &'a str,
    pub nonce: &'a str,
    pub groups_claim: &'a str,
}

/// What we keep from a validated token. The raw token is not retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdentity {
    pub sub: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub name: Option<String>,
    pub groups: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Claims {
    #[serde(default)]
    sub: String,
    #[serde(default)]
    aud: Value,
    #[serde(default)]
    azp: Option<String>,
    // `exp` is not read here: jsonwebtoken's Validation requires it to exist
    // and rejects it once expired, including the leeway below.
    #[serde(default)]
    iat: Option<u64>,
    #[serde(default)]
    nonce: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<Value>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    preferred_username: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

pub fn verify(
    token: &str,
    expected: &ExpectedToken<'_>,
    jwks: &JwkSet,
) -> Result<VerifiedIdentity, VerifyError> {
    let header = jsonwebtoken::decode_header(token)
        .map_err(|e| VerifyError::Invalid(format!("malformed token header: {e}")))?;
    if !ALLOWED_ALGORITHMS.contains(&header.alg) {
        return Err(VerifyError::Invalid(format!(
            "signing algorithm {:?} is not allowed",
            header.alg
        )));
    }

    let jwk = select_key(jwks, header.kid.as_deref())?;
    let key = DecodingKey::from_jwk(jwk)
        .map_err(|e| VerifyError::Invalid(format!("unusable signing key: {e}")))?;

    let mut validation = Validation::new(header.alg);
    validation.leeway = CLOCK_LEEWAY_SECS;
    validation.validate_exp = true;
    validation.validate_aud = true;
    validation.set_issuer(&[expected.issuer]);
    validation.set_audience(&[expected.client_id]);

    let data = jsonwebtoken::decode::<Claims>(token, &key, &validation)
        .map_err(|e| VerifyError::Invalid(e.to_string()))?;
    let claims = data.claims;

    // `state` was consumed by the caller; the nonce is what binds this token to
    // that specific authorization request. A missing nonce never matches.
    if claims.nonce.as_deref() != Some(expected.nonce) {
        return Err(VerifyError::Invalid(
            "nonce does not match the pending authorization".into(),
        ));
    }

    if let Some(azp) = &claims.azp
        && azp != expected.client_id
    {
        return Err(VerifyError::Invalid(format!(
            "authorized party {azp:?} is not this client"
        )));
    }
    if claims.azp.is_none()
        && let Value::Array(audiences) = &claims.aud
        && audiences.len() > 1
    {
        return Err(VerifyError::Invalid(
            "multiple audiences without an azp claim".into(),
        ));
    }

    let sub = claims.sub.trim();
    if sub.is_empty() || sub.len() > MAX_SUB_LEN {
        return Err(VerifyError::Invalid("sub is missing or too long".into()));
    }

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    if let Some(iat) = claims.iat
        && iat > now + CLOCK_LEEWAY_SECS
    {
        return Err(VerifyError::Invalid(
            "token was issued in the future".into(),
        ));
    }

    Ok(VerifiedIdentity {
        sub: sub.to_string(),
        email: claims
            .email
            .as_deref()
            .map(|e| clean_text(e, MAX_EMAIL_LEN))
            .filter(|e| !e.is_empty()),
        email_verified: parse_email_verified(claims.email_verified.as_ref()),
        name: claims
            .name
            .as_deref()
            .or(claims.preferred_username.as_deref())
            .map(|n| clean_text(n, MAX_NAME_LEN))
            .filter(|n| !n.is_empty()),
        groups: extract_groups(&claims.extra, expected.groups_claim),
    })
}

/// A back-channel logout token (`OpenID Connect Back-Channel Logout 1.0`).
/// `iss`/`aud` are validated by the JWT layer; `exp` is optional on logout
/// tokens and only checked when present, like buz's validation.
#[derive(Debug, Deserialize)]
struct LogoutTokenClaims {
    #[serde(default)]
    sub: Option<String>,
    #[serde(default)]
    sid: Option<String>,
    iat: u64,
    #[serde(default)]
    exp: Option<u64>,
    #[serde(default)]
    nonce: Option<String>,
    #[serde(default)]
    events: Option<Value>,
}

/// Validate a back-channel logout token and return the subject whose sessions
/// must be revoked. `Ok(None)` is a *valid* `sid`-only token: this deployment
/// tracks users, not provider session ids, so there is nothing to map.
///
/// Signature + `iss`/`aud` are checked here; the caller refetches the JWKS
/// once on [`VerifyError::UnknownKid`], exactly as for ID tokens.
pub fn validate_logout_token(
    token: &str,
    issuer: &str,
    client_id: &str,
    jwks: &JwkSet,
) -> Result<Option<String>, VerifyError> {
    let header = jsonwebtoken::decode_header(token)
        .map_err(|e| VerifyError::Invalid(format!("malformed logout token header: {e}")))?;
    if !ALLOWED_ALGORITHMS.contains(&header.alg) {
        return Err(VerifyError::Invalid(format!(
            "signing algorithm {:?} is not allowed",
            header.alg
        )));
    }

    let jwk = select_key(jwks, header.kid.as_deref())?;
    let key = DecodingKey::from_jwk(jwk)
        .map_err(|e| VerifyError::Invalid(format!("unusable signing key: {e}")))?;

    let mut validation = Validation::new(header.alg);
    validation.leeway = CLOCK_LEEWAY_SECS;
    // `exp` is optional on logout tokens: checked manually below when present.
    validation.validate_exp = false;
    validation.validate_aud = true;
    validation.set_issuer(&[issuer]);
    validation.set_audience(&[client_id]);
    validation.required_spec_claims.clear();

    let claims = jsonwebtoken::decode::<LogoutTokenClaims>(token, &key, &validation)
        .map_err(|e| VerifyError::Invalid(e.to_string()))?
        .claims;

    // A logout token must not carry a nonce: that would make it replayable
    // as an ID token for this client.
    if claims.nonce.is_some() {
        return Err(VerifyError::Invalid(
            "logout token must not carry a nonce".into(),
        ));
    }

    // The spec says `events` is an object with the event URI as a member.
    match &claims.events {
        Some(Value::Object(map)) if map.contains_key(BACKCHANNEL_LOGOUT_EVENT) => {}
        _ => {
            return Err(VerifyError::Invalid(
                "logout token lacks the back-channel logout event".into(),
            ));
        }
    }

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    if let Some(exp) = claims.exp
        && exp + CLOCK_LEEWAY_SECS < now
    {
        return Err(VerifyError::Invalid("logout token is expired".into()));
    }
    if claims.iat > now + CLOCK_LEEWAY_SECS {
        return Err(VerifyError::Invalid(
            "logout token was issued in the future".into(),
        ));
    }
    if now.saturating_sub(claims.iat) > BACKCHANNEL_MAX_AGE_SECS {
        return Err(VerifyError::Invalid("logout token is stale".into()));
    }

    let sub = claims
        .sub
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if sub.is_none() && claims.sid.is_none() {
        return Err(VerifyError::Invalid(
            "logout token carries neither sub nor sid".into(),
        ));
    }
    match &sub {
        Some(sub) if sub.len() > MAX_SUB_LEN => {
            Err(VerifyError::Invalid("sub is missing or too long".into()))
        }
        other => Ok(other.clone()),
    }
}

fn select_key<'a>(jwks: &'a JwkSet, kid: Option<&str>) -> Result<&'a Jwk, VerifyError> {
    match kid {
        Some(kid) => jwks
            .keys
            .iter()
            .find(|key| key.common.key_id.as_deref() == Some(kid))
            .ok_or_else(|| VerifyError::UnknownKid(kid.to_string())),
        // Some IdPs omit `kid` on single-key setups; accept that only when the
        // JWKS is unambiguous.
        None => match jwks.keys.as_slice() {
            [only] => Ok(only),
            _ => Err(VerifyError::Invalid(
                "token has no kid and the JWKS is not unambiguous".into(),
            )),
        },
    }
}

/// `email_verified` is a boolean in the spec, but several IdPs send the string
/// `"true"`. Anything else — including a missing claim — is unverified.
fn parse_email_verified(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(verified)) => *verified,
        Some(Value::String(raw)) => raw.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

fn extract_groups(extra: &Map<String, Value>, claim: &str) -> Vec<String> {
    let raw = extra.get(claim);
    let mut groups: Vec<String> = match raw {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(|g| clean_text(g, MAX_GROUP_LEN))
            .filter(|g| !g.is_empty())
            .collect(),
        Some(Value::String(single)) => {
            let group = clean_text(single, MAX_GROUP_LEN);
            if group.is_empty() {
                Vec::new()
            } else {
                vec![group]
            }
        }
        _ => Vec::new(),
    };
    groups.truncate(MAX_GROUPS);
    groups
}

fn clean_text(raw: &str, max: usize) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_verified_accepts_bool_and_string() {
        assert!(parse_email_verified(Some(&Value::Bool(true))));
        assert!(!parse_email_verified(Some(&Value::Bool(false))));
        assert!(parse_email_verified(Some(&Value::String("TRUE".into()))));
        assert!(!parse_email_verified(Some(&Value::String("no".into()))));
        assert!(!parse_email_verified(None));
    }

    #[test]
    fn groups_accept_array_string_and_junk() {
        let mut extra = Map::new();
        extra.insert("groups".into(), serde_json::json!(["a", 1, "b"]));
        assert_eq!(extract_groups(&extra, "groups"), vec!["a", "b"]);

        extra.insert("groups".into(), serde_json::json!("solo"));
        assert_eq!(extract_groups(&extra, "groups"), vec!["solo"]);

        extra.insert("groups".into(), serde_json::json!({"nested": true}));
        assert!(extract_groups(&extra, "groups").is_empty());
    }

    #[test]
    fn group_count_and_length_are_capped() {
        let mut extra = Map::new();
        let many: Vec<String> = (0..300).map(|i| format!("g{i}")).collect();
        extra.insert("groups".into(), serde_json::json!(many));
        assert_eq!(extract_groups(&extra, "groups").len(), MAX_GROUPS);

        extra.insert("groups".into(), serde_json::json!(["x".repeat(1000)]));
        assert_eq!(extract_groups(&extra, "groups")[0].len(), MAX_GROUP_LEN);
    }

    #[test]
    fn control_characters_are_stripped_from_names() {
        assert_eq!(clean_text("a\nb\tc", 100), "abc");
        assert_eq!(clean_text("  padded  ", 100), "padded");
        assert_eq!(clean_text("truncate-me", 4), "trun");
    }
}
