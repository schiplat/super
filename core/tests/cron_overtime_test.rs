use common::{CreateProgramRequest, ProcessStatus};
use std::collections::HashMap;
use std::time::Duration;
use super_core::ManagerHandle;
use super_core::extension::Extension;
use super_core::manager::Manager;
use tokio::sync::{broadcast, mpsc};

#[path = "test_helpers.rs"]
mod test_helpers;

use uuid::Uuid;

const POLL: Duration = Duration::from_millis(250);
const DEADLINE: Duration = Duration::from_secs(30);

/// Poll until the program settles into `want`, returning the last observed
/// state. Replaces fixed sleeps: on a loaded runner the kill→stop chain can
/// overshoot any constant wait, so wait on the condition instead.
async fn wait_for_state(
    handle: &ManagerHandle,
    id: Uuid,
    want: impl Fn(ProcessStatus) -> bool,
    deadline: Duration,
) -> ProcessStatus {
    let started = std::time::Instant::now();
    loop {
        let state = handle.get_program(id).await.expect("Get failed").state;
        if want(state) || started.elapsed() >= deadline {
            return state;
        }
        tokio::time::sleep(POLL).await;
    }
}

/// Poll until the ledger records `event` (e.g. after an async cron tick).
async fn wait_for_event(handle: &ManagerHandle, id: Uuid, event: &str, deadline: Duration) -> bool {
    let started = std::time::Instant::now();
    loop {
        let events = handle.get_program_events(id).await.expect("Events failed");
        if events.iter().any(|e| e.event == event) {
            return true;
        }
        if started.elapsed() >= deadline {
            return false;
        }
        tokio::time::sleep(POLL).await;
    }
}

struct NoopExtension;
impl Extension for NoopExtension {}

async fn spawn_manager(
    temp_dir: &tempfile::TempDir,
) -> (ManagerHandle, tokio::task::JoinHandle<()>) {
    use std::collections::HashSet;
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
        HashSet::new(),
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

    // Wait until the overtime kill settles the run as Stopped. Poll on the
    // condition instead of a fixed sleep — on a loaded runner the
    // spawn→cap→kill→stop chain can overshoot any constant wait.
    let state = wait_for_state(&handle, id, |st| st == ProcessStatus::Stopped, DEADLINE).await;
    assert_eq!(
        state,
        ProcessStatus::Stopped,
        "overtime-killed run must settle as Stopped, not Fatal/Crashed"
    );
    assert!(
        handle
            .get_program(id)
            .await
            .expect("Get failed")
            .config
            .autostart,
        "overtime kill must not disable the schedule (autostart untouched)"
    );
    assert!(
        wait_for_event(&handle, id, "cron_overtime_kill", DEADLINE).await,
        "ledger must record a cron_overtime_kill event"
    );

    // The schedule keeps firing: the kill is followed by fresh `cron_started`
    // events (the cycle is ~3s: 2s runtime + 1s stop grace).
    assert!(
        wait_for_event(&handle, id, "cron_started", DEADLINE).await,
        "schedule must keep firing after an overtime kill"
    );
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

    handle.shutdown().await.ok();
}

/// Regression: `super stop` on a cron program must hold the schedule down —
/// the next tick must not respawn it. `super start` clears the hold and the
/// schedule resumes.
#[tokio::test]
async fn cron_stop_holds_schedule_until_start() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (handle, _task) = spawn_manager(&temp_dir).await;

    let req = CreateProgramRequest {
        name: Some("stoppable-cron".to_string()),
        command: "sleep".to_string(),
        args: vec!["30".to_string()],
        autostart: true,
        cron: Some("* * * * * *".to_string()),
        ..Default::default()
    };
    let ids = handle.create_program(req).await.expect("Create failed");
    let id = ids[0];

    // Let it spawn before stopping.
    wait_for_state(
        &handle,
        id,
        |st| matches!(st, ProcessStatus::Running | ProcessStatus::Healthy),
        DEADLINE,
    )
    .await;
    handle.stop_program(id, false).await.expect("Stop failed");

    // The every-second schedule must NOT respawn the program while held:
    // hold for a full tick window and assert it stays stopped.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let info = handle.get_program(id).await.expect("Get failed");
    assert_eq!(
        info.state,
        ProcessStatus::Stopped,
        "manually stopped cron program must stay stopped across ticks"
    );

    // An explicit start clears the hold; the schedule resumes.
    handle.start_program(id).await.expect("Start failed");
    let state = wait_for_state(
        &handle,
        id,
        |st| matches!(st, ProcessStatus::Running | ProcessStatus::Healthy),
        DEADLINE,
    )
    .await;
    assert!(
        matches!(state, ProcessStatus::Running | ProcessStatus::Healthy),
        "after start, cron runs again; got {state:?}"
    );
    assert!(
        wait_for_event(&handle, id, "cron_started", DEADLINE).await,
        "cron_started events must resume after start"
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

    wait_for_state(
        &handle,
        id,
        |st| matches!(st, ProcessStatus::Running | ProcessStatus::Healthy),
        DEADLINE,
    )
    .await;
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
