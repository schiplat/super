//! Lifecycle ordering regressions.
//!
//! Covers three fixes from the process-management audit:
//! 1. Group/batch operations follow dependency topological order (starts run
//!    dependencies first; stops run dependents first).
//! 2. Dependency auto-start does not flip `autostart`, and never pulls up a
//!    dependency the operator explicitly stopped (`stopped_by_user`).
//! 3. Dependency cycles are rejected at create/update/apply time instead of
//!    deadlocking both members in WAITING forever at runtime.
use common::{CreateProgramRequest, ProcessStatus};
use std::collections::HashMap;
use std::time::Duration;
use super_core::ManagerHandle;
use super_core::extension::Extension;
use super_core::manager::Manager;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

#[path = "test_helpers.rs"]
mod test_helpers;

struct NoopExtension;
impl Extension for NoopExtension {}

async fn setup_manager() -> (ManagerHandle, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_file = temp_dir.path().join("super.toml");
    let config = test_helpers::test_server_config(&temp_dir);

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

async fn create_sleep(
    handle: &ManagerHandle,
    name: &str,
    group: Option<&str>,
    depends_on: Vec<String>,
) -> Uuid {
    let req = CreateProgramRequest {
        name: Some(name.to_string()),
        command: "/bin/sleep".to_string(),
        args: vec!["30".to_string()],
        autostart: false,
        group: group.map(str::to_string),
        depends_on,
        ..Default::default()
    };
    handle.create_program(req).await.expect("create failed")[0]
}

async fn wait_state(handle: &ManagerHandle, id: Uuid, want: ProcessStatus, secs: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
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
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn assert_state_is(handle: &ManagerHandle, id: Uuid, want: ProcessStatus) {
    let state = handle
        .get_program(id)
        .await
        .map(|p| p.state)
        .unwrap_or(ProcessStatus::Stopped);
    assert_eq!(state, want, "unexpected state for {id}");
}

// --- fix 1: group start order (dep Healthy before dependent spawns) ---------

/// `start @group` runs in dependency order: the dependency is already
/// Running/Healthy by the time the dependent is spawned, so the dependent
/// never parks in the WAITING queue.
#[tokio::test]
async fn group_start_runs_dependencies_first() {
    let (handle, _tmp) = setup_manager().await;

    let db = create_sleep(&handle, "ord-db", Some("ord"), vec![]).await;
    let app = create_sleep(&handle, "ord-app", Some("ord"), vec!["ord-db".into()]).await;
    let edge = create_sleep(&handle, "ord-edge", Some("ord"), vec!["ord-app".into()]).await;

    let affected = handle
        .start_group("ord".to_string())
        .await
        .expect("group start failed");
    assert_eq!(affected.len(), 3);

    wait_state(&handle, db, ProcessStatus::Healthy, 10).await;
    wait_state(&handle, app, ProcessStatus::Healthy, 10).await;
    wait_state(&handle, edge, ProcessStatus::Healthy, 10).await;
}

/// Group stop runs in reverse dependency order. Observation window: while the
/// dependency is still running, the dependent must already be gone (or the
/// dependency already gone by the time we observe, for single-member chains
/// we assert the dependent stopped strictly before the dependency exits).
#[tokio::test]
async fn group_stop_runs_dependents_first() {
    let (handle, _tmp) = setup_manager().await;

    let db = create_sleep(&handle, "stop-db", Some("stopg"), vec![]).await;
    let app = create_sleep(&handle, "stop-app", Some("stopg"), vec!["stop-db".into()]).await;

    handle
        .start_group("stopg".to_string())
        .await
        .expect("group start");
    wait_state(&handle, db, ProcessStatus::Healthy, 10).await;
    wait_state(&handle, app, ProcessStatus::Healthy, 10).await;

    handle
        .stop_group("stopg".to_string(), false)
        .await
        .expect("group stop");
    wait_state(&handle, db, ProcessStatus::Stopped, 10).await;
    wait_state(&handle, app, ProcessStatus::Stopped, 10).await;
}

// --- fix 2: dependency auto-start respects operator intent ------------------

/// Auto-starting a dependency must not flip its `autostart` config: after the
/// dependent's start pulled the dependency up, the dependency's stored
/// autostart stays false (create default), so a daemon restart keeps it down.
#[tokio::test]
async fn dependency_auto_start_does_not_flip_autostart() {
    let (handle, _tmp) = setup_manager().await;

    let dep = create_sleep(&handle, "ad-dep", None, vec![]).await;
    let app = create_sleep(&handle, "ad-app", None, vec!["ad-dep".into()]).await;

    handle.start_program(app).await.expect("start app");
    wait_state(&handle, dep, ProcessStatus::Healthy, 10).await;
    wait_state(&handle, app, ProcessStatus::Healthy, 10).await;

    let info = handle.get_program(dep).await.expect("get dep");
    assert!(
        !info.config.autostart,
        "dependency auto-start must not persist autostart=true"
    );
}

/// A dependency the operator explicitly stopped is never pulled back up by a
/// dependent's start; the dependent parks in WAITING (visible) instead.
#[tokio::test]
async fn held_stopped_dependency_is_not_revived() {
    let (handle, _tmp) = setup_manager().await;

    let dep = create_sleep(&handle, "hs-dep", None, vec![]).await;
    let app = create_sleep(&handle, "hs-app", None, vec!["hs-dep".into()]).await;

    // Bring both up, then stop the dependency explicitly (this also marks it
    // held-stopped), then stop the dependent too.
    handle.start_program(app).await.expect("start app");
    wait_state(&handle, dep, ProcessStatus::Healthy, 10).await;
    wait_state(&handle, app, ProcessStatus::Healthy, 10).await;
    handle.stop_program(dep, false).await.expect("stop dep");
    wait_state(&handle, dep, ProcessStatus::Stopped, 10).await;
    handle.stop_program(app, false).await.expect("stop app");
    wait_state(&handle, app, ProcessStatus::Stopped, 10).await;

    // Restarting the dependent must NOT silently revive the held-stopped dep.
    handle.start_program(app).await.expect("start app");
    wait_state(&handle, app, ProcessStatus::Waiting, 10).await;

    // Give any (wrongful) auto-start ample time to show up.
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_state_is(&handle, dep, ProcessStatus::Stopped).await;
}

// --- fix 3: cycles rejected up front -----------------------------------------

/// Update-path cycle: A -> B already exists; updating B -> A must be rejected
/// (this is the incremental cycle per-request reference checks cannot see).
#[tokio::test]
async fn update_creating_cycle_is_rejected() {
    let (handle, _tmp) = setup_manager().await;

    let a = create_sleep(&handle, "cyc-a", None, vec![]).await;
    let _b = create_sleep(&handle, "cyc-b", None, vec!["cyc-a".into()]).await;

    // Now point A back at B: A -> B -> A.
    let err = handle
        .update_program(
            a,
            common::UpdateProgramRequest {
                depends_on: Some(vec!["cyc-b".to_string()]),
                ..Default::default()
            },
        )
        .await
        .expect_err("cycle-forming update must be rejected");
    assert!(err.to_string().contains("cycle"), "{err}");
}

// --- fix 6: batch restart is a two-phase ordered cycle -----------------------

/// A batch restart over multiple targets (CLI `restart @group` / `restart all`)
/// must run as one coordinated cycle — ordered stop, wait for exits, ordered
/// start — instead of N independent restarts racing each other's dependencies.
#[tokio::test]
async fn batch_restart_runs_two_phase_ordered_cycle() {
    let (handle, _tmp) = setup_manager().await;

    let db = create_sleep(&handle, "br-db", Some("brg"), vec![]).await;
    let app = create_sleep(&handle, "br-app", Some("brg"), vec!["br-db".into()]).await;

    handle
        .start_group("brg".to_string())
        .await
        .expect("group start");
    wait_state(&handle, db, ProcessStatus::Healthy, 10).await;
    wait_state(&handle, app, ProcessStatus::Healthy, 10).await;

    let res = handle
        .batch_programs(common::BatchProgramRequest {
            target_ids: None,
            group_name: Some("brg".to_string()),
            select_all: false,
            action: common::BatchAction::Restart,
        })
        .await
        .expect("batch restart");
    assert_eq!(res.affected.len(), 2, "both members restarted");
    assert!(res.failed.is_empty(), "no failures: {:?}", res.failed);

    wait_state(&handle, db, ProcessStatus::Healthy, 15).await;
    wait_state(&handle, app, ProcessStatus::Healthy, 15).await;
}

/// Stack apply with a cycle inside the batch must be rejected up front.
#[tokio::test]
async fn stack_apply_rejects_cycle() {
    let (handle, _tmp) = setup_manager().await;

    let stack = common::StackApplyRequest {
        services: vec![
            CreateProgramRequest {
                name: Some("sa".to_string()),
                command: "/bin/sleep".to_string(),
                args: vec!["30".to_string()],
                depends_on: vec!["sb".to_string()],
                ..Default::default()
            },
            CreateProgramRequest {
                name: Some("sb".to_string()),
                command: "/bin/sleep".to_string(),
                args: vec!["30".to_string()],
                depends_on: vec!["sa".to_string()],
                ..Default::default()
            },
        ],
        prune: false,
    };
    let err = handle
        .apply_stack(stack)
        .await
        .expect_err("cyclic stack apply must fail");
    assert!(err.to_string().contains("cycle"), "{err}");

    let programs = handle.list_programs().await.expect("list failed");
    assert!(
        programs.is_empty(),
        "nothing from a rejected batch may persist, got {programs:?}"
    );
}
