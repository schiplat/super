use crate::extension::Extension;
use common::plugin_abi::SuperPluginV1;
use common::{ProgramConfig, SystemEvent, take_last_plugin_error};
use std::ffi::CStr;
use uuid::Uuid;

type AfterStartFn =
    unsafe extern "C" fn(*const std::ffi::c_char, u32, *const std::ffi::c_char) -> i32;
type AfterStopFn = unsafe extern "C" fn(*const std::ffi::c_char, *const std::ffi::c_char) -> i32;
type OnUpdateFn = unsafe extern "C" fn(
    *const std::ffi::c_char,
    u32,
    *const std::ffi::c_char,
    *const std::ffi::c_char,
) -> i32;
type CollectMetricsFn = unsafe extern "C" fn(*mut std::ffi::c_char, usize) -> usize;
type OnEventFn = unsafe extern "C" fn(*const std::ffi::c_char) -> i32;
type OnReloadFn = unsafe extern "C" fn() -> i32;

fn plugin_hook_error(plugin: &str, hook: &str, code: i32) -> anyhow::Error {
    let detail = take_last_plugin_error().unwrap_or_else(|| format!("exit code {code}"));
    anyhow::anyhow!("plugin '{plugin}' {hook} failed: {detail}")
}

/// Adapts a loaded plugin vtable into the in-process `Extension` trait.
pub struct PluginExtensionAdapter {
    name: String,
    after_start: Option<AfterStartFn>,
    after_stop: Option<AfterStopFn>,
    on_update: Option<OnUpdateFn>,
    collect_metrics: Option<CollectMetricsFn>,
    on_event: Option<OnEventFn>,
    on_reload: Option<OnReloadFn>,
}

impl PluginExtensionAdapter {
    pub fn new(expected_id: &str, vtable: SuperPluginV1) -> anyhow::Result<Self> {
        if vtable.api_version != common::plugin_abi::PLUGIN_API_VERSION {
            anyhow::bail!(
                "plugin API version {} != host {}",
                vtable.api_version,
                common::plugin_abi::PLUGIN_API_VERSION
            );
        }

        // SAFETY: `vtable.plugin_id` is null-checked above. The plugin ABI
        // requires it to point to a valid NUL-terminated C string owned by the
        // plugin, which outlives this call (the library stays loaded).
        let plugin_id = unsafe {
            if vtable.plugin_id.is_null() {
                anyhow::bail!("plugin_id is null");
            }
            CStr::from_ptr(vtable.plugin_id)
                .to_str()
                .map_err(|_| anyhow::anyhow!("plugin_id is not valid UTF-8"))?
                .to_string()
        };

        if plugin_id != expected_id {
            anyhow::bail!(
                "plugin id mismatch: file '{}' but library reports '{}'",
                expected_id,
                plugin_id
            );
        }

        if let Some(init) = vtable.init {
            let host = crate::plugin::host_emit::host_vtable();
            // SAFETY: `init` is a valid function pointer from the plugin's
            // `SuperPluginV1` vtable, whose `api_version` was checked above.
            // The ABI contract is a synchronous `extern "C" fn(*const
            // SuperPluginHostV1) -> i32`; the host table outlives the call and
            // the plugin copies any callback pointers it needs.
            let code = unsafe { init(&host) };
            if code != 0 {
                return Err(plugin_hook_error(&plugin_id, "init", code));
            }
        }

        Ok(Self {
            name: plugin_id,
            after_start: vtable.after_start,
            after_stop: vtable.after_stop,
            on_update: vtable.on_update,
            collect_metrics: vtable.collect_metrics,
            on_event: vtable.on_event,
            on_reload: vtable.on_reload,
        })
    }
}

impl Extension for PluginExtensionAdapter {
    fn after_start(&self, id: Uuid, pid: u32, config: &ProgramConfig) -> anyhow::Result<()> {
        let Some(hook) = self.after_start else {
            return Ok(());
        };
        let id_c = id.to_string();
        let config_json = serde_json::to_string(config)?;
        let id_ptr = std::ffi::CString::new(id_c)?;
        let cfg_ptr = std::ffi::CString::new(config_json)?;
        // SAFETY: both pointers are valid, NUL-terminated `CString`s that
        // outlive the call; `hook` is a checked vtable entry point.
        let code = unsafe { hook(id_ptr.as_ptr(), pid, cfg_ptr.as_ptr()) };
        if code != 0 {
            return Err(plugin_hook_error(&self.name, "after_start", code));
        }
        Ok(())
    }

    fn after_stop(&self, id: Uuid, config: &ProgramConfig) -> anyhow::Result<()> {
        let Some(hook) = self.after_stop else {
            return Ok(());
        };
        let id_c = id.to_string();
        let config_json = serde_json::to_string(config)?;
        let id_ptr = std::ffi::CString::new(id_c)?;
        let cfg_ptr = std::ffi::CString::new(config_json)?;
        // SAFETY: both pointers are valid, NUL-terminated `CString`s that
        // outlive the call; `hook` is a checked vtable entry point.
        let code = unsafe { hook(id_ptr.as_ptr(), cfg_ptr.as_ptr()) };
        if code != 0 {
            return Err(plugin_hook_error(&self.name, "after_stop", code));
        }
        Ok(())
    }

    fn on_update(
        &self,
        id: Uuid,
        pid: Option<u32>,
        old_config: &ProgramConfig,
        new_config: &ProgramConfig,
    ) -> anyhow::Result<()> {
        let Some(hook) = self.on_update else {
            return Ok(());
        };
        let pid_val = pid.unwrap_or(0);
        let id_c = id.to_string();
        let old_json = serde_json::to_string(old_config)?;
        let new_json = serde_json::to_string(new_config)?;
        let id_ptr = std::ffi::CString::new(id_c)?;
        let old_ptr = std::ffi::CString::new(old_json)?;
        let new_ptr = std::ffi::CString::new(new_json)?;
        // SAFETY: all three pointers are valid, NUL-terminated `CString`s that
        // outlive the call; `hook` is a checked vtable entry point.
        let code = unsafe { hook(id_ptr.as_ptr(), pid_val, old_ptr.as_ptr(), new_ptr.as_ptr()) };
        if code != 0 {
            return Err(plugin_hook_error(&self.name, "on_update", code));
        }
        Ok(())
    }

    fn on_event(&self, event: SystemEvent) {
        let Some(hook) = self.on_event else {
            return;
        };
        if let Ok(json) = serde_json::to_string(&event)
            && let Ok(cstr) = std::ffi::CString::new(json)
        {
            // SAFETY: `cstr` is a valid NUL-terminated `CString` that outlives
            // the call; `hook` is a checked vtable entry point.
            let _ = unsafe { hook(cstr.as_ptr()) };
        }
    }

    fn on_reload(&self) -> anyhow::Result<()> {
        if let Some(hook) = self.on_reload {
            // SAFETY: `hook` is a checked vtable entry point with no arguments.
            let code = unsafe { hook() };
            if code != 0 {
                return Err(plugin_hook_error(&self.name, "on_reload", code));
            }
        }
        Ok(())
    }

    fn collect_metrics(&self) -> String {
        let Some(hook) = self.collect_metrics else {
            return String::new();
        };
        let mut buf = vec![0u8; 4096];
        // SAFETY: `buf` is a valid writable buffer of `buf.len()` bytes for the
        // duration of the call; the ABI contract is that the plugin writes at
        // most `buf.len()` bytes and returns the number written.
        let written = unsafe { hook(buf.as_mut_ptr().cast(), buf.len()) };
        if written == 0 {
            return String::new();
        }
        let end = written.min(buf.len());
        String::from_utf8_lossy(&buf[..end]).into_owned()
    }

    fn supports_resource_limits(&self) -> bool {
        self.after_start.is_some() && self.on_update.is_some()
    }
}
