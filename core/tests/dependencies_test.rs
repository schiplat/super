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

// --- dangling depends_on references -----------------------------------------

/// Layer 1: single-program create must reject an unknown depends_on name.
#[tokio::test]
async fn create_rejects_unknown_dependency() {
    let (handle, _tmp) = setup_manager().await;

    let err = handle
        .create_program(CreateProgramRequest {
            name: Some("nginx".to_string()),
            command: "/bin/sleep".to_string(),
            args: vec!["30".to_string()],
            depends_on: vec!["webs".to_string()],
            ..Default::default()
        })
        .await
        .expect_err("create with unknown dependency must fail");
    assert!(
        err.to_string().contains("webs") && err.to_string().contains("depends_on"),
        "{err}"
    );
}

/// Layer 1: stack apply rejects the batch when any depends_on is dangling,
/// and nothing from the batch is persisted.
#[tokio::test]
async fn stack_apply_rejects_unknown_dependency() {
    let (handle, _tmp) = setup_manager().await;

    let stack = common::StackApplyRequest {
        services: vec![
            CreateProgramRequest {
                name: Some("web".to_string()),
                command: "/bin/sleep".to_string(),
                args: vec!["30".to_string()],
                ..Default::default()
            },
            CreateProgramRequest {
                name: Some("nginx".to_string()),
                command: "/bin/sleep".to_string(),
                args: vec!["30".to_string()],
                depends_on: vec!["webs".to_string()],
                ..Default::default()
            },
        ],
        prune: false,
    };
    let err = handle
        .apply_stack(stack)
        .await
        .expect_err("apply with dangling dependency must fail");
    assert!(err.to_string().contains("webs"), "{err}");

    // Nothing persisted: neither the valid service nor the dangling one.
    let programs = handle.list_programs().await.expect("list failed");
    assert!(
        programs.is_empty(),
        "expected empty registry, got {programs:?}"
    );
}

/// Layer 1: forward references inside one batch are allowed — a service may
/// depend on another service defined later in the same stack.
#[tokio::test]
async fn stack_apply_allows_forward_reference() {
    let (handle, _tmp) = setup_manager().await;

    let stack = common::StackApplyRequest {
        services: vec![
            CreateProgramRequest {
                name: Some("nginx".to_string()),
                command: "/bin/sleep".to_string(),
                args: vec!["30".to_string()],
                depends_on: vec!["web".to_string()],
                ..Default::default()
            },
            CreateProgramRequest {
                name: Some("web".to_string()),
                command: "/bin/sleep".to_string(),
                args: vec!["30".to_string()],
                ..Default::default()
            },
        ],
        prune: false,
    };
    handle
        .apply_stack(stack)
        .await
        .expect("forward reference within the batch must be accepted");
}

/// Layer 2: a program whose dependency disappears after a legit start must,
/// on the next start, park in WAITING with a visible config-error last_error
/// (instead of the old silent path), and recover automatically once the
/// missing dependency exists again.
#[tokio::test]
async fn missing_dependency_waits_visibly_then_recovers() {
    let (handle, _tmp) = setup_manager().await;

    // Layer 1 requires the dependency to exist before the dependent is
    // created (single-program create has no forward references).
    let dep_id = create_sleep(&handle, "dep", vec![]).await;
    let app_id = create_sleep(&handle, "app", vec!["dep".to_string()]).await;

    // Sanity: with the dependency present everything runs.
    handle.start_program(app_id).await.expect("start app");
    wait_running_or_healthy(&handle, dep_id).await;
    wait_running_or_healthy(&handle, app_id).await;

    // Now remove the dependency program: the reference becomes dangling.
    handle.stop_program(app_id, true).await.expect("stop app");
    handle.stop_program(dep_id, true).await.expect("stop dep");
    wait_state_is(&handle, dep_id, ProcessStatus::Stopped).await;
    handle.remove_program(dep_id).await.expect("remove dep");

    // Layer 2 gate: starting the dependent must hold it in WAITING (never
    // spawn) and surface the dangling reference in last_error.
    handle
        .start_program(app_id)
        .await
        .expect("start returns Ok while WAITING");
    wait_state_is(&handle, app_id, ProcessStatus::Waiting).await;
    let info = handle.get_program(app_id).await.expect("get app");
    let last_error = info
        .last_error
        .expect("last_error must surface the dangling dependency");
    assert!(
        last_error.contains("dep") && last_error.contains("not found"),
        "{last_error}"
    );

    // Recreate the missing dependency and start it → the healthy transition
    // must re-trigger the waiting queue and recover the dependent.
    let dep2 = create_sleep(&handle, "dep", vec![]).await;
    handle.start_program(dep2).await.expect("start dep");
    wait_running_or_healthy(&handle, dep2).await;
    wait_running_or_healthy(&handle, app_id).await;

    // Recovery clears the config error.
    let info = handle.get_program(app_id).await.expect("get app");
    assert!(info.last_error.is_none(), "{:?}", info.last_error);
}

/// Layer 3: removing a program that a WAITING dependent references must
/// refresh the dependent's last_error with a removal notice.
#[tokio::test]
async fn removing_dependency_updates_waiting_dependent() {
    let (handle, _tmp) = setup_manager().await;

    let dep1 = create_sleep(&handle, "dep-one", vec![]).await;
    let dep2 = create_sleep(&handle, "dep-two", vec![]).await;
    let app_id = create_sleep(
        &handle,
        "app",
        vec!["dep-one".to_string(), "dep-two".to_string()],
    )
    .await;

    // Everything healthy…
    handle.start_program(app_id).await.expect("start app");
    wait_running_or_healthy(&handle, dep1).await;
    wait_running_or_healthy(&handle, dep2).await;
    wait_running_or_healthy(&handle, app_id).await;

    // …then dep-one is removed outright; app keeps a dangling reference.
    handle.stop_program(app_id, true).await.expect("stop app");
    handle.stop_program(dep1, true).await.expect("stop dep1");
    wait_state_is(&handle, dep1, ProcessStatus::Stopped).await;
    handle.remove_program(dep1).await.expect("remove dep1");

    // Restart app → layer 2 gate parks it in WAITING citing dep-one.
    handle.start_program(app_id).await.expect("start app");
    wait_state_is(&handle, app_id, ProcessStatus::Waiting).await;

    // Layer 3: while app is WAITING on dep-two, remove dep-two → the
    // dependent's last_error must be refreshed with the removal notice.
    handle.stop_program(dep2, true).await.expect("stop dep2");
    wait_state_is(&handle, dep2, ProcessStatus::Stopped).await;
    handle.remove_program(dep2).await.expect("remove dep2");

    let info = handle.get_program(app_id).await.expect("get app");
    let last_error = info
        .last_error
        .expect("last_error must be set after dependency removal");
    assert!(
        last_error.contains("dep-two") && last_error.contains("removed"),
        "{last_error}"
    );
}

async fn wait_state_is(handle: &ManagerHandle, id: Uuid, want: ProcessStatus) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let state = handle
            .get_program(id)
            .await
            .map(|p| p.state)
            .unwrap_or(ProcessStatus::Stopped);
        if state == want {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "program {id} never reached {want:?} (currently {state:?})"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
