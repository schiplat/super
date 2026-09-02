use common::{CreateProgramRequest, ProcessStatus};
use std::collections::HashMap;
use std::time::Duration;
use super_core::ManagerHandle;
use super_core::extension::Extension;
use super_core::manager::Manager;
use tempfile::TempDir;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

#[path = "test_helpers.rs"]
mod test_helpers;

struct NoopExtension;
impl Extension for NoopExtension {}

async fn setup_manager() -> (ManagerHandle, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join("super.toml");

    let config = test_helpers::test_server_config(&temp_dir);

    let (log_tx, _) = broadcast::channel(100);
    let (tx, rx) = mpsc::channel(32);
    let log_reloader = Box::new(|_| Ok(()));

    let event_db = super_core::event_db::EventDb::open(&config.storage.events_file)
        .await
        .unwrap();

    let manager = Manager::new(
        config,
        config_file,
        log_reloader,
        rx,
        tx.clone(),
        HashMap::new(),
        log_tx,
        Box::new(NoopExtension),
        event_db,
    );
    tokio::spawn(async move {
        manager.run().await;
    });

    (ManagerHandle::new(tx), temp_dir)
}

async fn create_sleep(handle: &ManagerHandle, name: &str, depends_on: Vec<String>) -> Uuid {
    let req = CreateProgramRequest {
        name: Some(name.to_string()),
        command: "/bin/sleep".to_string(),
        args: vec!["30".to_string()],
        autostart: false,
        depends_on,
        ..Default::default()
    };
    let ids = handle.create_program(req).await.expect("Create failed");
    ids[0]
}

async fn wait_running_or_healthy(handle: &ManagerHandle, id: Uuid) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let info = handle.get_program(id).await.expect("Get failed");
        if matches!(info.state, ProcessStatus::Running | ProcessStatus::Healthy) {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "program {} never became Running/Healthy (currently {:?})",
            id,
            info.state
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn dependency_is_auto_started() {
    let (handle, _tmp) = setup_manager().await;

    let dep_id = create_sleep(&handle, "dep-db", vec![]).await;
    let app_id = create_sleep(&handle, "app-api", vec!["dep-db".to_string()]).await;

    // Start the dependent program: it should pull its dependency up first.
    handle.start_program(app_id).await.expect("Start failed");

    // The dependency must be auto-started and the dependent must transition to
    // Running once the dependency is up.
    wait_running_or_healthy(&handle, dep_id).await;
    wait_running_or_healthy(&handle, app_id).await;
}
