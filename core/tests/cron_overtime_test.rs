use common::{CreateProgramRequest, ProcessStatus};
use std::collections::HashMap;
use std::time::Duration;
use super_core::ManagerHandle;
use super_core::extension::Extension;
use super_core::manager::Manager;
use tokio::sync::{broadcast, mpsc};

#[path = "test_helpers.rs"]
mod test_helpers;

struct NoopExtension;
impl Extension for NoopExtension {}

async fn spawn_manager(
    temp_dir: &tempfile::TempDir,
) -> (ManagerHandle, tokio::task::JoinHandle<()>) {
    let (log_tx, _) = broadcast::channel(100);
    let mut config = test_helpers::test_server_config(temp_dir);
    // Aggressive flapping config: any long-running service restarting 2+ times
    // within 3s would be flagged. Cron must be immune by design.
    config.server.flapping_window = 3;
    config.server.flapping_threshold = 2;

    let (cmd_tx, cmd_rx) = mpsc::channel(100);
    let event_db = super_core::event_db::EventDb::open(&config.storage.events_file)
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
    )
    .expect("Manager::new should not fail");
    let task = tokio::spawn(async move {
        manager.run().await;
    });
    (ManagerHandle::new(cmd_tx), task)
}

/// A cron run that exceeds `kill_after_secs` is terminated through the
/// graceful stop path, recorded as `cron_overtime_kill`, and the schedule
/// keeps firing — without tripping flapping detection.
#[tokio::test]
async fn cron_overtime_kill_terminates_and_reschedules() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (handle, _task) = spawn_manager(&temp_dir).await;

    let req = CreateProgramRequest {
        name: Some("overtime-cron".to_string()),
        // Outlives the cap; the supervisor must end it.
        command: "sleep".to_string(),
        args: vec!["30".to_string()],
        autostart: true,
        cron: Some("* * * * * *".to_string()),
        // 1s tick granularity + 1s stop grace: dead well before 4s.
        kill_after_secs: Some(2),
        stopsecs: Some(1),
        max_concurrent: Some(1),
        ..Default::default()
    };
    let ids = handle.create_program(req).await.expect("Create failed");
    let id = ids[0];

    // Wait past cap + kill + stop grace, then some.
    tokio::time::sleep(Duration::from_secs(6)).await;

    let info = handle.get_program(id).await.expect("Get failed");
    assert_eq!(
        info.state,
        ProcessStatus::Stopped,
        "overtime-killed run must settle as Stopped, not Fatal/Crashed"
    );
    assert!(
        info.config.autostart,
        "overtime kill must not disable the schedule (autostart untouched)"
    );

    let events = handle.get_program_events(id).await.expect("Events failed");
    assert!(
        events.iter().any(|e| e.event == "cron_overtime_kill"),
        "ledger must record a cron_overtime_kill event, got: {:?}",
        events.iter().map(|e| e.event.as_str()).collect::<Vec<_>>()
    );

    // The schedule keeps firing: the kill is followed by fresh `cron_started`
    // events (the cycle is ~3s: 2s runtime + 1s stop grace), so expect more
    // than the initial start.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let info = handle.get_program(id).await.expect("Get failed");
    assert!(
        matches!(
            info.state,
            ProcessStatus::Running | ProcessStatus::Healthy | ProcessStatus::Stopped
        ),
        "schedule keeps cycling after an overtime kill; got {:?}",
        info.state
    );
    assert_ne!(
        info.state,
        ProcessStatus::Fatal,
        "cron job must never be marked Fatal by flapping detection"
    );
    let events = handle.get_program_events(id).await.expect("Events failed");
    assert!(
        events.iter().any(|e| e.event == "cron_started"),
        "schedule must keep firing after an overtime kill"
    );

    handle.shutdown().await.ok();
}

/// Without the cap, an overrunning run is left alone (backwards compatible).
#[tokio::test]
async fn cron_without_cap_is_not_killed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (handle, _task) = spawn_manager(&temp_dir).await;

    let req = CreateProgramRequest {
        name: Some("uncapped-cron".to_string()),
        command: "sleep".to_string(),
        args: vec!["30".to_string()],
        autostart: true,
        cron: Some("* * * * * *".to_string()),
        max_concurrent: Some(1),
        ..Default::default()
    };
    let ids = handle.create_program(req).await.expect("Create failed");
    let id = ids[0];

    tokio::time::sleep(Duration::from_secs(4)).await;

    let info = handle.get_program(id).await.expect("Get failed");
    assert!(
        matches!(info.state, ProcessStatus::Running | ProcessStatus::Healthy),
        "run must still be going with no kill_after_secs set; got {:?}",
        info.state
    );
    let events = handle.get_program_events(id).await.expect("Events failed");
    assert!(
        !events.iter().any(|e| e.event == "cron_overtime_kill"),
        "no overtime kill may be recorded without the cap"
    );

    handle.shutdown().await.ok();
}
