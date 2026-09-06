//! Regression: dependency-gated spawns must not count toward flapping.
//!
//! Regression scenario: a program with `depends_on` sits in the
//! Waiting queue while its dependency boots. Each Waiting→spawn retry used to be
//! recorded as a start by the flapping tracker (the check ran before the
//! dependency gate), so the dependent could be marked Fatal the moment its
//! dependency finally became healthy — even though it never actually started.
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

async fn setup_manager() -> (ManagerHandle, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_file = temp_dir.path().join("super.toml");
    let mut config = test_helpers::test_server_config(&temp_dir);
    // Aggressive: 2+ starts within 60s are considered flapping. A slow dep boot
    // would previously blow through this while only ever *waiting*.
    config.server.flapping_window = 60;
    config.server.flapping_threshold = 2;

    let (log_tx, _) = broadcast::channel(100);
    let (tx, rx) = mpsc::channel(64);
    let event_db = super_core::event_db::EventDb::open(&config.storage.events_file)
        .await
        .unwrap();
    let manager = Manager::new(
        config,
        config_file,
        Box::new(|_| Ok(())),
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

async fn wait_state(handle: &ManagerHandle, id: uuid::Uuid, want: &[ProcessStatus], secs: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let state = handle
            .get_program(id)
            .await
            .map(|p| p.state)
            .unwrap_or(ProcessStatus::Stopped);
        if want.contains(&state) {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "program {id} never reached {want:?} (currently {state:?})"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// A dependent program must not be flagged Fatal by Waiting retries while its
/// dependency starts; once the dependency is up, the dependent runs normally.
#[tokio::test]
async fn waiting_retries_do_not_count_as_starts() {
    let (handle, _tmp) = setup_manager().await;

    // Dependency boots slowly so the dependent parks in Waiting repeatedly.
    let dep = {
        let req = CreateProgramRequest {
            name: Some("slow-db".to_string()),
            command: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "sleep 2; exec sleep 300".to_string()],
            autostart: false,
            ..Default::default()
        };
        handle.create_program(req).await.expect("create dep")[0]
    };
    let app = {
        let req = CreateProgramRequest {
            name: Some("app".to_string()),
            command: "/bin/sleep".to_string(),
            args: vec!["300".to_string()],
            autostart: false,
            depends_on: vec!["slow-db".to_string()],
            ..Default::default()
        };
        handle.create_program(req).await.expect("create app")[0]
    };

    handle.start_program(app).await.expect("start app");

    // Dependency and dependent both end up Running/Healthy…
    wait_state(
        &handle,
        dep,
        &[ProcessStatus::Running, ProcessStatus::Healthy],
        15,
    )
    .await;
    wait_state(
        &handle,
        app,
        &[ProcessStatus::Running, ProcessStatus::Healthy],
        15,
    )
    .await;

    // …and the dependent must NOT have been Fatal'ed on the way.
    let info = handle.get_program(app).await.expect("get app");
    assert_ne!(
        info.state,
        ProcessStatus::Fatal,
        "dependent was flapping-fataled by Waiting retries"
    );
}

/// Flapping detection still works for genuinely crash-looping programs.
#[tokio::test]
async fn crash_loop_still_fatales() {
    let (handle, _tmp) = setup_manager().await;

    let bad = {
        let req = CreateProgramRequest {
            name: Some("bad".to_string()),
            // exits instantly with code 1 on every run
            command: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "exit 1".to_string()],
            autostart: false,
            retry_limit: 10,
            ..Default::default()
        };
        handle.create_program(req).await.expect("create bad")[0]
    };
    handle.start_program(bad).await.expect("start bad");
    wait_state(&handle, bad, &[ProcessStatus::Fatal], 15).await;
}
