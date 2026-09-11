//! Core auth policy helpers (activate / bind matrix).

use super::core_auth_should_activate;

#[test]
fn plugin_always_wins() {
    assert!(!core_auth_should_activate("0.0.0.0", false, true, true));
    assert!(!core_auth_should_activate("127.0.0.1", false, true, true));
}

#[test]
fn loopback_default_is_open() {
    assert!(!core_auth_should_activate("127.0.0.1", false, false, false));
    assert!(!core_auth_should_activate("::1", false, false, false));
}

#[test]
fn loopback_honors_auth_required_flag() {
    assert!(core_auth_should_activate("127.0.0.1", false, true, false));
}

#[test]
fn non_loopback_always_requires_auth() {
    assert!(core_auth_should_activate("0.0.0.0", false, false, false));
    assert!(core_auth_should_activate(
        "192.168.1.10",
        false,
        false,
        false
    ));
}

#[test]
fn socket_only_stays_open_without_flag() {
    assert!(!core_auth_should_activate("0.0.0.0", true, false, false));
    assert!(core_auth_should_activate("0.0.0.0", true, true, false));
}
