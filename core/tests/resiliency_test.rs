use anyhow::anyhow;
use common::{ProcessStatus, ProgramConfig};
use std::collections::HashMap;
use super_core::ManagerHandle;
use super_core::config::ServerConfig;
use super_core::extension::Extension;
use super_core::manager::Manager;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

// --- Mock Extension ---
// Extension that can be configured to fail
struct SaboteurExtension {
    pub should_fail_after_start: bool,
}

impl Extension for SaboteurExtension {
    fn after_start(&self, _id: Uuid, _pid: u32, _config: &ProgramConfig) -> anyhow::Result<()> {
        if self.should_fail_after_start {
            // Simulate cgroup mount failure
            return Err(anyhow!("Simulated Cgroup Failure!"));
        }
        Ok(())
    }
}

// [Fixed] Removed unused `setup_manager` helper function to clear warnings.

#[tokio::test]
async fn test_strict_policy_kills_process() {
    // 1. Set up environment: extension always fails
    // [Fixed] Removed unused `tx` declaration
    let (log_tx, _) = broadcast::channel(100);

    let temp_dir = tempfile::tempdir().unwrap();
    let data_file = temp_dir.path().join("data.json");
    let mut config = ServerConfig::default();
    config.storage.data_file = data_file.clone();

    // Extension returns an error from after_start
    let extension = Box::new(SaboteurExtension {
        should_fail_after_start: true,
    });

    let (cmd_tx, cmd_rx) = mpsc::channel(100);

    let manager = Manager::new(
        config,
        temp_dir.path().join("super.toml"),
        Box::new(|_| Ok(())),
        cmd_rx,
        cmd_tx.clone(),
        HashMap::new(),
        HashMap::new(),
        log_tx,
        extension,
    );

    tokio::spawn(async move {
        manager.run().await;
    });

    let handle = ManagerHandle::new(cmd_tx);

    // 2. Create and start a task (sleep command)
    let req = common::CreateProgramRequest {
        name: Some("victim".to_string()),
        command: "sleep".to_string(),
        args: vec!["100".to_string()], // sleep for 100 seconds
        autostart: true,
        ..Default::default()
    };

    let ids = handle.create_program(req).await.expect("Create failed");
    let id = ids[0];

    // Allow time for spawn → extension → kill to complete
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // 3. Verify results
    let info = handle.get_program(id).await.expect("Get info failed");

    println!("Status after sabotage: {:?}", info.state);

    // Key check 1: status must be Fatal
    assert_eq!(
        info.state,
        ProcessStatus::Fatal,
        "Process should be Fatal after extension failure"
    );

    // Key check 2: PID should be cleared (we SIGKILL and wait)
    assert!(
        info.pid.is_none(),
        "PID should be cleared from running state"
    );

    // Key check 3: error message should include our mock error
    assert!(info.last_error.is_some());
    assert!(
        info.last_error
            .unwrap()
            .contains("Simulated Cgroup Failure")
    );
}
