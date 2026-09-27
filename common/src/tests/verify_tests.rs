use super::*;
use crate::license::claims::LicenseClaims;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;

fn sign_claims(signing_key: &SigningKey, claims: &LicenseClaims) -> String {
    let claims_bytes = serde_json::to_vec(claims).unwrap();
    let signature = signing_key.sign(&claims_bytes);
    let container = LicenseContainer {
        claims: claims.clone(),
        signature: signature.to_bytes().to_vec(),
    };
    BASE64.encode(serde_json::to_vec(&container).unwrap())
}

#[test]
fn parse_major_version_works() {
    assert_eq!(parse_major_version("1.1.9"), 1);
    assert_eq!(parse_major_version("2.0.0"), 2);
}

#[test]
fn expired_license_still_loads_for_superd() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let claims = LicenseClaims {
        product_id: None,
        kid: None,
        issued_to: "expired@example.com".into(),
        issued_at: 1,
        major_version: 1,
        minor_version: None,
        max_super_minor: None,
        minor_ahead: None,
        issued_super_version: None,
        grants: vec!["security".into()],
        expires_at: Some(2),
        retain_grants_after_expiry: Some(true),
        license_id: Some("lic-test".into()),
    };
    let token = sign_claims(&signing_key, &claims);
    let verifying_key = signing_key.verifying_key();
    let (verified, status) =
        verify_license_for_superd_with_key(&token, &verifying_key).expect("superd accepts expired");
    assert_eq!(status, LicenseExpiryStatus::Expired);
    assert_eq!(verified.issued_to, "expired@example.com");
}

#[test]
fn rejects_expired_license() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let claims = LicenseClaims {
        product_id: None,
        kid: None,
        issued_to: "expired@example.com".into(),
        issued_at: 1,
        major_version: 1,
        minor_version: None,
        max_super_minor: None,
        minor_ahead: None,
        issued_super_version: None,
        grants: vec!["security".into()],
        expires_at: Some(2),
        retain_grants_after_expiry: Some(true),
        license_id: Some("lic-test".into()),
    };
    let token = sign_claims(&signing_key, &claims);
    assert!(verify_license_with_key(&token, &verifying_key).is_err());
}

#[test]
fn verify_signature_requires_kid() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let claims = LicenseClaims {
        product_id: Some("super-pro".into()),
        kid: None,
        issued_to: "t@example.com".into(),
        issued_at: 1,
        major_version: 1,
        minor_version: None,
        max_super_minor: None,
        minor_ahead: None,
        issued_super_version: None,
        grants: vec!["security".into()],
        expires_at: None,
        retain_grants_after_expiry: None,
        license_id: None,
    };
    let token = sign_claims(&signing_key, &claims);
    let err = verify_license(&token).unwrap_err().to_string();
    assert!(err.contains("kid"), "{err}");
}

#[test]
fn claims_without_kid_still_verify_with_explicit_key() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let claims = LicenseClaims {
        product_id: None,
        kid: None,
        issued_to: "legacy".into(),
        issued_at: 1,
        major_version: 1,
        minor_version: None,
        max_super_minor: None,
        minor_ahead: None,
        issued_super_version: None,
        grants: vec!["security".into()],
        expires_at: None,
        retain_grants_after_expiry: None,
        license_id: None,
    };
    let token = sign_claims(&signing_key, &claims);
    verify_license_with_key(&token, &verifying_key).expect("legacy license should verify");
}

#[test]
fn superd_rejects_expired_when_claim_says_so() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let claims = LicenseClaims {
        product_id: None,
        kid: None,
        issued_to: "expired@example.com".into(),
        issued_at: 1,
        major_version: 1,
        minor_version: None,
        max_super_minor: None,
        minor_ahead: None,
        issued_super_version: None,
        grants: vec!["security".into()],
        expires_at: Some(2),
        retain_grants_after_expiry: Some(false),
        license_id: Some("lic-hard".into()),
    };
    let token = sign_claims(&signing_key, &claims);
    let verifying_key = signing_key.verifying_key();
    assert!(verify_license_for_superd_with_key(&token, &verifying_key).is_err());
}

#[test]
fn unknown_kid_suggests_upgrade_or_keep_license() {
    let signing_key = SigningKey::generate(&mut OsRng);
    let claims = LicenseClaims {
        product_id: Some("super-pro".into()),
        kid: Some("future-k".into()),
        issued_to: "newkey@example.com".into(),
        issued_at: 1,
        major_version: 1,
        minor_version: None,
        max_super_minor: None,
        minor_ahead: None,
        issued_super_version: None,
        grants: vec!["security".into()],
        expires_at: None,
        retain_grants_after_expiry: None,
        license_id: Some("lic-new".into()),
    };
    let token = sign_claims(&signing_key, &claims);
    let err = verify_license(&token).unwrap_err().to_string();
    assert!(err.contains("future-k"), "{err}");
    assert!(
        err.contains("Keep your current license") || err.contains("upgrade superd"),
        "{err}"
    );
}

#[test]
fn embedded_keyring_matches_public_key_ring() {
    let keys = embedded_verifying_keys();
    assert_eq!(keys.len(), PUBLIC_KEY_RING.len());
    for (info, entry) in keys.iter().zip(PUBLIC_KEY_RING.iter()) {
        assert_eq!(info.kid, entry.kid);
        assert_eq!(info.fingerprint.len(), 8);
    }
    assert!(!embedded_keyring_summary().is_empty());
}

#[test]
fn keyring_summary_truncates_many_keys() {
    let kids = ["a", "b", "c", "d", "e"];
    assert_eq!(format_keyring_summary(&kids), "a, b, c (+2 more)");
    assert_eq!(format_keyring_summary(&["only"]), "only");
    assert_eq!(format_keyring_summary(&[] as &[&str]), "none");
}
