//! Core auth gate selection: exactly one gate, with a deliberate precedence.

use super::AuthGate;

#[test]
fn plugin_always_wins() {
    assert_eq!(
        AuthGate::resolve(Some("secret"), true, true),
        AuthGate::Plugin
    );
    assert_eq!(AuthGate::resolve(None, true, false), AuthGate::Plugin);
}

#[test]
fn signet_beats_the_static_secret() {
    // Enabling SSO is what switches the long-lived shared secret off; leaving
    // it dormant in conf/super.toml is the break-glass fallback, not a second
    // simultaneous gate.
    assert_eq!(
        AuthGate::resolve(Some("secret"), false, true),
        AuthGate::Sso
    );
    assert_eq!(AuthGate::resolve(None, false, true), AuthGate::Sso);
}

#[test]
fn no_secret_means_open() {
    assert_eq!(AuthGate::resolve(None, false, false), AuthGate::None);
    assert_eq!(AuthGate::resolve(Some(""), false, false), AuthGate::None);
    assert_eq!(AuthGate::resolve(Some("   "), false, false), AuthGate::None);
    assert!(!AuthGate::None.requires_auth());
}

#[test]
fn non_empty_secret_activates() {
    assert_eq!(
        AuthGate::resolve(Some("my-secret"), false, false),
        AuthGate::CoreSecret
    );
    assert_eq!(
        AuthGate::resolve(Some("  padded  "), false, false),
        AuthGate::CoreSecret
    );
    assert!(AuthGate::CoreSecret.requires_auth());
}

#[test]
fn every_gate_but_none_requires_auth() {
    assert!(AuthGate::Plugin.requires_auth());
    assert!(AuthGate::Sso.requires_auth());
    assert!(AuthGate::CoreSecret.requires_auth());
    assert!(!AuthGate::None.requires_auth());
}
