//! Regression: a graceful daemon shutdown must not flip `autostart` or add
//! operator hold-stops. `super shutdown` (and SIGTERM) stop the children, but
//! the next daemon start must bring autostart services back — exactly like
//! any other restart. Previously `handle_shutdown` reused `stop_program`, so
//! every running program came back with `autostart = false` and a
//! `stopped_by_user` mark, and whether that even reached disk depended on a
//! PersistTick racing the process exit.

use common::{CreateProgramRequest, ProcessStatus};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use super_core::extension::NoOpExtension;
use super_core::manager::{Command, Manager};
use super_core::{ManagerHandle, store};
use tempfile::TempDir;
use tokio::sync::{broadcast, mpsc};

#[path = "test_helpers.rs"]
mod test_helpers;

async fn spawn_manager(
    config: super_core::config::ServerConfig,
    initial: (
        HashMap<uuid::Uuid, common::ProgramConfig>,
        HashSet<uuid::Uuid>,
    ),
) -> (ManagerHandle, mpsc::Sender<Command>) {
    let (log_tx, _) = broadcast::channel(100);
    let (tx, rx) = mpsc::channel(64);
    let event_db = super_core::event_db::EventDb::open(&config.storage.events_file)
        .await
        .unwrap();

    let manager = Manager::new(
        config,
        std::path::PathBuf::from("super.toml"),
        Box::new(|_| Ok(())),
        rx,
        tx.clone(),
        initial.0,
        initial.1,
        log_tx,
        Box::new(NoOpExtension),
        event_db,
    )
    .expect("Manager::new should not fail");

    tokio::spawn(async move {
        manager.run().await;
    });
    (ManagerHandle::new(tx.clone()), tx)
}

#[tokio::test]
async fn graceful_shutdown_preserves_autostart() {
    let temp = TempDir::new().unwrap();
    let mut config = test_helpers::test_server_config(&temp);
    config.server.shutdown_timeout = 3;
    let data_file = config.storage.data_file.clone();

    // First daemon instance: one autostart service, one idle service.
    let (handle, _tx) = spawn_manager(config.clone(), (HashMap::new(), HashSet::new())).await;

    let keep_id = handle
        .create_program(CreateProgramRequest {
            name: Some("keep-me".to_string()),
            command: "/bin/sleep".to_string(),
            args: vec!["300".to_string()],
            autostart: true,
            ..Default::default()
        })
        .await
        .unwrap()[0];
    handle
        .create_program(CreateProgramRequest {
            name: Some("idle".to_string()),
            command: "/bin/sleep".to_string(),
            args: vec!["300".to_string()],
            autostart: false,
            ..Default::default()
        })
        .await
        .unwrap();

    // keep-me must come up (autostart path works).
    let mut healthy = false;
    for _ in 0..20 {
        if handle.get_program(keep_id).await.unwrap().state == ProcessStatus::Healthy {
            healthy = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert!(healthy, "keep-me should be Healthy before shutdown");

    // Graceful shutdown (same path `super shutdown` / SIGTERM take).
    handle.shutdown().await.unwrap();

    // The on-disk snapshot must be untouched: autostart preserved, no
    // operator hold-stops introduced by the shutdown itself.
    let snapshot = store::load_with_recovery(&data_file).await.unwrap();
    let keep = snapshot.programs.get(&keep_id).expect("keep-me persisted");
    assert!(
        keep.autostart,
        "graceful shutdown must not flip autostart to false"
    );
    assert!(
        !snapshot.stopped_by_user.contains(&keep_id),
        "graceful shutdown must not add an operator hold-stop"
    );

    // Second daemon instance on the same snapshot (the restart path superd
    // takes): keep-me must be restored.
    let restored_state = store::load_with_recovery(&data_file).await.unwrap();
    let (handle2, _tx2) = spawn_manager(
        config,
        (restored_state.programs, restored_state.stopped_by_user),
    )
    .await;
    let mut restored = false;
    for _ in 0..30 {
        if handle2.get_program(keep_id).await.unwrap().state == ProcessStatus::Healthy {
            restored = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert!(
        restored,
        "keep-me must come back after a restart following graceful shutdown"
    );

    // Also verify the explicit operator path still holds: `super stop`
    // semantics (autostart=false + hold) must survive the SAME shutdown flow.
    handle2.stop_program(keep_id, false).await.unwrap();
    handle2.shutdown().await.unwrap();
    let snapshot = store::load_with_recovery(&data_file).await.unwrap();
    let keep = snapshot.programs.get(&keep_id).expect("keep-me persisted");
    assert!(
        !keep.autostart,
        "an explicit operator stop must still disable autostart"
    );
    assert!(
        snapshot.stopped_by_user.contains(&keep_id),
        "an explicit operator stop must still hold the program"
    );
}
