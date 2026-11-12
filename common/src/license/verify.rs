use super::claims::{LicenseClaims, LicenseInfo};
use anyhow::Context;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Product the OSS runtime verifies licenses for.
const LICENSE_PRODUCT_ID: &str = "super-pro";

/// Root verifying key embedded at compile time. This is the only long-lived
/// trust anchor: signing keys are certified by it and travel inside each
/// license, so rotating a signing key does not require a new `superd` build.
///
/// The committed file is public material. Production root keys are generated
/// and kept offline (see the private release runbook); replace this file with
/// the production root public key before shipping a release.
const ROOT_PUBLIC_KEY_BYTES: &[u8; 32] = include_bytes!("../../root.public.key");

/// Domain separation for the two Ed25519 signatures carried by a license.
pub const LICENSE_SIGNING_DOMAIN: &[u8] = b"super-license-v1\n";
pub const KEY_CERT_SIGNING_DOMAIN: &[u8] = b"super-keycert-v1\n";

/// Key-certificate format version.
pub const KEY_CERT_VERSION: u8 = 1;

/// Tolerated clock skew when checking certificate validity.
const CERT_CLOCK_SKEW_SECS: u64 = 300;

/// Decoded-field bounds (the whole Base64 payload is capped separately).
const MAX_CLAIMS_JSON_LEN: usize = 32 * 1024;
const MAX_KEY_CERT_JSON_LEN: usize = 4 * 1024;

/// Signing-key certificate carried inside a license.
///
/// `cert_signature` on the container is an Ed25519 signature by the root key
/// over `KEY_CERT_SIGNING_DOMAIN || cert_json_bytes`. The certificate binds a
/// signing `kid`/public key to this product and a validity window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyCert {
    /// Certificate format version (currently 1).
    pub v: u8,
    /// Product the signing key belongs to (e.g. `super-pro`).
    pub product_id: String,
    /// Signing key id (`k_<8hex>`), derived from `public_key`.
    pub kid: String,
    /// Base64-encoded 32-byte Ed25519 verifying key.
    pub public_key: String,
    /// Unix timestamp the key becomes valid (0 = no constraint).
    pub not_before: u64,
    /// Unix timestamp the key stops being valid (0 = no expiry).
    pub not_after: u64,
    /// Id of the root key that signed this certificate.
    pub root_kid: String,
}

/// A signing-key certificate signed by the root key, stored by Manager next to
/// the signing key and copied into every license.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedKeyCert {
    /// Base64 of the `KeyCert` JSON bytes (exactly what the root signed).
    pub cert: String,
    /// Base64 of the root Ed25519 signature over the certificate bytes.
    pub cert_signature: String,
}

/// Derive stable signing key id from an Ed25519 public key (Manager + OSS convention).
pub fn kid_for_public_bytes(bytes: &[u8; 32]) -> String {
    let hex: String = bytes[..4].iter().map(|b| format!("{b:02x}")).collect();
    format!("k_{hex}")
}

/// Renewal / purchase page (Get Super Pro). Edition comparison: feature matrix.
pub const LICENSE_UPGRADE_URL: &str = "https://super.docs.sconts.com/go/pro/";

/// Free 90-day Pro trial during public beta (Portal claim).
pub const LICENSE_TRIAL_URL: &str =
    "https://platform.ddl.sconts.com/portal/claim?product=super-pro&plan=first-trials-001";

/// Short footer for CLI / log messages when license verification fails.
pub fn license_help_footer() -> String {
    format!("More: {LICENSE_UPGRADE_URL} — free Pro trial: {LICENSE_TRIAL_URL}")
}

/// One root verifying key compiled into this binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedVerifyingKeyInfo {
    pub kid: String,
    /// Short fingerprint for support (first 8 hex chars of the 32-byte public key).
    pub fingerprint: String,
}

fn verifying_key_fingerprint(bytes: &[u8]) -> String {
    bytes.iter().take(4).map(|b| format!("{b:02x}")).collect()
}

fn embedded_root_kid() -> &'static str {
    static KID: OnceLock<String> = OnceLock::new();
    KID.get_or_init(|| kid_for_public_bytes(ROOT_PUBLIC_KEY_BYTES))
}

fn root_verifying_key() -> anyhow::Result<VerifyingKey> {
    VerifyingKey::from_bytes(ROOT_PUBLIC_KEY_BYTES)
        .map_err(|_| anyhow::anyhow!("Internal error: invalid embedded root public key"))
}

/// Every root verifying key embedded at compile time (rotation adds entries).
pub fn embedded_verifying_keys() -> Vec<EmbeddedVerifyingKeyInfo> {
    vec![EmbeddedVerifyingKeyInfo {
        kid: embedded_root_kid().to_string(),
        fingerprint: verifying_key_fingerprint(ROOT_PUBLIC_KEY_BYTES),
    }]
}

/// Key ids of the root verifying keys embedded in this binary.
pub fn embedded_verifying_key_ids() -> Vec<&'static str> {
    vec![embedded_root_kid()]
}

/// Comma-separated key ids for one-line CLI output (`super doctor`, etc.).
pub fn format_keyring_summary(kids: &[impl AsRef<str>]) -> String {
    if kids.is_empty() {
        return "none".into();
    }
    const MAX_INLINE: usize = 4;
    if kids.len() <= MAX_INLINE {
        return kids
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
            .join(", ");
    }
    let head = kids
        .iter()
        .take(3)
        .map(|s| s.as_ref())
        .collect::<Vec<_>>()
        .join(", ");
    format!("{head} (+{} more)", kids.len() - 3)
}

/// Comma-separated embedded root ids for one-line CLI output.
pub fn embedded_keyring_summary() -> String {
    format_keyring_summary(&embedded_verifying_key_ids())
}

/// Wire format of a signed license: raw signed bytes plus their detached
/// signatures, all Base64-encoded so the signed bytes are verified verbatim
/// (no re-serialization drift).
#[derive(Debug, Serialize, Deserialize)]
struct LicenseContainer {
    /// Base64(JSON) of the signed claims.
    claims: String,
    /// Base64(JSON) of the signed signing-key certificate.
    key_cert: String,
    /// Base64 Ed25519 signature over LICENSE_SIGNING_DOMAIN + claims bytes.
    signature: String,
    /// Base64 Ed25519 signature over KEY_CERT_SIGNING_DOMAIN + cert bytes.
    cert_signature: String,
}

/// Subscription expiry derived from signed claims and local clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseExpiryStatus {
    Active,
    Expired,
    Perpetual,
}

impl From<&LicenseClaims> for LicenseInfo {
    fn from(claims: &LicenseClaims) -> Self {
        let subscription_status = match license_expiry_status(claims) {
            Ok(LicenseExpiryStatus::Perpetual) => "perpetual",
            Ok(LicenseExpiryStatus::Active) => "active",
            Ok(LicenseExpiryStatus::Expired) => "expired",
            Err(_) => "unknown",
        };
        let max_superd_version = license_max_superd_version(claims);
        Self {
            issued_to: claims.issued_to.clone(),
            issued_at: claims.issued_at,
            major_version: claims.major_version,
            minor_version: claims.minor_version,
            issued_super_version: claims.issued_super_version.clone(),
            grants: claims.grants.clone(),
            expires_at: claims.expires_at,
            license_id: claims.license_id.clone(),
            features: Vec::new(),
            plugin_versions: HashMap::new(),
            subscription_status: subscription_status.to_string(),
            issued_for_version: license_issued_for_version(claims),
            max_superd_version: max_superd_version.clone(),
            supported_super_version: max_superd_version,
            superd_version: None,
            version_in_range: None,
            upgrade_url: LICENSE_UPGRADE_URL.to_string(),
            max_super_minor: licensed_max_super_minor(claims),
        }
    }
}

/// Product version recorded at issuance (e.g. `1.2.0`).
pub fn license_issued_for_version(claims: &LicenseClaims) -> Option<String> {
    if let Some(ref ver) = claims.issued_super_version {
        return Some(ver.clone());
    }
    claims
        .minor_version
        .map(|minor| format!("{}.{}.0", claims.major_version, minor))
}

/// Highest superd release this key permanently authorizes (derived from signed claims).
pub fn license_max_superd_version(claims: &LicenseClaims) -> String {
    match licensed_max_super_minor(claims) {
        Some(max_minor) => format!("{}.{}.x", claims.major_version, max_minor),
        None => format!("{}.x", claims.major_version),
    }
}

pub fn superd_within_license(claims: &LicenseClaims, superd_version: &str) -> bool {
    check_superd_version(claims, superd_version).is_ok()
}

fn current_unix_secs() -> anyhow::Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

fn ensure_not_expired(claims: &LicenseClaims) -> anyhow::Result<()> {
    if matches!(license_expiry_status(claims)?, LicenseExpiryStatus::Expired)
        && let Some(expires_at) = claims.expires_at
    {
        let now = current_unix_secs()?;
        anyhow::bail!("License expired at {expires_at} (current time {now})");
    }
    Ok(())
}

pub fn license_expiry_status(claims: &LicenseClaims) -> anyhow::Result<LicenseExpiryStatus> {
    match claims.expires_at {
        None => Ok(LicenseExpiryStatus::Perpetual),
        Some(expires_at) => {
            let now = current_unix_secs()?;
            if now > expires_at {
                Ok(LicenseExpiryStatus::Expired)
            } else {
                Ok(LicenseExpiryStatus::Active)
            }
        }
    }
}

fn decode_license_container(license_str: &str) -> anyhow::Result<LicenseContainer> {
    let trimmed = license_str.trim();
    if trimmed.len() > crate::security::MAX_LICENSE_B64_LEN {
        anyhow::bail!("License key too large");
    }

    let json_bytes = BASE64
        .decode(trimmed)
        .context("Invalid license format (Base64 decode failed)")?;

    if json_bytes.len() > crate::security::MAX_LICENSE_JSON_LEN {
        anyhow::bail!("License payload too large");
    }

    serde_json::from_slice(&json_bytes).context(
        "Invalid license structure (this license may predate the certificate format; re-issue it)",
    )
}

fn decode_field(field: &str, what: &str, max_len: usize) -> anyhow::Result<Vec<u8>> {
    let trimmed = field.trim();
    if trimmed.is_empty() {
        anyhow::bail!("License is missing {what}");
    }
    let bytes = BASE64
        .decode(trimmed)
        .with_context(|| format!("Invalid license {what} (Base64 decode failed)"))?;
    if bytes.len() > max_len {
        anyhow::bail!("License {what} is too large");
    }
    Ok(bytes)
}

fn decode_signature(field: &str, what: &str) -> anyhow::Result<Signature> {
    let bytes = decode_field(field, what, 64)?;
    Signature::from_slice(&bytes).map_err(|_| anyhow::anyhow!("Invalid {what} format"))
}

fn verifying_key_from_cert(cert: &KeyCert) -> anyhow::Result<VerifyingKey> {
    let bytes = decode_field(&cert.public_key, "key certificate public_key", 32)?;
    let key: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("Key certificate public key must be 32 bytes"))?;
    VerifyingKey::from_bytes(&key)
        .map_err(|_| anyhow::anyhow!("Key certificate public key is not a valid Ed25519 key"))
}

fn unknown_root_message(kid: &str) -> String {
    format!(
        "This license was certified by root key '{kid}', which this superd build does not recognize \
         (embedded root: {}). Upgrade superd to a release that includes that root key. {}",
        embedded_root_kid(),
        license_help_footer()
    )
}

/// Validate the certificate against the root that signed it and return the
/// certified signing key.
fn validate_key_cert(cert: &KeyCert, root_kid: &str) -> anyhow::Result<VerifyingKey> {
    if cert.v != KEY_CERT_VERSION {
        anyhow::bail!(
            "Unsupported signing-key certificate version {} (this superd supports {KEY_CERT_VERSION})",
            cert.v
        );
    }
    if cert.product_id.trim() != LICENSE_PRODUCT_ID {
        anyhow::bail!(
            "This license is for product '{}', not Super Pro. {}",
            cert.product_id,
            license_help_footer()
        );
    }

    let signing_key = verifying_key_from_cert(cert)?;
    let derived_kid = kid_for_public_bytes(&signing_key.to_bytes());
    if cert.kid != derived_kid {
        anyhow::bail!(
            "Signing-key certificate kid '{}' does not match its public key ('{derived_kid}')",
            cert.kid
        );
    }
    if cert.root_kid != root_kid {
        anyhow::bail!("{}", unknown_root_message(&cert.root_kid));
    }

    let now = current_unix_secs()?;
    if cert.not_before != 0 && now.saturating_add(CERT_CLOCK_SKEW_SECS) < cert.not_before {
        anyhow::bail!(
            "License signing-key certificate is not valid yet (not_before {}, current time {now})",
            cert.not_before
        );
    }
    if cert.not_after != 0 && now > cert.not_after.saturating_add(CERT_CLOCK_SKEW_SECS) {
        anyhow::bail!(
            "License signing-key certificate expired at {} (current time {now}); \
             ask your vendor to re-issue the license",
            cert.not_after
        );
    }

    Ok(signing_key)
}

fn verify_detached(
    public_key: &VerifyingKey,
    domain: &[u8],
    payload: &[u8],
    signature: &Signature,
) -> bool {
    let mut signed = Vec::with_capacity(domain.len() + payload.len());
    signed.extend_from_slice(domain);
    signed.extend_from_slice(payload);
    public_key.verify(&signed, signature).is_ok()
}

fn verify_signed_key_cert_with_root(
    signed: &SignedKeyCert,
    root: &VerifyingKey,
) -> anyhow::Result<(KeyCert, VerifyingKey)> {
    let cert_bytes = decode_field(&signed.cert, "key certificate", MAX_KEY_CERT_JSON_LEN)?;
    let cert_signature = decode_signature(&signed.cert_signature, "key certificate signature")?;
    let cert: KeyCert =
        serde_json::from_slice(&cert_bytes).context("Invalid signing-key certificate structure")?;

    let root_kid = kid_for_public_bytes(&root.to_bytes());
    if cert.root_kid != root_kid {
        anyhow::bail!("{}", unknown_root_message(&cert.root_kid));
    }
    if !verify_detached(root, KEY_CERT_SIGNING_DOMAIN, &cert_bytes, &cert_signature) {
        anyhow::bail!(
            "Signing-key certificate signature verification failed for root '{root_kid}' \
             (invalid or tampered)"
        );
    }

    let signing_key = validate_key_cert(&cert, &root_kid)?;
    Ok((cert, signing_key))
}

/// Decode a stored certificate without checking its root signature.
///
/// Signing-side tools use this to validate structure/kid binding; anything that
/// persists or issues a certificate must use [`verify_signed_key_cert`].
pub fn parse_signed_key_cert_unverified(signed: &SignedKeyCert) -> anyhow::Result<KeyCert> {
    let cert_bytes = decode_field(&signed.cert, "key certificate", MAX_KEY_CERT_JSON_LEN)?;
    serde_json::from_slice(&cert_bytes).context("Invalid signing-key certificate structure")
}

/// Verify a stored signing-key certificate against the embedded root key.
/// Manager calls this before storing/using a certificate.
pub fn verify_signed_key_cert(signed: &SignedKeyCert) -> anyhow::Result<KeyCert> {
    let root = root_verifying_key()?;
    verify_signed_key_cert_with_root(signed, &root).map(|(cert, _)| cert)
}

fn verify_signature_with_root(
    license_str: &str,
    root: &VerifyingKey,
) -> anyhow::Result<LicenseClaims> {
    let container = decode_license_container(license_str)?;
    let signed_cert = SignedKeyCert {
        cert: container.key_cert.clone(),
        cert_signature: container.cert_signature.clone(),
    };
    let (cert, signing_key) = verify_signed_key_cert_with_root(&signed_cert, root)?;

    let claims_bytes = decode_field(&container.claims, "claims", MAX_CLAIMS_JSON_LEN)?;
    let signature = decode_signature(&container.signature, "license signature")?;
    if !verify_detached(
        &signing_key,
        LICENSE_SIGNING_DOMAIN,
        &claims_bytes,
        &signature,
    ) {
        anyhow::bail!(
            "License signature verification failed for signing key '{}' (invalid or tampered)",
            cert.kid
        );
    }

    let claims: LicenseClaims =
        serde_json::from_slice(&claims_bytes).context("Invalid license claims structure")?;
    if claims.kid.as_deref() != Some(cert.kid.as_str()) {
        anyhow::bail!("License claims kid does not match the signing-key certificate");
    }
    if let Some(product_id) = claims.product_id.as_deref()
        && !product_id.trim().is_empty()
        && product_id.trim() != cert.product_id
    {
        anyhow::bail!("License product id does not match its signing-key certificate");
    }

    crate::security::validate_license_grant_ids(&claims.grants)?;
    Ok(claims)
}

fn verify_signature(license_str: &str) -> anyhow::Result<LicenseClaims> {
    let root = root_verifying_key()?;
    verify_signature_with_root(license_str, &root)
}

/// Verify against an explicit root key (tests / tooling).
pub fn verify_license_with_root(
    license_str: &str,
    root: &VerifyingKey,
) -> anyhow::Result<LicenseClaims> {
    let claims = verify_signature_with_root(license_str, root)?;
    ensure_not_expired(&claims)?;
    Ok(claims)
}

/// Verify a Base64-encoded signed license string (strict: rejects expired keys).
pub fn verify_license(license_str: &str) -> anyhow::Result<LicenseClaims> {
    let claims = verify_signature(license_str)?;
    ensure_not_expired(&claims)?;
    Ok(claims)
}

/// Verify signature for superd runtime (honours signed `retain_grants_after_expiry` when set).
pub fn verify_license_for_superd(
    license_str: &str,
) -> anyhow::Result<(LicenseClaims, LicenseExpiryStatus)> {
    let root = root_verifying_key()?;
    verify_license_for_superd_with_root(license_str, &root)
}

#[allow(dead_code)] // used by unit tests
fn verify_license_for_superd_with_root(
    license_str: &str,
    root: &VerifyingKey,
) -> anyhow::Result<(LicenseClaims, LicenseExpiryStatus)> {
    let claims = verify_signature_with_root(license_str, root)?;
    let expiry = license_expiry_status(&claims)?;
    if matches!(expiry, LicenseExpiryStatus::Expired)
        && claims.retain_grants_after_expiry == Some(false)
    {
        ensure_not_expired(&claims)?;
    }
    Ok((claims, expiry))
}

/// Parse the major version from a semver-like string (e.g. "1.1.9" -> 1).
pub fn parse_major_version(version: &str) -> u32 {
    parse_semver(version)
        .map(|(major, _, _)| major)
        .unwrap_or(0)
}

/// Parse `major.minor.patch` from a semver-like string (missing parts default to 0).
pub fn parse_semver(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

/// Minor line used for version cap checks (explicit `minor_version`, else parsed from `issued_super_version`).
pub fn licensed_minor_line(claims: &LicenseClaims) -> Option<u32> {
    if let Some(minor) = claims.minor_version {
        return Some(minor);
    }
    claims
        .issued_super_version
        .as_deref()
        .and_then(parse_semver)
        .map(|(_, minor, _)| minor)
}

/// Highest allowed semver minor within `major_version` (explicit claim or legacy `minor_ahead` delta).
pub fn licensed_max_super_minor(claims: &LicenseClaims) -> Option<u32> {
    if let Some(max_minor) = claims.max_super_minor {
        return Some(max_minor);
    }
    match (licensed_minor_line(claims), claims.minor_ahead) {
        (Some(issue_minor), Some(ahead)) => Some(issue_minor.saturating_add(ahead)),
        _ => None,
    }
}

/// Human-readable maximum licensed superd version for logs and CLI.
pub fn licensed_version_scope(claims: &LicenseClaims) -> String {
    if let Some(issued) = license_issued_for_version(claims) {
        format!("{issued} → max {}", license_max_superd_version(claims))
    } else {
        license_max_superd_version(claims)
    }
}

/// Check whether `superd` semver is within the signed major/minor policy.
pub fn check_superd_version(claims: &LicenseClaims, superd_version: &str) -> Result<(), String> {
    // Multi-product keys: superd only accepts Super Pro (or legacy keys without product_id).
    if let Some(pid) = claims.product_id.as_deref() {
        let pid = pid.trim();
        if !pid.is_empty() && pid != "super-pro" {
            return Err(format!(
                "This license is for product '{pid}', not Super Pro. {}",
                license_help_footer()
            ));
        }
    }

    let (host_major, host_minor, _) = parse_semver(superd_version)
        .ok_or_else(|| format!("Invalid superd version '{superd_version}'"))?;

    if host_major != claims.major_version {
        return Err(format!(
            "This license is for Super {}.x (max {}). \
             You are running superd {superd_version}. {}",
            claims.major_version,
            license_max_superd_version(claims),
            license_help_footer()
        ));
    }

    let Some(max_minor) = licensed_max_super_minor(claims) else {
        return Ok(());
    };

    if host_minor > max_minor {
        return Err(format!(
            "This license supports superd up to {host_major}.{max_minor}.x. \
             You are running superd {host_major}.{host_minor}. {}",
            license_help_footer()
        ));
    }

    Ok(())
}

#[cfg(test)]
#[path = "../tests/semver_tests.rs"]
mod semver_tests;

#[cfg(test)]
#[path = "../tests/verify_tests.rs"]
mod tests;
