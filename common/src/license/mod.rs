//! Runtime license verification for `superd` (OSS scope).
//!
//! A license carries its signing-key certificate, certified by a root key
//! embedded in the binary. Rotating a signing key therefore does not require a
//! new `superd` build; only root rotation does.
//!
//! Root public keys are committed under `common/keys/root.public.key` (public
//! material). Signing private keys stay in Manager; this repository only
//! verifies.

mod claims;
mod deployment;
mod verify;

pub use claims::{LicenseClaims, LicenseInfo};
pub use deployment::{
    env_license_strict, license_degradation_refusal_message, licensed_deployment_intent,
    read_license_strict, resolve_license_strict, scan_plugin_stems,
    should_refuse_license_degradation,
};
pub use verify::{
    EmbeddedVerifyingKeyInfo, KEY_CERT_SIGNING_DOMAIN, KEY_CERT_VERSION, KeyCert,
    LICENSE_SIGNING_DOMAIN, LICENSE_TRIAL_URL, LICENSE_UPGRADE_URL, LicenseExpiryStatus,
    SignedKeyCert, check_superd_version, embedded_keyring_summary, embedded_verifying_key_ids,
    embedded_verifying_keys, format_keyring_summary, kid_for_public_bytes, license_expiry_status,
    license_help_footer, license_issued_for_version, license_max_superd_version,
    licensed_max_super_minor, licensed_minor_line, licensed_version_scope, parse_major_version,
    parse_semver, parse_signed_key_cert_unverified, superd_within_license, validate_key_cert_time,
    verify_license, verify_license_for_superd, verify_license_with_root, verify_signed_key_cert,
};
