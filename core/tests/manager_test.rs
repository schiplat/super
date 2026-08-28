use common::{ProcessStatus, ProgramConfig, SystemEvent};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use super_core::ManagerHandle;
use super_core::config::ServerConfig;
use super_core::extension::Extension;
use super_core::manager::Manager;
use tempfile::TempDir;
use tokio::sync::{broadcast, mpsc};

// + Mock components +

#[derive(Clone)]
struct MockExtension {
    pub events: Arc<Mutex<Vec<SystemEvent>>>,
}

impl MockExtension {
    fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn has_event(&self, event_type: &str) -> bool {
        let events = self.events.lock().unwrap();
        events.iter().any(|e| e.event_type() == event_type)
    }
}

impl Extension for MockExtension {
    fn on_event(&self, event: SystemEvent) {
        let mut events = self.events.lock().unwrap();
        events.push(event);
    }
}

// + Test setup +

async fn setup_manager() -> (ManagerHandle, TempDir, MockExtension) {
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join("super.toml");

    let mut config = ServerConfig::default();
    config.storage.data_file = temp_dir.path().join("snapshot.json");
    config.storage.log_dir = temp_dir.path().join("logs");
    config.child_logging.max_size_mb = 1;
    config.child_logging.max_backups = 1;

    let extension = MockExtension::new();
    let (log_tx, _) = broadcast::channel(100);
    let (tx, rx) = mpsc::channel(32);

    let log_reloader = Box::new(|_| Ok(()));

    let manager = Manager::new(
        config,
        config_file,
        log_reloader,
        rx,
        tx.clone(),
        HashMap::new(),
        log_tx,
        Box::new(extension.clone()),
    );

    tokio::spawn(async move {
        manager.run().await;
    });

    (ManagerHandle::new(tx), temp_dir, extension)
}

// + Test cases +

#[tokio::test]
async fn test_basic_lifecycle() {
    let (handle, _tmp, _notify) = setup_manager().await;

    // When building ProgramConfig manually, all fields must be set
    let config = ProgramConfig {
        name: "test-lifecycle".to_string(),
        command: "/bin/sleep".to_string(),
        args: vec!["5".to_string()],
        env: HashMap::new(),
        cwd: None,
        user: None,
        autostart: false,
        retry_limit: 0,
        group: None,
        depends_on: vec![],
        health_check: None,
        hooks: Default::default(),
        artifact: None,
        created_at: 0,
        updated_at: 0,
        cron: None,
        restore_path: None,

        ..Default::default()
    };

    let ids = handle
        .create_program(common::CreateProgramRequest {
            name: Some(config.name),
            command: config.command,
            args: config.args,
            autostart: false,
            ..Default::default()
        })
        .await
        .unwrap();

    let id = ids[0];

    // 2. Verify initial state: Stopped
    let info = handle.get_program(id).await.unwrap();
    assert_eq!(info.state, ProcessStatus::Stopped);

    // 3. Start
    handle.start_program(id).await.unwrap();

    // 4. Wait for spawn to complete
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 5. Verify running state
    let info = handle.get_program(id).await.unwrap();
    println!("Lifecycle State: {:?}", info.state);
    assert!(matches!(
        info.state,
        ProcessStatus::Running | ProcessStatus::Healthy
    ));

    // 6. Stop
    handle.stop_program(id, false).await.unwrap();

    // Give the Tokio spawn wait task time to finish
    tokio::time::sleep(Duration::from_millis(200)).await;

    let info = handle.get_program(id).await.unwrap();
    assert_eq!(info.state, ProcessStatus::Stopped);
}

#[tokio::test]
async fn test_dependency_orchestration() {
    let (handle, _tmp, _notify) = setup_manager().await;

    // A. Upstream service (provider)
    handle
        .create_program(common::CreateProgramRequest {
            name: Some("provider".to_string()),
            command: "/bin/sleep".to_string(),
            args: vec!["100".to_string()],
            autostart: false,
            ..Default::default()
        })
        .await
        .unwrap();

    // B. Downstream service (consumer) — depends on provider
    let consumer_ids = handle
        .create_program(common::CreateProgramRequest {
            name: Some("consumer".to_string()),
            command: "/bin/echo".to_string(),
            args: vec!["hello".to_string()],
            autostart: false,
            depends_on: vec!["provider".to_string()],
            ..Default::default()
        })
        .await
        .unwrap();
    let consumer_id = consumer_ids[0];

    // 1. Start consumer first → should enter Waiting (provider not running)
    handle.start_program(consumer_id).await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;
    let info = handle.get_program(consumer_id).await.unwrap();
    assert_eq!(
        info.state,
        ProcessStatus::Waiting,
        "Consumer should wait for provider"
    );

    // 2. Start provider
    let list = handle.list_programs().await.unwrap();
    let provider_id = list.iter().find(|p| p.name == "provider").unwrap().id;
    handle.start_program(provider_id).await.unwrap();

    // 3. Wait for provider to become Healthy and trigger scheduling
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 4. Verify consumer state
    let info = handle.get_program(consumer_id).await.unwrap();
    println!("Consumer State after provider start: {:?}", info.state);
    assert_ne!(
        info.state,
        ProcessStatus::Waiting,
        "Consumer should have been triggered"
    );
}

#[tokio::test]
async fn test_fatal_alert() {
    let (handle, _tmp, notify) = setup_manager().await;

    // Create a task that is guaranteed to fail
    handle
        .create_program(common::CreateProgramRequest {
            name: Some("crasher".to_string()),
            command: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "exit 1".to_string()],
            autostart: true,
            retry_limit: 1,
            ..Default::default()
        })
        .await
        .unwrap();

    // Wait for retry logic to complete
    tokio::time::sleep(Duration::from_secs(4)).await;

    let list = handle.list_programs().await.unwrap();
    let prog = list.first().unwrap();
    println!("Crasher Final Status: {:?}", prog.status);

    assert_eq!(prog.status, ProcessStatus::Fatal);
    assert!(
        notify.has_event("process_fatal"),
        "Should have triggered process_fatal event"
    );
}

#[tokio::test]
async fn test_duplicate_program_name_rejected() {
    let (handle, _tmp, _notify) = setup_manager().await;

    let req = common::CreateProgramRequest {
        name: Some("worker-a".to_string()),
        command: "/bin/sleep".to_string(),
        args: vec!["10".to_string()],
        autostart: false,
        ..Default::default()
    };

    handle.create_program(req.clone()).await.unwrap();

    let err = handle.create_program(req).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("already exists"),
        "expected name conflict error, got: {msg}"
    );
}

#[tokio::test]
async fn test_create_rejects_empty_command() {
    let (handle, _tmp, _notify) = setup_manager().await;

    let err = handle
        .create_program(common::CreateProgramRequest {
            name: Some("empty-cmd".to_string()),
            command: "   ".to_string(),
            autostart: false,
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("command:"),
        "expected command validation error, got: {err}"
    );
    assert!(
        err.to_string().contains("empty-cmd"),
        "expected program name in error, got: {err}"
    );
}

#[tokio::test]
async fn test_stack_validation_names_service_index() {
    let (handle, _tmp, _notify) = setup_manager().await;

    let err = handle
        .apply_stack(common::StackApplyRequest {
            services: vec![common::CreateProgramRequest {
                name: Some("web".to_string()),
                command: "  ".to_string(),
                autostart: false,
                ..Default::default()
            }],
            prune: false,
        })
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("services[0] (name=web)"), "got: {msg}");
    assert!(msg.contains("command:"), "got: {msg}");
}

#[tokio::test]
async fn test_duplicate_names_in_stack_rejected() {
    let (handle, _tmp, _notify) = setup_manager().await;

    let stack = common::StackApplyRequest {
        services: vec![
            common::CreateProgramRequest {
                name: Some("dup".to_string()),
                command: "/bin/sleep".to_string(),
                args: vec!["1".to_string()],
                ..Default::default()
            },
            common::CreateProgramRequest {
                name: Some("dup".to_string()),
                command: "/bin/sleep".to_string(),
                args: vec!["2".to_string()],
                ..Default::default()
            },
        ],
        prune: false,
    };

    let err = handle.apply_stack(stack).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("Duplicate program name"),
        "expected stack duplicate error, got: {msg}"
    );
}

#[tokio::test]
async fn test_update_resource_limits_persisted_immediately() {
    use common::{ResourceLimits, UpdateProgramRequest};

    let (handle, tmp, _notify) = setup_manager().await;

    let create = common::CreateProgramRequest {
        name: Some("limited-worker".to_string()),
        command: "/bin/sleep".to_string(),
        args: vec!["3600".to_string()],
        ..Default::default()
    };
    let ids = handle.create_program(create).await.unwrap();
    let id = ids[0];

    handle
        .update_program(
            id,
            UpdateProgramRequest {
                resource_limits: Some(ResourceLimits {
                    cpu_quota: Some(25.0),
                    memory_limit: Some(64 * 1024 * 1024),
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let snapshot = tmp.path().join("snapshot.json");
    let content = tokio::fs::read_to_string(&snapshot).await.unwrap();
    assert!(
        content.contains("resource_limits"),
        "snapshot should contain resource_limits immediately after update"
    );
    assert!(
        content.contains("67108864"),
        "memory_limit bytes should be persisted"
    );
}
