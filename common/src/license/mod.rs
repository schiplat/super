//! Runtime license verification for `superd` (OSS scope).
//!
//! Verifies cryptographically signed subscription keys and enforces signed claims at
//! runtime. Subscription key signing and plugin catalogs are out of scope
//! for this repository.
//!
//! Verifying keys: committed under `common/keys/*.public.key` (public material).
//! `build.rs` embeds them into `PUBLIC_KEY_RING`. Maintainers refresh with `make fetch-keys`.

mod claims;
mod verify;

pub use claims::{LicenseClaims, LicenseInfo};
pub use verify::{
    DEFAULT_LICENSE_KID, EmbeddedPublicKey, LICENSE_UPGRADE_URL, LicenseExpiryStatus,
    PUBLIC_KEY_BYTES, PUBLIC_KEY_RING, check_superd_version, license_expiry_status,
    license_issued_for_version, license_max_superd_version, licensed_max_super_minor,
    licensed_minor_line, licensed_version_scope, parse_major_version, parse_semver,
    superd_within_license, verify_license, verify_license_for_superd, verify_license_with_key,
};
