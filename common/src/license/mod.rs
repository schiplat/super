//! Runtime license verification for `superd` (OSS scope).
//!
//! Verifies cryptographically signed subscription keys and enforces signed claims at
//! runtime. Subscription key signing and plugin catalogs are out of scope
//! for this repository.
//!
//! Verifying keys: committed under `common/keys/*.public.key` (public material).
//! `build.rs` embeds them into `PUBLIC_KEY_RING`. Maintainers refresh with `make fetch-keys`.

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
    EmbeddedPublicKey, EmbeddedVerifyingKeyInfo, LICENSE_TRIAL_URL, LICENSE_UPGRADE_URL,
    LicenseExpiryStatus, PUBLIC_KEY_RING, check_superd_version, embedded_keyring_summary,
    embedded_verifying_key_ids, embedded_verifying_keys, format_keyring_summary,
    kid_for_public_bytes, license_expiry_status, license_help_footer, license_issued_for_version,
    license_max_superd_version, licensed_max_super_minor, licensed_minor_line,
    licensed_version_scope, parse_major_version, parse_semver, superd_within_license,
    verify_license, verify_license_for_superd, verify_license_with_key,
};
