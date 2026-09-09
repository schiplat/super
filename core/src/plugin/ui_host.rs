//! Static UI bridge for the optional `ui` plugin (`super_plugin_ui_v1`).

use crate::plugin::loader::PluginRuntime;
use common::plugin_ui_abi::{SuperPluginUiV1, UI_PLUGIN_API_VERSION, UI_PLUGIN_SYMBOL};
use libloading::Library;
use std::ffi::{CStr, CString};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

/// Wall-clock budget for one UI asset resolve FFI call. Mirrors the HTTP
/// host's `PLUGIN_FFI_TIMEOUT`: a plugin that never returns must not pin a
/// core async worker thread — the resolve runs on the blocking pool and this
/// budget bounds how long a request can wait on it.
const RESOLVE_FFI_TIMEOUT: Duration = Duration::from_secs(10);

/// Test-only override for the resolve budget (milliseconds); 0 = use default.
static RESOLVE_TIMEOUT_MS_OVERRIDE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

fn resolve_timeout() -> Duration {
    use std::sync::atomic::Ordering;
    match RESOLVE_TIMEOUT_MS_OVERRIDE.load(Ordering::Relaxed) {
        0 => RESOLVE_FFI_TIMEOUT,
        ms => Duration::from_millis(ms),
    }
}

/// Outcome of a bounded UI asset resolve.
pub enum UiResolveOutcome {
    /// Asset found; owned copies of the plugin's static mime/data bytes.
    Asset { mime: String, data: Vec<u8> },
    /// Plugin answered "not found" / invalid arguments.
    Missing,
    /// The plugin did not answer within the budget (or the call panicked).
    /// Fail-closed: callers answer 504, never fall back to SPA routing.
    TimedOut,
}

#[derive(Clone)]
pub struct UiPluginHandle {
    resolve_asset: common::plugin_ui_abi::UiResolveAssetFn,
    build_id: Option<common::plugin_ui_abi::UiBuildIdFn>,
}

pub struct UiAsset<'a> {
    pub data: &'a [u8],
    pub mime: &'a str,
}

impl UiPluginHandle {
    pub fn build_id(&self) -> Option<String> {
        let build_id = self.build_id?;
        // SAFETY: `build_id` is a checked vtable entry point; the ABI contract
        // is that it returns null or a pointer to a NUL-terminated static
        // string owned by the plugin (valid for the library's loaded lifetime).
        unsafe {
            let ptr = build_id();
            if ptr.is_null() {
                None
            } else {
                CStr::from_ptr(ptr).to_str().ok().map(str::to_string)
            }
        }
    }

    pub fn resolve(&self, path: &str) -> Option<UiAsset<'_>> {
        let path_c = CString::new(path).ok()?;
        let mut ptr: *const u8 = std::ptr::null();
        let mut len: usize = 0;
        let mut mime_ptr: *const std::ffi::c_char = std::ptr::null();

        // SAFETY: `path_c` is a valid NUL-terminated `CString` and the three
        // out-pointers are valid for writes for the duration of the call;
        // `resolve_asset` is a checked vtable entry point.
        let code =
            unsafe { (self.resolve_asset)(path_c.as_ptr(), &mut ptr, &mut len, &mut mime_ptr) };

        if code != 0 || ptr.is_null() || len == 0 {
            return None;
        }

        let mime = if mime_ptr.is_null() {
            "application/octet-stream"
        } else {
            // SAFETY: plugin returns a NUL-terminated static string.
            unsafe {
                CStr::from_ptr(mime_ptr)
                    .to_str()
                    .unwrap_or("application/octet-stream")
            }
        };

        // SAFETY: pointer/length refer to read-only embedded data in the loaded plugin
        // library; valid until the library is unloaded (never during superd lifetime).
        let data = unsafe { std::slice::from_raw_parts(ptr, len) };
        Some(UiAsset { data, mime })
    }

    /// Bounded resolve for the request path: runs the synchronous FFI on the
    /// blocking pool under a wall-clock budget, copying the plugin's static
    /// bytes out so nothing points into plugin memory after the call.
    ///
    /// The unbounded [`Self::resolve`] stays for non-request uses where a
    /// borrowed view is preferred (none today; `build_id` is startup-only).
    pub async fn resolve_bounded(&self, path: &str) -> UiResolveOutcome {
        let path_c = match CString::new(path) {
            Ok(p) => p,
            Err(_) => return UiResolveOutcome::Missing,
        };
        let resolve_asset = self.resolve_asset;

        let attempt = tokio::time::timeout(
            resolve_timeout(),
            tokio::task::spawn_blocking(move || {
                let mut ptr: *const u8 = std::ptr::null();
                let mut len: usize = 0;
                let mut mime_ptr: *const std::ffi::c_char = std::ptr::null();
                // SAFETY: `path_c` is a valid NUL-terminated `CString` and the
                // three out-pointers are valid for writes for the duration of
                // the call; `resolve_asset` is a checked vtable entry point.
                let code =
                    unsafe { (resolve_asset)(path_c.as_ptr(), &mut ptr, &mut len, &mut mime_ptr) };
                if code != 0 || ptr.is_null() || len == 0 {
                    return UiResolveOutcome::Missing;
                }
                let mime = if mime_ptr.is_null() {
                    "application/octet-stream".to_string()
                } else {
                    // SAFETY: plugin returns a NUL-terminated static string.
                    let s = unsafe { CStr::from_ptr(mime_ptr) };
                    s.to_str().unwrap_or("application/octet-stream").to_string()
                };
                // SAFETY: pointer/length refer to read-only embedded data in
                // the loaded plugin library; valid for the process lifetime.
                let data = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
                UiResolveOutcome::Asset { mime, data }
            }),
        )
        .await;

        match attempt {
            Ok(Ok(outcome)) => outcome,
            // Timeout, JoinError (panic), or a plugin answering garbage is
            // collapsed here; callers fail closed with 504.
            _ => UiResolveOutcome::TimedOut,
        }
    }
}

fn load_ui_vtable(library: &Library) -> Option<SuperPluginUiV1> {
    // SAFETY: symbol must match `SuperPluginUiV1` ABI when present.
    unsafe {
        let symbol: Result<libloading::Symbol<unsafe extern "C" fn() -> SuperPluginUiV1>, _> =
            library.get(UI_PLUGIN_SYMBOL);
        symbol.ok().map(|s| s())
    }
}

/// Load the UI plugin vtable when the `ui` plugin is authorized and loaded.
pub fn load_ui_plugin(runtime: &PluginRuntime) -> Option<Arc<UiPluginHandle>> {
    if !runtime.loaded_ids.iter().any(|id| id == "ui") {
        return None;
    }

    let library = runtime.library("ui")?;
    let vtable = load_ui_vtable(library)?;

    if vtable.api_version != UI_PLUGIN_API_VERSION {
        tracing::warn!(
            "UI plugin API version {} != host {}; UI disabled",
            vtable.api_version,
            UI_PLUGIN_API_VERSION
        );
        return None;
    }

    let resolve_asset = vtable.resolve_asset?;

    if let Some(build_id_fn) = vtable.build_id {
        // SAFETY: `build_id_fn` is a checked vtable entry point; it returns
        // null or a pointer to a NUL-terminated static string owned by the
        // plugin (valid for the library's loaded lifetime).
        let id = unsafe {
            let ptr = build_id_fn();
            if ptr.is_null() {
                None
            } else {
                CStr::from_ptr(ptr).to_str().ok().map(str::to_string)
            }
        };
        if let Some(id) = id {
            info!("UI plugin loaded (build {})", id);
        } else {
            info!("UI plugin loaded");
        }
    } else {
        info!("UI plugin loaded");
    }

    Some(Arc::new(UiPluginHandle {
        resolve_asset,
        build_id: vtable.build_id,
    }))
}

/// Normalize a request path into a plugin asset key (`index.html`, `assets/app.js`, …).
pub fn normalize_ui_path(uri_path: &str) -> String {
    common::security::sanitize_ui_asset_path(uri_path).unwrap_or_else(|| "index.html".to_string())
}

#[cfg(test)]
#[path = "tests/ui_host_tests.rs"]
mod ui_host_tests;
