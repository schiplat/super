use common::{
    ArtifactConfig, CreateProgramRequest, ProcessStatus, ProgramConfig, UpdateProgramRequest,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use super_core::ManagerHandle;
use super_core::extension::NoOpExtension;
use super_core::manager::{Command, Manager};
use tempfile::TempDir;
use tokio::sync::{broadcast, mpsc};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[path = "test_helpers.rs"]
mod test_helpers;

// + Helpers +

fn calculate_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

async fn setup_system() -> (ManagerHandle, TempDir, PathBuf, PathBuf) {
    let (handle, tmp, target_bin, data_file, _tx) = setup_system_full().await;
    (handle, tmp, target_bin, data_file)
}

async fn setup_system_full() -> (
    ManagerHandle,
    TempDir,
    PathBuf,
    PathBuf,
    mpsc::Sender<Command>,
) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    let config_file = root.join("super.toml");
    let target_bin = root.join("my_app");

    // 1. Create initial version (v1)
    let mut f = std::fs::File::create(&target_bin).unwrap();
    f.write_all(b"VERSION_1").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&target_bin).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&target_bin, perms).unwrap();
    }

    let mut config = test_helpers::test_server_config(&temp_dir);
    config.server.shutdown_timeout = 1;
    let data_file = config.storage.data_file.clone();

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
        Box::new(NoOpExtension),
        event_db,
    );

    tokio::spawn(async move {
        manager.run().await;
    });

    (
        ManagerHandle::new(tx.clone()),
        temp_dir,
        target_bin,
        data_file,
        tx,
    )
}

// + Test cases +

#[tokio::test]
async fn test_ota_transaction_rollback() {
    let (handle, _tmp, target_bin, data_file) = setup_system().await;
    let mock_server = MockServer::start().await;

    // 1. Prepare v2
    let v2_content = "VERSION_2_NEW";
    let v2_hash = calculate_hash(v2_content);

    Mock::given(method("GET"))
        .and(path("/download/v2"))
        .respond_with(ResponseTemplate::new(200).set_body_string(v2_content))
        .mount(&mock_server)
        .await;

    // 2. Run v1
    let req = CreateProgramRequest {
        name: Some("app-rollback".to_string()),
        command: "sleep".to_string(),
        args: vec!["100".to_string()],
        autostart: true,
        ..Default::default()
    };
    let ids = handle.create_program(req).await.unwrap();
    let id = ids[0];

    tokio::time::sleep(Duration::from_millis(200)).await;
    let info_v1 = handle.get_program(id).await.unwrap();
    let pid_v1 = info_v1.pid.expect("Process v1 should have PID");
    assert_eq!(info_v1.state, ProcessStatus::Healthy);

    // 3. Trigger OTA update
    println!(">>> Triggering Update...");
    let update_req = UpdateProgramRequest {
        artifact: Some(ArtifactConfig {
            source: format!("{}/download/v2", mock_server.uri()),
            checksum: v2_hash,
            extract: false,
            destination: target_bin.to_string_lossy().to_string(),
            restart_policy: "immediate".to_string(),
        }),
        ..Default::default()
    };
    handle.update_program(id, update_req).await.unwrap();

    // 4. Wait for verification phase (restore_path persisted) and PID change (process restart)
    let mut verified_phase_reached = false;
    let mut new_pid = None;

    // Poll longer to allow download and restart to finish
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Check on-disk state
        let content = std::fs::read_to_string(&data_file).unwrap_or_default();
        if content.is_empty() {
            continue;
        }

        let saved_state: HashMap<uuid::Uuid, ProgramConfig> =
            serde_json::from_str(&content).unwrap_or_default();

        if let Some(cfg) = saved_state.get(&id) {
            // Condition 1: restore_path recorded on disk (state machine entered verification)
            if cfg.restore_path.is_some() {
                // Condition 2: process restarted (PID changed)
                // Must fetch the latest in-memory PID via the API
                if let Ok(info) = handle.get_program(id).await
                    && let Some(p) = info.pid
                    && p != pid_v1
                {
                    verified_phase_reached = true;
                    new_pid = Some(p);
                    break;
                }
            }
        }
    }

    assert!(
        verified_phase_reached,
        "Manager failed to enter verification phase or restart process"
    );
    let pid_v2 = new_pid.expect("New PID should exist");
    println!(">>> Process restarted: PID {} -> PID {}", pid_v1, pid_v2);

    // Verify: on-disk file should be v2
    let current_content = std::fs::read_to_string(&target_bin).unwrap();
    assert_eq!(current_content, v2_content, "File should be swapped to v2");

    // Verify: backup file should exist
    let backup_path = target_bin.with_extension("bak");
    assert!(backup_path.exists(), "Backup file missing");

    // 5. Simulate new process crash (kill PID v2)
    // restore_path is set and this is not a user stop → should trigger rollback
    println!(">>> Simulating Crash on PID {}...", pid_v2);
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid_v2 as i32),
        nix::sys::signal::Signal::SIGKILL,
    )
    .unwrap();

    // 6. Wait for rollback to complete
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 7. Final verification
    // A. File rolled back to v1
    let restored_content = std::fs::read_to_string(&target_bin).unwrap();
    assert_eq!(
        restored_content, "VERSION_1",
        "Rollback failed! Content mismatch"
    );

    // B. Backup file should be gone (renamed back to target)
    assert!(!backup_path.exists(), "Backup file should be consumed");

    // C. restore_path cleared
    let saved_state_final: HashMap<uuid::Uuid, ProgramConfig> =
        serde_json::from_str(&std::fs::read_to_string(&data_file).unwrap()).unwrap();
    assert!(
        saved_state_final.get(&id).unwrap().restore_path.is_none(),
        "restore_path should be cleared"
    );

    println!("Test Passed: Rollback successful.");
}

#[tokio::test]
async fn test_ota_transaction_commit() {
    let (handle, _tmp, target_bin, data_file) = setup_system().await;
    let mock_server = MockServer::start().await;

    let v2_content = "VERSION_2_COMMIT";
    let v2_hash = calculate_hash(v2_content);

    Mock::given(method("GET"))
        .and(path("/download/v2"))
        .respond_with(ResponseTemplate::new(200).set_body_string(v2_content))
        .mount(&mock_server)
        .await;

    // 1. Register (exec "true" simulates passing health check)
    let req = CreateProgramRequest {
        name: Some("app-commit".to_string()),
        command: "sleep".to_string(),
        args: vec!["100".to_string()],
        autostart: true,
        health_check: Some(common::HealthCheck::Exec {
            command: "true".to_string(),
            interval_secs: 0,
            timeout_secs: 0,
            start_period_secs: 0,
            max_failures: 0,
        }),
        ..Default::default()
    };
    let ids = handle.create_program(req).await.unwrap();
    let id = ids[0];

    tokio::time::sleep(Duration::from_millis(500)).await;

    // 2. Trigger update
    println!(">>> Triggering Update...");
    let update_req = UpdateProgramRequest {
        artifact: Some(ArtifactConfig {
            source: format!("{}/download/v2", mock_server.uri()),
            checksum: v2_hash,
            extract: false,
            destination: target_bin.to_string_lossy().to_string(),
            restart_policy: "immediate".to_string(),
        }),
        ..Default::default()
    };
    handle.update_program(id, update_req).await.unwrap();

    // 3. Wait for commit (restore_path cleared & healthy state)
    let backup_path = target_bin.with_extension("bak");
    let mut commit_done = false;

    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(500)).await;

        let info = handle.get_program(id).await.unwrap();
        // Commit is complete only when restore_path is empty and state is Healthy
        if info.config.restore_path.is_none() && info.state == ProcessStatus::Healthy {
            commit_done = true;
            break;
        }
    }
    assert!(
        commit_done,
        "Upgrade did not commit (restore_path did not clear)"
    );

    // 4. Verify
    // A. File is v2
    let current_content = std::fs::read_to_string(&target_bin).unwrap();
    assert_eq!(current_content, v2_content, "File should be v2");

    // B. Backup deleted
    assert!(!backup_path.exists(), "Backup file should be deleted");

    // C. On-disk state consistent
    let saved_state: HashMap<uuid::Uuid, ProgramConfig> =
        serde_json::from_str(&std::fs::read_to_string(&data_file).unwrap()).unwrap();
    assert!(saved_state.get(&id).unwrap().restore_path.is_none());

    println!("Test Passed: Commit successful.");
}

// Regression test: an OTA-ready command for a program whose config has no
// artifact (or an empty destination) must be logged and skipped, not panic.
#[tokio::test]
async fn test_ota_ready_without_artifact_does_not_panic() {
    let (handle, _tmp, _target_bin, _data_file, tx) = setup_system_full().await;

    // Program without any artifact config.
    let req = CreateProgramRequest {
        name: Some("app-no-artifact".to_string()),
        command: "sleep".to_string(),
        args: vec!["100".to_string()],
        autostart: false,
        ..Default::default()
    };
    let ids = handle.create_program(req).await.unwrap();
    let id = ids[0];

    // Directly inject the internal OTA-ready command (normally only sent after
    // a successful download). With no artifact in config this used to panic
    // the Manager actor on `config.artifact.as_ref().unwrap()`.
    tx.send(Command::InternalArtifactReady {
        id,
        path: PathBuf::from("/nonexistent/staging/file"),
    })
    .await
    .unwrap();

    // The Manager must still be alive and answering commands.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let info = handle.get_program(id).await.unwrap();
    assert_eq!(info.config.name, "app-no-artifact");
    assert!(info.config.restore_path.is_none());
}
