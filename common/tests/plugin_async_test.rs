//! `common::plugin_async` worker pool behaviour (crate-level integration test).

use common::plugin_async::block_on;

#[test]
fn block_on_returns_value() {
    assert_eq!(block_on("test-ok", async { 41 + 1 }), Some(42));
}

/// A panicking future must not kill the worker thread: subsequent
/// `block_on` calls must still complete instead of hanging forever.
#[test]
fn block_on_survives_future_panic() {
    assert!(block_on("test-panic", async { panic!("boom") }).is_none());
    // The same pool must still be usable after the panic above.
    assert_eq!(
        block_on("test-panic", async { "still-alive" }),
        Some("still-alive")
    );
}
