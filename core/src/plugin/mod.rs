//! Plugin host: optional subscription runtime (license verify + `plugins/*` dlopen).
//!
//! OSS scope: discover plugin libraries, verify signed `[license].key` claims, and
//! bridge HTTP/UI ABIs. Plugin implementations and subscription signing tools are not in
//! this repository.

mod abi;
mod adapter;
mod host;
mod http_host;
mod loader;
mod ui_host;

pub use abi::{PLUGIN_API_VERSION, PLUGIN_SYMBOL, SuperPluginV1};
pub use host::{
    LicenseOutcome, PluginHost, RunMode, log_license_degradation, validate_licensed_auth_secret,
    validate_licensed_security,
};
pub use http_host::attach_http_plugins;
pub use loader::{PluginRuntime, load_authorized_plugins, resolve_plugin_path};
pub use ui_host::{UiPluginHandle, load_ui_plugin, normalize_ui_path};
