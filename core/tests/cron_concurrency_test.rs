use common::{CreateProgramRequest, CronOverlap};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use super_core::ManagerHandle;
use super_core::extension::Extension;
use super_core::manager::Manager;
use tokio::sync::{broadcast, mpsc};

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

// + Marker helpers +

fn marker_lines(marker: &Path) -> Vec<String> {
    std::fs::read_to_string(marker)
        .map(|c| c.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

fn count(lines: &[String], needle: &str) -> usize {
    lines.iter().filter(|l| *l == needle).count()
}

/// Poll the marker file until `cond` holds or `timeout` elapses, then return
/// the latest contents. Absorbs CI scheduling jitter around fixed sleeps.
async fn wait_until<F>(marker: &Path, cond: F, timeout: Duration) -> Vec<String>
where
    F: Fn(&[String]) -> bool,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let lines = marker_lines(marker);
        if cond(&lines) || tokio::time::Instant::now() >= deadline {
            return lines;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Two `start` lines with no `end` between them mean two instances of the same
/// program were in flight at once.
fn assert_no_overlap(lines: &[String]) {
    let starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| *l == "start")
        .map(|(i, _)| i)
        .collect();
    for w in starts.windows(2) {
        let between = &lines[w[0] + 1..w[1]];
        assert!(
            between.iter().any(|l| l == "end"),
            "two overlapping runs (no 'end' between starts at {}-{}): {:?}",
            w[0],
            w[1],
            lines
        );
    }
}

/// A slow cron task (3s) firing every second with `max_concurrent = 3` must
/// produce genuinely overlapping runs: the second firing starts before the
/// first finishes.
#[tokio::test]
async fn max_concurrent_allows_overlapping_runs() {
    let logs = tempfile::tempdir().unwrap();
    let marker = logs.path().join("overlap.log");
    let (handle, _temp) = manager_with_temp(logs.path().to_path_buf()).await;

    // `sh -c` appends "start" then sleeps 3s then appends "end". With overlap,
    // at least two "start" lines appear before the first "end" line.
    let script = format!(
        "echo start >> {}; sleep 3; echo end >> {}",
        marker.display(),
        marker.display()
    );
    let req = CreateProgramRequest {
        name: Some("slow-cron".to_string()),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), script],
        autostart: true,
        cron: Some("* * * * * *".to_string()),
        on_overlap: Some(CronOverlap::Queue),
        max_concurrent: Some(3),
        max_queued: Some(50),
        ..Default::default()
    };
    let ids = handle.create_program(req).await.expect("Create failed");
    let id = ids[0];

    // Let several ticks fire; each run lasts 3s, so three can be in flight.
    tokio::time::sleep(Duration::from_secs(6)).await;

    let info = handle.get_program(id).await.expect("Get failed");
    assert_eq!(
        info.config.max_concurrent,
        Some(3),
        "max_concurrent must round-trip through config"
    );

    let content = std::fs::read_to_string(&marker).unwrap_or_default();
    let starts: Vec<&str> = content.lines().filter(|l| *l == "start").collect();
    let first_end = content.lines().position(|l| l == "end");
    assert!(
        starts.len() >= 2,
        "expected multiple firings, got {} (content: {:?})",
        starts.len(),
        content
    );
    let first_end = first_end.expect("at least one run must finish");
    assert!(
        starts.len() as usize > first_end + 1,
        "runs must overlap: at least two starts before the first end (content: {:?})",
        content
    );

    handle.shutdown().await.expect("Shutdown failed");
}

/// Default `max_concurrent` is 1: a 2s task on a 1s cron must never run two
/// instances concurrently, even though ticks fire while the run is active.
#[tokio::test]
async fn default_max_concurrent_one_prevents_overlap() {
    let logs = tempfile::tempdir().unwrap();
    let marker = logs.path().join("default.log");
    let (handle, _temp) = manager_with_temp(logs.path().to_path_buf()).await;

    let script = format!(
        "echo start >> {}; sleep 2; echo end >> {}",
        marker.display(),
        marker.display()
    );
    let req = CreateProgramRequest {
        name: Some("default-cron".to_string()),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), script],
        autostart: true,
        cron: Some("* * * * * *".to_string()),
        ..Default::default()
    };
    let ids = handle.create_program(req).await.expect("Create failed");
    let id = ids[0];

    tokio::time::sleep(Duration::from_secs(6)).await;

    let info = handle.get_program(id).await.expect("Get failed");
    assert!(
        info.config.max_concurrent.is_none(),
        "max_concurrent must be unset by default, got {:?}",
        info.config.max_concurrent
    );

    // Poll until two runs completed; the third may still be in flight.
    let lines = wait_until(&marker, |l| count(l, "end") >= 2, Duration::from_secs(10)).await;
    let starts = count(&lines, "start");
    let ends = count(&lines, "end");
    assert!(
        starts >= 2,
        "expected repeated runs, got {starts}: {lines:?}"
    );
    assert!(
        starts <= ends + 1,
        "every run must complete (starts={starts}, ends={ends}): {lines:?}"
    );
    assert_no_overlap(&lines);

    handle.shutdown().await.expect("Shutdown failed");
}

/// `on_overlap = skip` at `max_concurrent = 1` drops ticks while a run is
/// active: nothing is queued, nothing is killed, and runs never overlap.
#[tokio::test]
async fn on_overlap_skip_drops_ticks_at_max_concurrent() {
    let logs = tempfile::tempdir().unwrap();
    let marker = logs.path().join("skip.log");
    let (handle, _temp) = manager_with_temp(logs.path().to_path_buf()).await;

    let script = format!(
        "echo start >> {}; sleep 3; echo end >> {}",
        marker.display(),
        marker.display()
    );
    let req = CreateProgramRequest {
        name: Some("skip-cron".to_string()),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), script],
        autostart: true,
        cron: Some("* * * * * *".to_string()),
        on_overlap: Some(CronOverlap::Skip),
        max_concurrent: Some(1),
        max_queued: Some(10),
        ..Default::default()
    };
    let ids = handle.create_program(req).await.expect("Create failed");
    let id = ids[0];

    // 7s of 1s ticks with 3s runs: at most three non-overlapping runs fit.
    // Poll until two runs have completed (a run may be in flight when read).
    let lines = wait_until(&marker, |l| count(l, "end") >= 2, Duration::from_secs(10)).await;
    let starts = count(&lines, "start");
    let ends = count(&lines, "end");
    assert!(
        (2..=3).contains(&starts),
        "skip must drop ticks (at most 3 non-overlapping runs fit in 7s), got {starts}: {lines:?}"
    );
    assert!(
        starts <= ends + 1,
        "skip must not kill runs mid-flight (starts={starts}, ends={ends}): {lines:?}"
    );
    assert_no_overlap(&lines);

    let events = handle
        .get_program_events(id)
        .await
        .expect("Get events failed");
    let fulls = events.iter().filter(|e| e.event == "queue_full").count();
    assert_eq!(
        fulls, 0,
        "skip must not queue, so never fill the queue: {events:?}"
    );

    handle.shutdown().await.expect("Shutdown failed");
}

/// `on_overlap = queue` at `max_concurrent = 1` starts queued firings as soon
/// as the slot frees: more runs than skip over the same window, still never
/// overlapping, and with a generous `max_queued` nothing is dropped.
#[tokio::test]
async fn on_overlap_queue_drains_after_slot_frees() {
    let logs = tempfile::tempdir().unwrap();
    let marker = logs.path().join("queue-drain.log");
    let (handle, _temp) = manager_with_temp(logs.path().to_path_buf()).await;

    let script = format!(
        "echo start >> {}; sleep 2; echo end >> {}",
        marker.display(),
        marker.display()
    );
    let req = CreateProgramRequest {
        name: Some("drain-cron".to_string()),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), script],
        autostart: true,
        cron: Some("* * * * * *".to_string()),
        on_overlap: Some(CronOverlap::Queue),
        max_concurrent: Some(1),
        max_queued: Some(10),
        ..Default::default()
    };
    let ids = handle.create_program(req).await.expect("Create failed");
    let id = ids[0];

    // Poll until three runs have completed (a 4th may be in flight when read).
    let lines = wait_until(&marker, |l| count(l, "end") >= 3, Duration::from_secs(10)).await;
    let starts = count(&lines, "start");
    let ends = count(&lines, "end");
    assert!(
        starts >= 3,
        "queued firings must drain into new runs as the slot frees, got {starts}: {lines:?}"
    );
    assert!(
        starts <= ends + 1,
        "every drained run must complete (starts={starts}, ends={ends}): {lines:?}"
    );
    assert_no_overlap(&lines);

    let events = handle
        .get_program_events(id)
        .await
        .expect("Get events failed");
    let fulls = events.iter().filter(|e| e.event == "queue_full").count();
    assert_eq!(
        fulls, 0,
        "a queue of 10 must not fill during a 7s window: {events:?}"
    );

    handle.shutdown().await.expect("Shutdown failed");
}

/// `on_overlap = kill` at `max_concurrent = 1` terminates the running instance
/// so the new firing can start. Killed instances are observable (they trap
/// TERM) and are never allowed to finish their run.
#[tokio::test]
async fn on_overlap_kill_terminates_running_instance() {
    let logs = tempfile::tempdir().unwrap();
    let marker = logs.path().join("kill.log");
    let (handle, _temp) = manager_with_temp(logs.path().to_path_buf()).await;

    // Trap TERM to write a marker: a killed instance shows up as "killed"
    // instead of completing its 5s sleep to "end".
    let script = format!(
        "trap 'echo killed >> {}; exit 0' TERM; echo start >> {}; sleep 5; echo end >> {}",
        marker.display(),
        marker.display(),
        marker.display()
    );
    let req = CreateProgramRequest {
        name: Some("kill-cron".to_string()),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), script],
        autostart: true,
        cron: Some("* * * * * *".to_string()),
        on_overlap: Some(CronOverlap::Kill),
        max_concurrent: Some(1),
        max_queued: Some(10),
        ..Default::default()
    };
    let ids = handle.create_program(req).await.expect("Create failed");
    let id = ids[0];

    let lines = wait_until(
        &marker,
        |l| count(l, "killed") >= 1 && count(l, "start") >= 2,
        Duration::from_secs(8),
    )
    .await;
    let starts = count(&lines, "start");
    let ends = count(&lines, "end");
    let killed = count(&lines, "killed");
    assert!(
        killed >= 1,
        "kill policy must terminate the running instance: {lines:?}"
    );
    assert!(
        starts >= 2,
        "new firings must start after a kill: {lines:?}"
    );
    assert!(
        ends < starts,
        "killed instances must not complete (starts={starts}, ends={ends}): {lines:?}"
    );

    let events = handle
        .get_program_events(id)
        .await
        .expect("Get events failed");
    let fulls = events.iter().filter(|e| e.event == "queue_full").count();
    assert_eq!(
        fulls, 0,
        "kill+queue must not overflow max_queued=10: {events:?}"
    );

    handle.shutdown().await.expect("Shutdown failed");
}

/// `on_overlap = kill` with `max_concurrent > 1` must terminate only the
/// *oldest* instance to free a single slot — a sibling must survive and be
/// allowed to finish its run.
#[tokio::test]
async fn on_overlap_kill_with_siblings_keeps_one_running() {
    let logs = tempfile::tempdir().unwrap();
    let marker = logs.path().join("kill-sibling.log");
    let (handle, _temp) = manager_with_temp(logs.path().to_path_buf()).await;

    let script = format!(
        "trap 'echo killed >> {}; exit 0' TERM; echo start >> {}; sleep 2; echo end >> {}",
        marker.display(),
        marker.display(),
        marker.display()
    );
    let req = CreateProgramRequest {
        name: Some("kill-rotate".to_string()),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), script],
        autostart: true,
        cron: Some("* * * * * *".to_string()),
        on_overlap: Some(CronOverlap::Kill),
        max_concurrent: Some(2),
        max_queued: Some(10),
        ..Default::default()
    };
    let ids = handle.create_program(req).await.expect("Create failed");
    let id = ids[0];

    // Sanity: policy fields round-trip through config.
    let info = handle.get_program(id).await.expect("Get failed");
    assert_eq!(info.config.max_concurrent, Some(2));
    assert_eq!(info.config.on_overlap, Some(CronOverlap::Kill));

    // Let several kill rotations accumulate; a sibling must still have
    // completed at least one full run.
    let lines = wait_until(
        &marker,
        |l| count(l, "killed") >= 2,
        Duration::from_secs(12),
    )
    .await;
    let starts = count(&lines, "start");
    let ends = count(&lines, "end");
    let killed = count(&lines, "killed");
    assert!(
        killed >= 2,
        "expected repeated kill firings, got {killed}: {lines:?}"
    );
    assert!(
        starts >= 3,
        "expected rotations, got {starts} starts: {lines:?}"
    );
    assert!(
        ends >= 1,
        "a sibling must survive the kill and finish its run (starts={starts}, ends={ends}, killed={killed}): {lines:?}"
    );

    handle.shutdown().await.expect("Shutdown failed");
}

/// `max_queued` bounds the pending cron queue; firings beyond the cap are
/// dropped and recorded as `queue_full` events.
#[tokio::test]
async fn max_queued_drops_firings_and_records_event() {
    let logs = tempfile::tempdir().unwrap();
    let marker = logs.path().join("drop.log");
    let (handle, _temp) = manager_with_temp(logs.path().to_path_buf()).await;

    // 2s task on a 1s cron with a single slot and a tiny queue: most ticks hit
    // a full queue and are dropped.
    let script = format!(
        "echo run >> {}; sleep 2; echo done >> {}",
        marker.display(),
        marker.display()
    );
    let req = CreateProgramRequest {
        name: Some("burst-cron".to_string()),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), script],
        autostart: true,
        cron: Some("* * * * * *".to_string()),
        on_overlap: Some(CronOverlap::Queue),
        max_concurrent: Some(1),
        max_queued: Some(1),
        ..Default::default()
    };
    let ids = handle.create_program(req).await.expect("Create failed");
    let id = ids[0];

    tokio::time::sleep(Duration::from_secs(7)).await;

    let lines = marker_lines(&marker);
    let runs = count(&lines, "run");
    assert!(runs >= 1, "the task must actually run: {lines:?}");
    assert!(
        runs < 6,
        "a 1-slot queue of 1 must drop most of ~7 ticks, got {runs} runs: {lines:?}"
    );

    let events = handle
        .get_program_events(id)
        .await
        .expect("Get events failed");
    let fulls = events.iter().filter(|e| e.event == "queue_full").count();
    assert!(
        fulls >= 1,
        "expected at least one queue_full event, got {fulls} (events: {:?})",
        events
    );

    handle.shutdown().await.expect("Shutdown failed");
}

/// `max_concurrent = 0` and `max_queued = 0` mean "use the default". The raw
/// values round-trip through the config while the effective accessors
/// normalize them.
#[tokio::test]
async fn zero_values_mean_defaults() {
    let logs = tempfile::tempdir().unwrap();
    let (handle, _temp) = manager_with_temp(logs.path().to_path_buf()).await;

    let req = CreateProgramRequest {
        name: Some("zero-cron".to_string()),
        command: "true".to_string(),
        args: vec![],
        autostart: true,
        cron: Some("* * * * * *".to_string()),
        max_concurrent: Some(0),
        max_queued: Some(0),
        ..Default::default()
    };
    let ids = handle.create_program(req).await.expect("Create failed");
    let id = ids[0];

    let info = handle.get_program(id).await.expect("Get failed");
    assert_eq!(
        info.config.max_concurrent,
        Some(0),
        "raw value must round-trip"
    );
    assert_eq!(info.config.max_queued, Some(0), "raw value must round-trip");
    assert_eq!(
        info.config.max_concurrent_eff(),
        1,
        "0 must normalize to the default"
    );
    assert_eq!(
        info.config.max_queued_eff(),
        100,
        "0 must normalize to the default"
    );

    handle.shutdown().await.expect("Shutdown failed");
}

/// Updating a program's concurrency policy persists and is visible on read.
#[tokio::test]
async fn update_changes_concurrency_policy() {
    let logs = tempfile::tempdir().unwrap();
    let (handle, _temp) = manager_with_temp(logs.path().to_path_buf()).await;

    let req = CreateProgramRequest {
        name: Some("updatable-cron".to_string()),
        command: "true".to_string(),
        args: vec![],
        autostart: true,
        cron: Some("* * * * * *".to_string()),
        ..Default::default()
    };
    let ids = handle.create_program(req).await.expect("Create failed");
    let id = ids[0];

    handle
        .update_program(
            id,
            common::UpdateProgramRequest {
                max_concurrent: Some(2),
                max_queued: Some(5),
                ..Default::default()
            },
        )
        .await
        .expect("Update failed");

    let info = handle.get_program(id).await.expect("Get failed");
    assert_eq!(info.config.max_concurrent, Some(2));
    assert_eq!(info.config.max_queued, Some(5));
    assert_eq!(info.config.max_concurrent_eff(), 2);
    assert_eq!(info.config.max_queued_eff(), 5);

    handle.shutdown().await.expect("Shutdown failed");
}
