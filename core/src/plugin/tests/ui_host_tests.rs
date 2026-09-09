//! Fail-closed tests for the bounded UI asset resolve (`resolve_bounded`).
//!
//! Uses fake FFI entry points shaped like `super_plugin_ui_v1` callbacks to
//! pin three behaviours: asset found, asset missing, and a plugin that hangs
//! past the budget — the last must collapse to `TimedOut` (host answers 504)
//! instead of pinning the caller forever.

use super::*;

// --- Fake FFI entry points ------------------------------------------------

/// Found: static mime + data, mirroring the embedded-assets contract
/// (pointers valid for the process lifetime).
unsafe extern "C" fn fake_resolve_found(
    _path: *const std::ffi::c_char,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
    out_mime: *mut *const std::ffi::c_char,
) -> i32 {
    static DATA: &[u8] = b"<html>ok</html>";
    static MIME: &[u8] = b"text/html\0";
    unsafe {
        *out_ptr = DATA.as_ptr();
        *out_len = DATA.len();
        *out_mime = MIME.as_ptr() as *const std::ffi::c_char;
    }
    0
}

/// Not found: null pointer out, non-zero code.
unsafe extern "C" fn fake_resolve_missing(
    _path: *const std::ffi::c_char,
    out_ptr: *mut *const u8,
    _out_len: *mut usize,
    _out_mime: *mut *const std::ffi::c_char,
) -> i32 {
    unsafe { *out_ptr = std::ptr::null() };
    1
}

/// Wedged: sleeps past the (overridden) budget, simulating a stuck plugin.
/// Keep the sleep short-ish: when the test runtime drops it waits for the
/// blocking task to finish, so this sleep bounds the test's wall time.
unsafe extern "C" fn fake_resolve_hang(
    _path: *const std::ffi::c_char,
    _out_ptr: *mut *const u8,
    _out_len: *mut usize,
    _out_mime: *mut *const std::ffi::c_char,
) -> i32 {
    std::thread::sleep(Duration::from_secs(2));
    1
}

// Note on panics: NOT testable here — a `panic!` inside an
// `unsafe extern "C"` fn aborts the process (unwinding out of extern "C" is
// UB; Rust turns it into a non-unwinding abort). This is precisely why
// `super-pro-shared::ffi_guard` wraps every plugin callback: the panic must
// be contained on the plugin side of the boundary. The host-side `JoinError`
// collapse arm in `resolve_bounded` remains as cheap defence in depth.

fn handle_for(f: common::plugin_ui_abi::UiResolveAssetFn) -> UiPluginHandle {
    UiPluginHandle {
        resolve_asset: f,
        build_id: None,
    }
}
/// RAII: shrink the resolve budget for the test, restore on drop.
struct TimeoutOverride;
impl TimeoutOverride {
    fn set_ms(ms: u64) -> Self {
        RESOLVE_TIMEOUT_MS_OVERRIDE.store(ms, std::sync::atomic::Ordering::Relaxed);
        TimeoutOverride
    }
}
impl Drop for TimeoutOverride {
    fn drop(&mut self) {
        RESOLVE_TIMEOUT_MS_OVERRIDE.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

// --- Tests ----------------------------------------------------------------

#[tokio::test]
async fn resolve_bounded_returns_asset() {
    let ui = handle_for(fake_resolve_found);
    match ui.resolve_bounded("index.html").await {
        UiResolveOutcome::Asset { mime, data } => {
            assert_eq!(mime, "text/html");
            assert_eq!(data, b"<html>ok</html>");
        }
        other => panic!("expected asset, got {:?}", discriminant(&other)),
    }
}

#[tokio::test]
async fn resolve_bounded_maps_missing() {
    let ui = handle_for(fake_resolve_missing);
    assert!(matches!(
        ui.resolve_bounded("nope.js").await,
        UiResolveOutcome::Missing
    ));
}

#[tokio::test]
async fn resolve_bounded_timeout_fails_closed() {
    let _guard = TimeoutOverride::set_ms(50);
    let ui = handle_for(fake_resolve_hang);
    let started = std::time::Instant::now();
    assert!(matches!(
        ui.resolve_bounded("index.html").await,
        UiResolveOutcome::TimedOut
    ));
    // Must return at the budget (~50 ms), not after the plugin's 2 s sleep.
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn normalize_root_to_index() {
    assert_eq!(normalize_ui_path("/"), "index.html");
    assert_eq!(normalize_ui_path(""), "index.html");
    assert_eq!(normalize_ui_path("/assets/app.js"), "assets/app.js");
    assert_eq!(normalize_ui_path("/../etc/passwd"), "index.html");
}

/// Helper so `panic!` on unexpected variants formats a readable message
/// without requiring `Debug` on the (pointer-bearing) outcome enum.
fn discriminant(outcome: &UiResolveOutcome) -> &'static str {
    match outcome {
        UiResolveOutcome::Asset { .. } => "Asset",
        UiResolveOutcome::Missing => "Missing",
        UiResolveOutcome::TimedOut => "TimedOut",
    }
}
