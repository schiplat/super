//! `http_host` fail-closed tests (kept out of http_host.rs).
//!
//! Pinned contract: every abnormal plugin answer (misconfigured code 2,
//! garbage context, missing hook, unparseable JSON) must fail closed —
//! 4xx/5xx, and never an injected `UserContext`.

use super::*;
use std::sync::Arc as StdArc;
use std::sync::atomic::{AtomicU8, Ordering};
use tower::ServiceExt as _;

/// Sentinel values for the downstream `UserContext` probe:
/// 2 = not seen (handler never ran), 1 = extension present, 0 = absent.
const NOT_SEEN: u8 = 2;
const HAS_USER: u8 = 1;
const NO_USER: u8 = 0;

// ---- fake plugin authenticate implementations (extern "C") ----

/// Misconfigured / panicked plugin: return code 2, no context written.
unsafe extern "C" fn fake_auth_misconfigured(
    _path: *const std::ffi::c_char,
    _authorization: *const std::ffi::c_char,
    _query: *const std::ffi::c_char,
    _out_ctx_json: *mut std::ffi::c_char,
    _out_len: usize,
) -> i32 {
    2
}

/// Code 0 but writes non-UTF-8 bytes into the context buffer.
unsafe extern "C" fn fake_auth_garbage_ctx(
    _path: *const std::ffi::c_char,
    _authorization: *const std::ffi::c_char,
    _query: *const std::ffi::c_char,
    out_ctx_json: *mut std::ffi::c_char,
    _out_len: usize,
) -> i32 {
    // SAFETY: host always passes a valid buffer of at least 2048 bytes.
    unsafe {
        std::ptr::copy_nonoverlapping([0xFF, 0xFE].as_ptr(), out_ctx_json.cast(), 2);
    }
    0
}

/// Code 0 but leaves the buffer untouched (all zeros = empty context).
unsafe extern "C" fn fake_auth_empty_ctx(
    _path: *const std::ffi::c_char,
    _authorization: *const std::ffi::c_char,
    _query: *const std::ffi::c_char,
    _out_ctx_json: *mut std::ffi::c_char,
    _out_len: usize,
) -> i32 {
    0
}

/// Code 0 with a syntactically valid but semantically wrong JSON object.
unsafe extern "C" fn fake_auth_junk_json(
    _path: *const std::ffi::c_char,
    _authorization: *const std::ffi::c_char,
    _query: *const std::ffi::c_char,
    out_ctx_json: *mut std::ffi::c_char,
    _out_len: usize,
) -> i32 {
    let junk = b"{\"totally\":\"unrelated\"}";
    // SAFETY: host always passes a valid buffer of at least 2048 bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(junk.as_ptr(), out_ctx_json.cast(), junk.len());
        *out_ctx_json.add(junk.len()) = 0;
    }
    0
}

unsafe extern "C" fn test_handle_api_always_500(
    _method: *const std::ffi::c_char,
    _path: *const std::ffi::c_char,
    _body: *const std::ffi::c_char,
    _ctx_json: *const std::ffi::c_char,
    _out: *mut std::ffi::c_char,
    _out_len: usize,
) -> u32 {
    500
}

/// Always-allow RBAC (mirrors a plugin that authorizes by context only).
unsafe extern "C" fn test_authorize_allow(
    _path: *const std::ffi::c_char,
    _method: *const std::ffi::c_char,
    _ctx_json: *const std::ffi::c_char,
) -> i32 {
    0
}

/// No-op audit sink.
unsafe extern "C" fn test_audit_noop(
    _ctx_json: *const std::ffi::c_char,
    _method: *const std::ffi::c_char,
    _path: *const std::ffi::c_char,
    _status: u16,
    _client_ip: *const std::ffi::c_char,
) {
}

fn ok_handle(authenticate: common::plugin_http_abi::HttpAuthFn) -> Arc<HttpPluginHandle> {
    Arc::new(HttpPluginHandle {
        init: None,
        authenticate: Some(authenticate),
        authorize: Some(test_authorize_allow),
        audit_request: Some(test_audit_noop),
        handle_api: test_handle_api_always_500,
    })
}

/// Build the middleware chain exactly like `attach_http_plugins` does
/// (rbac -> audit -> auth, auth outermost), with the probe innermost.
/// The probe records whether a `UserContext` reached past the auth chain.
fn probe_router(
    handle: Arc<HttpPluginHandle>,
    seen: StdArc<AtomicU8>,
) -> axum::routing::RouterIntoService<axum::body::Body, ()> {
    Router::new()
        .route("/probe", axum::routing::any(|| async { StatusCode::OK }))
        .layer(axum::middleware::from_fn(
            move |req: axum::extract::Request, next: Next| {
                let seen = seen.clone();
                async move {
                    let has_user = req.extensions().get::<UserContext>().is_some();
                    seen.store(if has_user { HAS_USER } else { NO_USER }, Ordering::SeqCst);
                    next.run(req).await
                }
            },
        ))
        .layer(axum::middleware::from_fn_with_state(
            handle.clone(),
            plugin_rbac_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            handle.clone(),
            plugin_audit_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            handle,
            plugin_auth_middleware,
        ))
        .into_service()
}

async fn probe_request(
    router: axum::routing::RouterIntoService<axum::body::Body, ()>,
) -> anyhow::Result<Response> {
    let req = axum::extract::Request::builder()
        .method(Method::GET)
        .uri("/probe")
        .body(axum::body::Body::empty())
        .unwrap();
    Ok(router.oneshot(req).await?)
}

#[tokio::test]
async fn auth_misconfigured_plugin_maps_to_500() {
    // The host-side half of the `ffi_guard::AUTH_MISCONFIGURED` contract:
    // plugin code 2 must never pass the request through.
    let seen = StdArc::new(AtomicU8::new(NOT_SEEN));
    let router = probe_router(ok_handle(fake_auth_misconfigured), seen.clone());
    let resp = probe_request(router).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(seen.load(Ordering::SeqCst), NOT_SEEN);
}

#[tokio::test]
async fn auth_garbage_context_maps_to_500() {
    let seen = StdArc::new(AtomicU8::new(NOT_SEEN));
    let router = probe_router(ok_handle(fake_auth_garbage_ctx), seen.clone());
    let resp = probe_request(router).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(seen.load(Ordering::SeqCst), NOT_SEEN);
}

#[tokio::test]
async fn auth_missing_hook_maps_to_500() {
    let handle = Arc::new(HttpPluginHandle {
        init: None,
        authenticate: None,
        authorize: None,
        audit_request: None,
        handle_api: test_handle_api_always_500,
    });
    let seen = StdArc::new(AtomicU8::new(NOT_SEEN));
    let router = probe_router(handle, seen.clone());
    let resp = probe_request(router).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(seen.load(Ordering::SeqCst), NOT_SEEN);
}

/// Valid-UTF-8 JSON the host cannot parse into `UserContext` passes the
/// request through (RBAC/dispatch layers reject it downstream) but must
/// never inject an authenticated identity.
#[tokio::test]
async fn auth_junk_json_injects_no_user_context() {
    let seen = StdArc::new(AtomicU8::new(NOT_SEEN));
    let router = probe_router(ok_handle(fake_auth_junk_json), seen.clone());
    let resp = probe_request(router).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        seen.load(Ordering::SeqCst),
        NO_USER,
        "unparseable ctx JSON must not yield a UserContext"
    );
}

/// Code 0 with an untouched buffer is a plugin contract violation; it must
/// likewise produce no `UserContext` (empty string parses to nothing).
#[tokio::test]
async fn auth_empty_ctx_injects_no_user_context() {
    let seen = StdArc::new(AtomicU8::new(NOT_SEEN));
    let router = probe_router(ok_handle(fake_auth_empty_ctx), seen.clone());
    let resp = probe_request(router).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(seen.load(Ordering::SeqCst), NO_USER);
}

/// Plugin claims success (code 0) but reports an out-of-range HTTP status
/// (e.g. `u32` written as signed success-zero or a bogus 4-digit code):
/// `from_u16` accepts 100..=999 and rejects 0, so this must surface as 500.
#[tokio::test]
async fn api_response_maps_invalid_status_to_500() {
    let resp = api_response(0, String::new());
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ---- bounded FFI (timeout) fail-closed tests ----

/// RAII guard restoring the test-only timeout override, so a panicking
/// assertion cannot leak a short timeout into other tests.
struct TimeoutOverride;

impl TimeoutOverride {
    fn ms(ms: u64) -> Self {
        FFI_TIMEOUT_MS_OVERRIDE.store(ms, Ordering::SeqCst);
        Self
    }
}

impl Drop for TimeoutOverride {
    fn drop(&mut self) {
        FFI_TIMEOUT_MS_OVERRIDE.store(0, Ordering::SeqCst);
    }
}

/// An `authenticate` hook that sleeps well past the (overridden) timeout.
unsafe extern "C" fn fake_auth_hangs(
    _path: *const std::ffi::c_char,
    _authorization: *const std::ffi::c_char,
    _query: *const std::ffi::c_char,
    _out_ctx_json: *mut std::ffi::c_char,
    _out_len: usize,
) -> i32 {
    std::thread::sleep(std::time::Duration::from_secs(5));
    0
}

/// A hung authenticate hook must fail closed with 504 and never inject a
/// `UserContext` — the request must not pass while the plugin is stuck.
#[tokio::test]
async fn auth_timeout_fails_closed_with_504() {
    let _guard = TimeoutOverride::ms(30);
    let seen = StdArc::new(AtomicU8::new(NOT_SEEN));
    let router = probe_router(ok_handle(fake_auth_hangs), seen.clone());
    let resp = probe_request(router).await.unwrap();
    assert_eq!(resp.status(), StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(seen.load(Ordering::SeqCst), NOT_SEEN);
}

/// A hung authorize hook must not fail open: the timeout maps to 500 and the
/// request never reaches the handler.
#[tokio::test]
async fn rbac_timeout_fails_closed() {
    unsafe extern "C" fn fake_authorize_hangs(
        _path: *const std::ffi::c_char,
        _method: *const std::ffi::c_char,
        _ctx_json: *const std::ffi::c_char,
    ) -> i32 {
        std::thread::sleep(std::time::Duration::from_secs(5));
        0
    }
    let _guard = TimeoutOverride::ms(30);
    let handle = Arc::new(HttpPluginHandle {
        init: None,
        authenticate: Some(fake_auth_empty_ctx),
        authorize: Some(fake_authorize_hangs),
        audit_request: Some(test_audit_noop),
        handle_api: test_handle_api_always_500,
    });
    let seen = StdArc::new(AtomicU8::new(NOT_SEEN));
    let router = probe_router(handle, seen.clone());
    let resp = probe_request(router).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(seen.load(Ordering::SeqCst), NOT_SEEN);
}

/// A hung `handle_api` must surface as 504 from the dispatch handler.
#[tokio::test]
async fn api_timeout_maps_to_504() {
    unsafe extern "C" fn fake_handle_api_hangs(
        _method: *const std::ffi::c_char,
        _path: *const std::ffi::c_char,
        _body: *const std::ffi::c_char,
        _ctx_json: *const std::ffi::c_char,
        _out: *mut std::ffi::c_char,
        _out_len: usize,
    ) -> u32 {
        std::thread::sleep(std::time::Duration::from_secs(5));
        200
    }
    let _guard = TimeoutOverride::ms(30);
    let handle = Arc::new(HttpPluginHandle {
        init: None,
        authenticate: None,
        authorize: None,
        audit_request: None,
        handle_api: fake_handle_api_hangs,
    });
    // Timeout is signalled as `None` (the dispatch handler maps it to 504).
    assert!(
        handle.call_api("GET", "/probe", "", "").await.is_none(),
        "hung handle_api must surface as None (timeout)"
    );
}
