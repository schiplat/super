use chrono::{TimeZone, Utc};
use std::thread;
use std::time::Duration;
use super_core::scheduler::CronScheduler;
use uuid::Uuid;

#[test]
fn test_scheduler_logic() {
    let mut scheduler = CronScheduler::new();
    let id = Uuid::new_v4();

    // Register a task that runs every second
    let cron_expr = "0/1 * * * * * *";

    scheduler.upsert(id, cron_expr, 0, None);

    assert!(
        scheduler.get_next_run(&id).is_some(),
        "Should have next run time"
    );

    // Should not be due immediately after registration
    let triggered = scheduler.tick();
    assert!(triggered.is_empty(), "Should not trigger immediately");

    println!("⏳ Waiting for cron tick...");
    thread::sleep(Duration::from_secs(2));

    let triggered = scheduler.tick();
    assert_eq!(triggered.len(), 1, "Should trigger after 1 second");
    assert_eq!(triggered[0].id, id);
    assert!(
        triggered[0].missed_slots >= 1,
        "Missed slots should be >= 1"
    );

    // Verify automatic rescheduling
    let triggered_again = scheduler.tick();
    assert!(
        triggered_again.is_empty(),
        "Should not trigger twice instantly"
    );
    assert!(
        scheduler.get_next_run(&id).is_some(),
        "Should have rescheduled"
    );
}

/// A long idle period must surface as `missed_slots > 1` so the manager can
/// apply the catchup policy instead of silently dropping the lost runs.
#[test]
fn test_scheduler_counts_missed_slots() {
    let mut scheduler = CronScheduler::new();
    let id = Uuid::new_v4();
    scheduler.upsert(id, "0/1 * * * * * *", 0, None);

    // Let several slots elapse without ticking (simulates a daemon outage).
    thread::sleep(Duration::from_millis(2800));

    let triggered = scheduler.tick();
    assert_eq!(triggered.len(), 1, "One task became due");
    assert!(
        triggered[0].missed_slots >= 2,
        "Expected >=2 missed slots after a 2.8s pause, got {}",
        triggered[0].missed_slots
    );
}

/// Re-registering after a restart with a `last_run` anchor must count the slots
/// between the anchor and now as missed, enabling the catchup policy.
#[test]
fn test_scheduler_restart_recovers_missed_slots_from_last_run() {
    let mut scheduler = CronScheduler::new();
    let id = Uuid::new_v4();
    let expr = "0/1 * * * * * *";

    // First run happened 3 seconds ago; daemon was down until now.
    let last_run = Utc::now() - chrono::Duration::seconds(3);
    scheduler.upsert(id, expr, 0, Some(last_run));

    let triggered = scheduler.tick();
    assert_eq!(triggered.len(), 1, "Recovered task becomes due immediately");
    assert!(
        triggered[0].missed_slots >= 3,
        "Expected >=3 missed slots from last_run, got {}",
        triggered[0].missed_slots
    );
}

/// A live scheduler (ticking every <1s) must never report the jitter window as
/// missed slots: the delayed trigger absorbs the slots in between.
#[test]
fn test_scheduler_jitter_does_not_create_false_missed() {
    let mut scheduler = CronScheduler::new();
    let id = Uuid::new_v4();
    scheduler.upsert(id, "0/1 * * * * * *", 3, None);

    let mut fired = false;
    for _ in 0..8 {
        thread::sleep(Duration::from_millis(700));
        for t in scheduler.tick() {
            assert_eq!(
                t.missed_slots, 1,
                "jitter must not count absorbed slots as missed, got {}",
                t.missed_slots
            );
            fired = true;
        }
        if fired {
            break;
        }
    }
    assert!(fired, "task should fire within the jitter window");
}

/// Jitter must keep the actual trigger deadline within [now, now + slot + jitter]
/// and must never prevent the trigger from firing (the window is bounded).
#[test]
fn test_scheduler_jitter_stays_within_window() {
    let mut scheduler = CronScheduler::new();
    let id = Uuid::new_v4();
    let jitter_sec = 4u64;
    scheduler.upsert(id, "0/1 * * * * * *", jitter_sec, None);

    let now = Utc::now();
    let next = scheduler
        .get_next_run(&id)
        .expect("task should be registered");

    // The deadline must be in the future and bounded by the next slot (<=1s
    // away) plus the full jitter window.
    let upper = now + chrono::Duration::seconds(1 + jitter_sec as i64 + 1);
    assert!(
        next >= now && next <= upper,
        "Jittered deadline {next} should be in [{now}, {upper}]"
    );

    // Wait past the worst-case deadline (slot + jitter); the task must fire.
    thread::sleep(Duration::from_millis((1 + jitter_sec) * 1000 + 400));
    let triggered = scheduler.tick();
    assert_eq!(triggered.len(), 1, "Should trigger once after the window");
    assert_eq!(triggered[0].id, id);
}

/// `catchup` semantics are decided by the manager from `missed_slots`; here we
/// only lock in that a one-shot past schedule is exhausted and dropped rather
/// than stuck forever.
#[test]
fn test_scheduler_drops_exhausted_schedule() {
    let mut scheduler = CronScheduler::new();
    let id = Uuid::new_v4();
    // A one-shot schedule in the past: never becomes due again.
    scheduler.upsert(id, "0 0 1 1 * * 2000", 0, None);
    assert!(
        scheduler.get_next_run(&id).is_none(),
        "Past one-shot schedule should not register"
    );
}

/// Timezone helper is not used by the core logic; kept for clarity in tests
/// that might want to construct explicit instants.
#[allow(dead_code)]
fn dt(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(y, mo, d, h, mi, s)
        .single()
        .expect("valid instant")
}
