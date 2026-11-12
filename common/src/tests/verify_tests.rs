use super::*;
use crate::license::claims::LicenseClaims;
use ed25519_dalek::{Signer, SigningKey};

fn random_key() -> SigningKey {
    use rand::TryRngCore;
    let mut seed = [0u8; 32];
    rand::rngs::OsRng
        .try_fill_bytes(&mut seed)
        .expect("OS RNG is unavailable");
    SigningKey::from_bytes(&seed)
}

fn now() -> u64 {
    current_unix_secs().unwrap()
}

fn claims_for(kid: &str) -> LicenseClaims {
    LicenseClaims {
        product_id: Some("super-pro".into()),
        kid: Some(kid.into()),
        issued_to: "test@example.com".into(),
        issued_at: 1,
        major_version: 1,
        minor_version: Some(9),
        max_super_minor: Some(9),
        minor_ahead: None,
        issued_super_version: Some("1.9.0".into()),
        grants: vec!["security".into()],
        expires_at: None,
        retain_grants_after_expiry: None,
        license_id: Some("lic-test".into()),
    }
}

fn cert_signed_by(
    root: &SigningKey,
    signing: &SigningKey,
    product: &str,
    not_before: u64,
    not_after: u64,
) -> (Vec<u8>, Vec<u8>) {
    let pubkey = signing.verifying_key().to_bytes();
    let cert = KeyCert {
        v: 1,
        product_id: product.into(),
        kid: kid_for_public_bytes(&pubkey),
        public_key: BASE64.encode(pubkey),
        not_before,
        not_after,
        root_kid: kid_for_public_bytes(&root.verifying_key().to_bytes()),
    };
    let bytes = serde_json::to_vec(&cert).unwrap();
    let mut signed = KEY_CERT_SIGNING_DOMAIN.to_vec();
    signed.extend_from_slice(&bytes);
    let signature = root.sign(&signed).to_bytes().to_vec();
    (bytes, signature)
}

fn container_for(
    root: &SigningKey,
    signing: &SigningKey,
    claims: &LicenseClaims,
    product: &str,
    not_before: u64,
    not_after: u64,
) -> LicenseContainer {
    let (cert_bytes, cert_signature) =
        cert_signed_by(root, signing, product, not_before, not_after);
    let claims_bytes = serde_json::to_vec(claims).unwrap();
    let mut signed = LICENSE_SIGNING_DOMAIN.to_vec();
    signed.extend_from_slice(&claims_bytes);
    let signature = signing.sign(&signed).to_bytes().to_vec();
    LicenseContainer {
        claims: BASE64.encode(claims_bytes),
        key_cert: BASE64.encode(cert_bytes),
        signature: BASE64.encode(signature),
        cert_signature: BASE64.encode(cert_signature),
    }
}

fn encode(container: &LicenseContainer) -> String {
    BASE64.encode(serde_json::to_vec(container).unwrap())
}

fn valid_license(root: &SigningKey, signing: &SigningKey) -> String {
    let kid = kid_for_public_bytes(&signing.verifying_key().to_bytes());
    let container = container_for(root, signing, &claims_for(&kid), "super-pro", 0, 0);
    encode(&container)
}

#[test]
fn parse_major_version_works() {
    assert_eq!(parse_major_version("1.1.9"), 1);
    assert_eq!(parse_major_version("2.0.0"), 2);
}

#[test]
fn valid_license_verifies_against_its_root() {
    let root = random_key();
    let signing = random_key();
    let token = valid_license(&root, &signing);
    let claims = verify_license_with_root(&token, &root.verifying_key()).expect("valid license");
    assert_eq!(claims.issued_to, "test@example.com");
    assert_eq!(
        claims.kid.as_deref(),
        Some(kid_for_public_bytes(&signing.verifying_key().to_bytes()).as_str())
    );
}

#[test]
fn tampered_claims_are_rejected() {
    let root = random_key();
    let signing = random_key();
    let kid = kid_for_public_bytes(&signing.verifying_key().to_bytes());
    let mut container = container_for(&root, &signing, &claims_for(&kid), "super-pro", 0, 0);
    let mut claims = claims_for(&kid);
    claims.issued_to = "attacker@example.com".into();
    container.claims = BASE64.encode(serde_json::to_vec(&claims).unwrap());
    let err = verify_license_with_root(&encode(&container), &root.verifying_key())
        .unwrap_err()
        .to_string();
    assert!(err.contains("signature verification failed"), "{err}");
}

#[test]
fn tampered_certificate_is_rejected() {
    let root = random_key();
    let signing = random_key();
    let kid = kid_for_public_bytes(&signing.verifying_key().to_bytes());
    let mut container = container_for(&root, &signing, &claims_for(&kid), "super-pro", 0, 0);
    let mut cert: KeyCert =
        serde_json::from_slice(&BASE64.decode(&container.key_cert).unwrap()).expect("cert parses");
    // Original certificate has not_after = 0; change it without re-signing.
    cert.not_after = 1;
    container.key_cert = BASE64.encode(serde_json::to_vec(&cert).unwrap());
    let err = verify_license_with_root(&encode(&container), &root.verifying_key())
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("certificate signature verification failed"),
        "{err}"
    );
}

#[test]
fn wrong_root_is_rejected() {
    let root = random_key();
    let other = random_key();
    let signing = random_key();
    let token = valid_license(&root, &signing);
    let err = verify_license_with_root(&token, &other.verifying_key())
        .unwrap_err()
        .to_string();
    assert!(err.contains("root key"), "{err}");
}

#[test]
fn wrong_product_is_rejected() {
    let root = random_key();
    let signing = random_key();
    let kid = kid_for_public_bytes(&signing.verifying_key().to_bytes());
    let container = container_for(&root, &signing, &claims_for(&kid), "other-product", 0, 0);
    let err = verify_license_with_root(&encode(&container), &root.verifying_key())
        .unwrap_err()
        .to_string();
    assert!(err.contains("not Super Pro"), "{err}");
}

#[test]
fn expired_certificate_is_rejected() {
    let root = random_key();
    let signing = random_key();
    let kid = kid_for_public_bytes(&signing.verifying_key().to_bytes());
    let container = container_for(
        &root,
        &signing,
        &claims_for(&kid),
        "super-pro",
        0,
        now().saturating_sub(10_000),
    );
    let err = verify_license_with_root(&encode(&container), &root.verifying_key())
        .unwrap_err()
        .to_string();
    assert!(err.contains("expired"), "{err}");
}

#[test]
fn not_yet_valid_certificate_is_rejected() {
    let root = random_key();
    let signing = random_key();
    let kid = kid_for_public_bytes(&signing.verifying_key().to_bytes());
    let container = container_for(
        &root,
        &signing,
        &claims_for(&kid),
        "super-pro",
        now().saturating_add(10_000),
        0,
    );
    let err = verify_license_with_root(&encode(&container), &root.verifying_key())
        .unwrap_err()
        .to_string();
    assert!(err.contains("not valid yet"), "{err}");
}

#[test]
fn claims_kid_must_match_certificate() {
    let root = random_key();
    let signing = random_key();
    let kid = kid_for_public_bytes(&signing.verifying_key().to_bytes());
    let mut claims = claims_for(&kid);
    claims.kid = Some("k_deadbeef".into());
    let container = container_for(&root, &signing, &claims, "super-pro", 0, 0);
    let err = verify_license_with_root(&encode(&container), &root.verifying_key())
        .unwrap_err()
        .to_string();
    assert!(err.contains("does not match"), "{err}");
}

#[test]
fn expired_license_still_loads_for_superd() {
    let root = random_key();
    let signing = random_key();
    let kid = kid_for_public_bytes(&signing.verifying_key().to_bytes());
    let mut claims = claims_for(&kid);
    claims.expires_at = Some(2);
    claims.retain_grants_after_expiry = Some(true);
    let container = container_for(&root, &signing, &claims, "super-pro", 0, 0);
    let (verified, status) =
        verify_license_for_superd_with_root(&encode(&container), &root.verifying_key())
            .expect("superd accepts expired");
    assert_eq!(status, LicenseExpiryStatus::Expired);
    assert_eq!(verified.issued_to, "test@example.com");
}

#[test]
fn superd_rejects_expired_when_claim_says_so() {
    let root = random_key();
    let signing = random_key();
    let kid = kid_for_public_bytes(&signing.verifying_key().to_bytes());
    let mut claims = claims_for(&kid);
    claims.expires_at = Some(2);
    claims.retain_grants_after_expiry = Some(false);
    let container = container_for(&root, &signing, &claims, "super-pro", 0, 0);
    assert!(
        verify_license_for_superd_with_root(&encode(&container), &root.verifying_key()).is_err()
    );
}

#[test]
fn strict_verify_rejects_expired_license() {
    let root = random_key();
    let signing = random_key();
    let kid = kid_for_public_bytes(&signing.verifying_key().to_bytes());
    let mut claims = claims_for(&kid);
    claims.expires_at = Some(2);
    let container = container_for(&root, &signing, &claims, "super-pro", 0, 0);
    assert!(verify_license_with_root(&encode(&container), &root.verifying_key()).is_err());
}

#[test]
fn missing_certificate_is_rejected() {
    let root = random_key();
    let signing = random_key();
    let kid = kid_for_public_bytes(&signing.verifying_key().to_bytes());
    let mut container = container_for(&root, &signing, &claims_for(&kid), "super-pro", 0, 0);
    container.key_cert = String::new();
    let err = verify_license_with_root(&encode(&container), &root.verifying_key())
        .unwrap_err()
        .to_string();
    assert!(err.contains("missing key certificate"), "{err}");
}

#[test]
fn legacy_container_format_is_rejected() {
    let legacy = serde_json::json!({
        "claims": { "issued_to": "old", "grants": ["security"] },
        "signature": [1, 2, 3]
    });
    let token = BASE64.encode(serde_json::to_vec(&legacy).unwrap());
    let root = random_key();
    let err = verify_license_with_root(&token, &root.verifying_key())
        .unwrap_err()
        .to_string();
    assert!(err.contains("re-issue"), "{err}");
}

#[test]
fn embedded_root_key_is_listed() {
    let keys = embedded_verifying_keys();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].kid, embedded_root_kid());
    assert_eq!(keys[0].fingerprint.len(), 8);
    assert!(!embedded_keyring_summary().is_empty());
    assert_eq!(embedded_verifying_key_ids(), vec![embedded_root_kid()]);
}

#[test]
fn keyring_summary_truncates_many_keys() {
    let kids = ["a", "b", "c", "d", "e"];
    assert_eq!(format_keyring_summary(&kids), "a, b, c (+2 more)");
    assert_eq!(format_keyring_summary(&["only"]), "only");
    assert_eq!(format_keyring_summary(&[] as &[&str]), "none");
}
