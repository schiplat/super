//! Shared integration-test setup — keep `[storage]` paths inside a temp dir so
//! `cargo test` never writes `{uuid}.out/.err` under the crate tree (e.g.
//! `core/logs/`).

use super_core::config::ServerConfig;
use tempfile::TempDir;

/// Build a `ServerConfig` whose storage paths all live under `temp`.
pub fn test_server_config(temp: &TempDir) -> ServerConfig {
    let base = temp.path();
    let mut config = ServerConfig::default();
    config.storage.data_file = base.join("data/snapshot.json");
    config.storage.log_dir = base.join("logs");
    config.storage.events_file = base.join("data/events.db");
    config
}
