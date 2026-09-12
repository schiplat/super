//! Core auth policy: activate only when `auth_secret` is set (unless plugin auth).

use super::core_auth_should_activate;

#[test]
fn plugin_always_wins() {
    assert!(!core_auth_should_activate(Some("secret"), true));
    assert!(!core_auth_should_activate(None, true));
}

#[test]
fn no_secret_means_open() {
    assert!(!core_auth_should_activate(None, false));
    assert!(!core_auth_should_activate(Some(""), false));
    assert!(!core_auth_should_activate(Some("   "), false));
}

#[test]
fn non_empty_secret_activates() {
    assert!(core_auth_should_activate(Some("my-secret"), false));
    assert!(core_auth_should_activate(Some("  padded  "), false));
}
