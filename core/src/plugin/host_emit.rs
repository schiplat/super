//! Plugin → host event bridge.
//!
//! Plugin cdylibs run on their own Tokio runtime and cannot call back into the
//! daemon through the plugin vtable. superd therefore injects a
//! `SuperPluginHostV1` table into each plugin's `init` call; the isolation
//! plugin uses `emit_event` to report `memory_pressure` / `memory_oom_kill`
//! back into the same event pipeline as lifecycle events.

use crate::event_hooks;
use crate::extension::Extension;
use common::SystemEvent;
use common::config::EventHookConfig;
use common::plugin_abi::{PLUGIN_API_VERSION, SuperPluginHostV1};
use std::ffi::CStr;
use std::sync::{Arc, OnceLock};

/// The daemon-side event pipeline reachable from plugin-emitted events.
struct EventPipeline {
    extension: Arc<dyn Extension>,
    hooks: Vec<EventHookConfig>,
    /// Host Tokio runtime, captured at install time so plugin threads can hand
    /// events over without running on the plugin's own worker runtime.
    runtime: tokio::runtime::Handle,
}

static PIPELINE: OnceLock<EventPipeline> = OnceLock::new();

/// Register the live event pipeline. Called once from `Manager::new` while the
/// host Tokio runtime is current; later calls are ignored.
pub fn install(extension: Arc<dyn Extension>, hooks: Vec<EventHookConfig>) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let _ = PIPELINE.set(EventPipeline {
            extension,
            hooks,
            runtime: handle,
        });
    }
}

/// The host table handed to each plugin's `init` callback.
pub fn host_vtable() -> SuperPluginHostV1 {
    SuperPluginHostV1 {
        api_version: PLUGIN_API_VERSION,
        emit_event: Some(emit_event),
    }
}

/// ABI entry: JSON-encoded `SystemEvent` from a plugin thread.
///
/// The caller runs on the plugin's own Tokio worker, so the event is scheduled
/// onto the host runtime (never blocked on) before dispatch.
unsafe extern "C" fn emit_event(json_ptr: *const std::ffi::c_char) -> i32 {
    let Some(pipeline) = PIPELINE.get() else {
        return 1;
    };
    if json_ptr.is_null() {
        return 1;
    }
    let Ok(json) = (unsafe { CStr::from_ptr(json_ptr) }).to_str() else {
        return 1;
    };
    let Ok(event) = serde_json::from_str::<SystemEvent>(json) else {
        return 1;
    };
    let extension = pipeline.extension.clone();
    let hooks = pipeline.hooks.clone();
    pipeline.runtime.spawn(async move {
        event_hooks::emit(&extension, &hooks, event);
    });
    0
}
