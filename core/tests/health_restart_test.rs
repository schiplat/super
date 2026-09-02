use common::{CreateProgramRequest, HealthCheck, ProcessStatus, ProgramEventRecord};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use super_core::ManagerHandle;
use super_core::extension::Extension;
use super_core::manager::Manager;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

#[path = "test_helpers.rs"]
mod test_helpers;

struct NoopExtension;
impl Extension for NoopExtension {}

async fn manager_with_temp(log_dir: std::path::PathBuf) -> (ManagerHandle, tempfile::TempDir) {
    let (log_tx, _) = broadcast::channel(100);
    let temp_dir = tempfile::tempdir().unwrap();
    let mut config = test_helpers::test_server_config(&temp_dir);
    config.storage.log_dir = log_dir;

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
    );
    tokio::spawn(async move {
        manager.run().await;
    });
    (ManagerHandle::new(cmd_tx), temp_dir)
}

async fn spawn_with_health(
    handle: &ManagerHandle,
    name: &str,
    health_cmd: String,
    interval_secs: u64,
    start_period_secs: u64,
    max_failures: u32,
    retry_limit: u32,
) -> Uuid {
    let req = CreateProgramRequest {
        name: Some(name.to_string()),
        command: "sleep".to_string(),
        args: vec!["1000".to_string()],
        autostart: true,
        retry_limit,
        health_check: Some(HealthCheck::Exec {
            command: health_cmd,
            interval_secs,
            timeout_secs: 2,
            start_period_secs,
            max_failures,
        }),
        ..Default::default()
    };
    let ids = handle.create_program(req).await.expect("Create failed");
    ids[0]
}

async fn current_pid(handle: &ManagerHandle, id: Uuid) -> Option<u32> {
    handle.get_program(id).await.ok().and_then(|p| p.pid)
}

async fn current_state(handle: &ManagerHandle, id: Uuid) -> ProcessStatus {
    handle
        .get_program(id)
        .await
        .map(|p| p.state)
        .unwrap_or(ProcessStatus::Stopped)
}

/// Poll until `cond` holds over the current PID (or `timeout` elapses).
async fn wait_for_pid<F>(
    handle: &ManagerHandle,
    id: Uuid,
    cond: F,
    timeout: Duration,
) -> Option<u32>
where
    F: Fn(Option<u32>) -> bool,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let pid = current_pid(handle, id).await;
        if cond(pid) || tokio::time::Instant::now() >= deadline {
            return pid;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Poll until `cond` holds over the event history (or `timeout` elapses).
async fn wait_for_events<F>(
    handle: &ManagerHandle,
    id: Uuid,
    cond: F,
    timeout: Duration,
) -> Vec<ProgramEventRecord>
where
    F: Fn(&[ProgramEventRecord]) -> bool,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let events = handle.get_program_events(id).await.unwrap_or_default();
        if cond(&events) || tokio::time::Instant::now() >= deadline {
            return events;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn event_retry_counts(events: &[ProgramEventRecord], kind: &str) -> Vec<u32> {
    events
        .iter()
        .filter(|e| e.event == kind)
        .map(|e| e.retry_count.unwrap_or(0))
        .collect()
}

// ---------------------------------------------------------------------------

/// A persistently-failing health check restarts the process (honoring
/// `max_failures`), accumulates a restart counter, and enters Fatal once
/// `retry_limit` health restarts are exhausted.
#[tokio::test]
async fn health_failures_restart_then_fatal_after_retry_limit() {
    let logs = tempfile::tempdir().unwrap();
    let (handle, _temp) = manager_with_temp(logs.path().to_path_buf()).await;

    // Always fails; interval 1s, restart after 1 failure, retry_limit 2.
    let id = spawn_with_health(&handle, "doomed", "exit 1".to_string(), 1, 0, 1, 2).await;
    let original_pid = wait_for_pid(&handle, id, |p| p.is_some(), Duration::from_secs(10)).await;
    assert!(original_pid.is_some(), "program should start");

    // Restart #1 then #2: pid must change twice, then the program goes Fatal.
    let pid_after_restart1 = wait_for_pid(
        &handle,
        id,
        |p| p.is_some() && p != original_pid,
        Duration::from_secs(15),
    )
    .await;
    assert!(pid_after_restart1.is_some(), "first health restart missing");

    let pid_after_restart2 = wait_for_pid(
        &handle,
        id,
        |p| p.is_some() && p != pid_after_restart1,
        Duration::from_secs(15),
    )
    .await;
    assert!(
        pid_after_restart2.is_some(),
        "second health restart missing"
    );

    // Third failure exceeds retry_limit=2 -> Fatal, process stopped.
    wait_for_events(
        &handle,
        id,
        |e| e.iter().any(|r| r.event == "process_fatal"),
        Duration::from_secs(15),
    )
    .await;
    assert_eq!(
        current_state(&handle, id).await,
        ProcessStatus::Fatal,
        "should be Fatal after retry_limit health restarts"
    );
    assert!(
        current_pid(&handle, id).await.is_none(),
        "Fatal program must be stopped"
    );

    let events = handle.get_program_events(id).await.unwrap_or_default();
    let restarts = event_retry_counts(&events, "health_restart");
    assert_eq!(restarts, vec![1, 2], "restart counters should accumulate");
    let fatals = event_retry_counts(&events, "process_fatal");
    assert!(
        fatals.contains(&3),
        "Fatal event should carry retry count 3, got {fatals:?}"
    );
}

/// `max_failures = 0` disables the auto-restart: the process stays running and
/// is only marked unhealthy.
#[tokio::test]
async fn max_failures_zero_disables_restart() {
    let logs = tempfile::tempdir().unwrap();
    let (handle, _temp) = manager_with_temp(logs.path().to_path_buf()).await;

    let id = spawn_with_health(
        &handle,
        "sticky",
        "exit 1".to_string(),
        1,
        0,
        0, // disabled
        3,
    )
    .await;
    let pid = wait_for_pid(&handle, id, |p| p.is_some(), Duration::from_secs(10)).await;
    assert!(pid.is_some());

    // Give several probe intervals; the process must survive untouched.
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert_eq!(
        current_pid(&handle, id).await,
        pid,
        "process must not be restarted when max_failures = 0"
    );
    assert_eq!(
        current_state(&handle, id).await,
        ProcessStatus::Running,
        "status should stay Running (unhealthy) without a restart"
    );

    let events = handle.get_program_events(id).await.unwrap_or_default();
    assert!(
        !events.iter().any(|e| e.event == "health_restart"),
        "no health_restart events expected when auto-restart is disabled"
    );
}

/// A recovering process resets the health-restart counter: after a healthy
/// window, the next failure starts counting from 1 again.
#[tokio::test]
async fn recovery_resets_health_restart_counter() {
    let logs = tempfile::tempdir().unwrap();
    let flag = logs.path().join("ready.flag");
    let (handle, _temp) = manager_with_temp(logs.path().to_path_buf()).await;

    let health_cmd = format!("[ -f {} ] && exit 0 || exit 1", flag.display());
    let id = spawn_with_health(&handle, "recoverer", health_cmd, 1, 0, 1, 3).await;
    wait_for_pid(&handle, id, |p| p.is_some(), Duration::from_secs(10)).await;

    // No flag yet -> fails -> restart #1 (retry_count 1).
    wait_for_events(
        &handle,
        id,
        |e| e.iter().any(|r| r.event == "health_restart"),
        Duration::from_secs(15),
    )
    .await;

    // Create the flag -> healthy.
    std::fs::write(&flag, "ready").unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        if current_state(&handle, id).await == ProcessStatus::Healthy
            || tokio::time::Instant::now() >= deadline
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        current_state(&handle, id).await,
        ProcessStatus::Healthy,
        "process should become Healthy once the flag appears"
    );

    // Remove the flag -> fails again -> restart #2. The counter must have been
    // reset by the healthy window, so retry_count is 1 again, not 2.
    std::fs::remove_file(&flag).unwrap();
    wait_for_events(
        &handle,
        id,
        |e| e.iter().filter(|r| r.event == "health_restart").count() >= 2,
        Duration::from_secs(15),
    )
    .await;

    let events = handle.get_program_events(id).await.unwrap_or_default();
    let restarts = event_retry_counts(&events, "health_restart");
    assert!(
        restarts.len() >= 2 && restarts.iter().all(|&c| c == 1),
        "counter should reset to 1 after recovery, got {restarts:?}"
    );
}

/// `start_period_secs` delays the first probe: no probe runs until the grace
/// period has elapsed.
#[tokio::test]
async fn start_period_delays_first_probe() {
    let logs = tempfile::tempdir().unwrap();
    let probes = logs.path().join("probes.log");
    let (handle, _temp) = manager_with_temp(logs.path().to_path_buf()).await;

    let health_cmd = format!("echo probe >> {}; exit 0", probes.display());
    let id = spawn_with_health(&handle, "graceful", health_cmd, 1, 3, 0, 3).await;
    wait_for_pid(&handle, id, |p| p.is_some(), Duration::from_secs(10)).await;

    let t0 = Instant::now();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let lines = std::fs::read_to_string(&probes)
            .map(|c| c.lines().count())
            .unwrap_or(0);
        if lines >= 1 || tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let elapsed = t0.elapsed();
    assert!(
        elapsed >= Duration::from_secs(2),
        "first probe must wait for start_period_secs=3 (took {elapsed:?})"
    );
}

/// `interval_secs` controls probe cadence: with interval 1s several probes run
/// within a few seconds (a default 5s interval would yield at most one).
#[tokio::test]
async fn interval_controls_probe_cadence() {
    let logs = tempfile::tempdir().unwrap();
    let probes = logs.path().join("probes.log");
    let (handle, _temp) = manager_with_temp(logs.path().to_path_buf()).await;

    let health_cmd = format!("echo probe >> {}; exit 0", probes.display());
    let id = spawn_with_health(&handle, "metered", health_cmd, 1, 0, 0, 3).await;
    wait_for_pid(&handle, id, |p| p.is_some(), Duration::from_secs(10)).await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    loop {
        let count = std::fs::read_to_string(&probes)
            .map(|c| c.lines().count())
            .unwrap_or(0);
        if count >= 2 || tokio::time::Instant::now() >= deadline {
            assert!(
                count >= 2,
                "interval_secs=1 should produce ≥2 probes in 4s (got {count})"
            );
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
