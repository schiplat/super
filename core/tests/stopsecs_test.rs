use common::ProgramConfig;
use std::collections::HashMap;
use super_core::config::ServerConfig;
use super_core::manager::controller::LifecycleController;
use super_core::manager::registry::ProcessRegistry;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

#[test]
fn test_stopsecs_fallback_to_server_default() {
    let (tx, _) = mpsc::channel(1);
    let (log_tx, _) = broadcast::channel(1);
    let mut server = ServerConfig::default();
    server.server.shutdown_timeout = 42;

    let controller = LifecycleController::new(
        server,
        tx,
        log_tx,
        std::sync::Arc::new(super_core::extension::NoOpExtension),
        std::sync::Arc::new(super_core::monitor::ResourceMonitor::new(
            mpsc::channel(1).0,
        )),
    );

    let id = Uuid::new_v4();
    let mut registry = ProcessRegistry::new(HashMap::from([(
        id,
        ProgramConfig {
            name: "svc".into(),
            command: "/bin/true".into(),
            ..Default::default()
        },
    )]));

    assert_eq!(controller.stop_timeout(&registry, id), 42);

    registry.get_config_mut(&id).unwrap().stopsecs = Some(120);
    assert_eq!(controller.stop_timeout(&registry, id), 120);
}

#[test]
fn test_stopsecs_deserializes_supervisor_alias() {
    let cfg: ProgramConfig = serde_json::from_str(
        r#"{"name":"x","command":"/bin/true","created_at":0,"updated_at":0,"stopwaitsecs":30}"#,
    )
    .unwrap();
    assert_eq!(cfg.stopsecs, Some(30));
}
