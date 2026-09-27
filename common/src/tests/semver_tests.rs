use super::*;
use crate::license::claims::LicenseClaims;

#[test]
fn version_labels_are_explicit() {
    let claims = LicenseClaims {
        product_id: None,
        kid: None,
        issued_to: "t".into(),
        issued_at: 1,
        major_version: 1,
        minor_version: Some(2),
        max_super_minor: Some(4),
        minor_ahead: None,
        issued_super_version: Some("1.2.0".into()),
        grants: vec![],
        expires_at: None,
        retain_grants_after_expiry: None,
        license_id: None,
    };
    assert_eq!(
        license_issued_for_version(&claims).as_deref(),
        Some("1.2.0")
    );
    assert_eq!(license_max_superd_version(&claims), "1.4.x");
    assert!(superd_within_license(&claims, "1.2.0"));
    assert!(superd_within_license(&claims, "1.4.9"));
    assert!(!superd_within_license(&claims, "1.5.0"));
    assert!(!superd_within_license(&claims, "2.0.0"));
}

#[test]
fn parse_semver_works() {
    assert_eq!(parse_semver("1.2.0"), Some((1, 2, 0)));
    assert_eq!(parse_semver("2"), Some((2, 0, 0)));
}

#[test]
fn issued_super_version_still_parses_minor_for_display() {
    let claims = LicenseClaims {
        product_id: None,
        kid: None,
        issued_to: "t".into(),
        issued_at: 1,
        major_version: 1,
        minor_version: None,
        max_super_minor: Some(4),
        minor_ahead: None,
        issued_super_version: Some("1.2.0".into()),
        grants: vec![],
        expires_at: None,
        retain_grants_after_expiry: None,
        license_id: None,
    };
    assert_eq!(licensed_minor_line(&claims), Some(2));
    assert!(check_superd_version(&claims, "1.4.0").is_ok());
    assert!(check_superd_version(&claims, "1.5.0").is_err());
}

#[test]
fn max_super_minor_caps_upper_bound_only() {
    let claims = LicenseClaims {
        product_id: None,
        kid: None,
        issued_to: "t".into(),
        issued_at: 1,
        major_version: 1,
        minor_version: Some(2),
        max_super_minor: Some(4),
        minor_ahead: None,
        issued_super_version: Some("1.2.0".into()),
        grants: vec![],
        expires_at: None,
        retain_grants_after_expiry: None,
        license_id: None,
    };
    assert!(check_superd_version(&claims, "1.0.0").is_ok());
    assert!(check_superd_version(&claims, "1.4.0").is_ok());
    assert!(check_superd_version(&claims, "1.5.0").is_err());
    assert!(check_superd_version(&claims, "2.0.0").is_err());
}

#[test]
fn legacy_minor_ahead_delta_still_works() {
    let claims = LicenseClaims {
        product_id: None,
        kid: None,
        issued_to: "t".into(),
        issued_at: 1,
        major_version: 1,
        minor_version: Some(2),
        max_super_minor: None,
        minor_ahead: Some(2),
        issued_super_version: Some("1.2.0".into()),
        grants: vec![],
        expires_at: None,
        retain_grants_after_expiry: None,
        license_id: None,
    };
    assert_eq!(licensed_max_super_minor(&claims), Some(4));
    assert_eq!(license_max_superd_version(&claims), "1.4.x");
    assert!(check_superd_version(&claims, "1.4.9").is_ok());
    assert!(check_superd_version(&claims, "1.5.0").is_err());
}

#[test]
fn legacy_license_major_only() {
    let claims = LicenseClaims {
        product_id: None,
        kid: None,
        issued_to: "t".into(),
        issued_at: 1,
        major_version: 1,
        minor_version: None,
        max_super_minor: None,
        minor_ahead: None,
        issued_super_version: None,
        grants: vec![],
        expires_at: None,
        retain_grants_after_expiry: None,
        license_id: None,
    };
    assert_eq!(license_max_superd_version(&claims), "1.x");
    assert!(check_superd_version(&claims, "1.9.0").is_ok());
    assert!(check_superd_version(&claims, "2.0.0").is_err());
}
