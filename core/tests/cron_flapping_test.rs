use common::{CreateProgramRequest, ProcessStatus};
use std::collections::HashMap;
use std::time::Duration;
use super_core::ManagerHandle;
use super_core::config::ServerConfig;
use super_core::extension::Extension;
use super_core::manager::Manager;
use tokio::sync::{broadcast, mpsc};

struct NoopExtension;
impl Extension for NoopExtension {}

#[tokio::test]
async fn cron_job_exempt_from_flapping_detection() {
    let (log_tx, _) = broadcast::channel(100);
    let temp_dir = tempfile::tempdir().unwrap();
    let data_file = temp_dir.path().join("data.json");

    let mut config = ServerConfig::default();
    config.storage.data_file = data_file.clone();
    // Aggressive flapping config: any long-running service that restarts 2+ times
    // within 3s would be flagged as flapping.
    config.server.flapping_window = 3;
    config.server.flapping_threshold = 2;

    let (cmd_tx, cmd_rx) = mpsc::channel(100);
    let event_db = super_core::event_db::EventDb::open(&temp_dir.path().join("events.db"))
        .await
        .unwrap();
    let manager = Manager::new(
        config,
        temp_dir.path().join("super.toml"),
        Box::new(|_| Ok(())),
        cmd_rx,
        cmd_tx.clone(),
        HashMap::new(),
        log_tx,
        Box::new(NoopExtension),
        event_db,
    );
    tokio::spawn(async move {
        manager.run().await;
    });
    let handle = ManagerHandle::new(cmd_tx);

    // A scheduled task that runs every second and succeeds instantly. Each tick
    // spawns it again — this is normal cron behavior, not a restart loop.
    let req = CreateProgramRequest {
        name: Some("fast-cron".to_string()),
        command: "true".to_string(),
        args: vec![],
        autostart: true,
        cron: Some("* * * * * *".to_string()),
        ..Default::default()
    };
    let ids = handle.create_program(req).await.expect("Create failed");
    let id = ids[0];

    // Let several ticks fire — more than the flapping threshold would allow.
    tokio::time::sleep(Duration::from_secs(5)).await;

    let info = handle.get_program(id).await.expect("Get failed");

    // Flapping would have flipped `autostart` to false and forced Fatal. Cron
    // jobs must be exempt: the schedule stays intact and the program returns to
    // Stopped between ticks.
    assert!(
        info.config.autostart,
        "cron job must not have autostart disabled by flapping detection"
    );
    assert_ne!(
        info.state,
        ProcessStatus::Fatal,
        "cron job must not be marked Fatal by flapping detection"
    );
}
