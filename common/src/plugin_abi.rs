//! Stable C ABI between `superd` and `plugins/*.so` / `*.dylib`.

use std::ffi::CStr;

pub const PLUGIN_API_VERSION: u32 = 2;
pub const PLUGIN_SYMBOL: &[u8] = b"super_plugin_v1";

/// Returns a NUL-terminated semver string (typically `CARGO_PKG_VERSION`).
pub type PluginVersionFn = unsafe extern "C" fn() -> *const std::ffi::c_char;

/// Read the optional release version exported by a lifecycle plugin.
pub fn read_plugin_version(vtable: &SuperPluginV1) -> Option<String> {
    let version_fn = vtable.plugin_version?;
    // SAFETY: `version_fn` is a vtable entry point; the ABI contract is that
    // it returns null or a pointer to a NUL-terminated static string owned by
    // the plugin (valid for the library's loaded lifetime).
    unsafe {
        let ptr = version_fn();
        if ptr.is_null() {
            None
        } else {
            CStr::from_ptr(ptr).to_str().ok().map(str::to_string)
        }
    }
}

/// Host callbacks injected into plugins at `init` time (plugin → host channel).
///
/// The host passes a pointer to a `SuperPluginHostV1` as the sole argument of
/// the `init` callback. Plugins must copy any function pointers they need
/// during `init`; the table itself is only valid for the duration of the call.
#[repr(C)]
pub struct SuperPluginHostV1 {
    pub api_version: u32,
    /// Emit a JSON-encoded `SystemEvent` back into superd's event pipeline
    /// (same path as lifecycle events → hooks + notify). Returns 0 on success.
    pub emit_event: Option<unsafe extern "C" fn(*const std::ffi::c_char) -> i32>,
}

/// Plugin descriptor exported as `super_plugin_v1`.
#[repr(C)]
pub struct SuperPluginV1 {
    pub api_version: u32,
    /// Must match the library filename stem (e.g. `isolation`).
    pub plugin_id: *const std::ffi::c_char,
    /// One-time init. Receives the host callback table; return 0 on success.
    pub init: Option<unsafe extern "C" fn(*const SuperPluginHostV1) -> i32>,
    pub after_start:
        Option<unsafe extern "C" fn(*const std::ffi::c_char, u32, *const std::ffi::c_char) -> i32>,
    pub after_stop:
        Option<unsafe extern "C" fn(*const std::ffi::c_char, *const std::ffi::c_char) -> i32>,
    pub on_update: Option<
        unsafe extern "C" fn(
            *const std::ffi::c_char,
            u32,
            *const std::ffi::c_char,
            *const std::ffi::c_char,
        ) -> i32,
    >,
    /// Writes Prometheus text into `buf`; returns bytes written (excluding NUL).
    pub collect_metrics: Option<unsafe extern "C" fn(*mut std::ffi::c_char, usize) -> usize>,
    /// JSON-encoded `SystemEvent`.
    pub on_event: Option<unsafe extern "C" fn(*const std::ffi::c_char) -> i32>,
    pub on_reload: Option<unsafe extern "C" fn() -> i32>,
    /// Release semver (e.g. `1.2.0`), not the ABI `api_version`.
    pub plugin_version: Option<PluginVersionFn>,
}
