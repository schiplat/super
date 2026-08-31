use common::ProgramConfig;
use std::collections::HashMap;
use super_core::manager::registry::{ProcessRegistry, RuntimeState};
use uuid::Uuid;

// + Helpers +
fn mock_config(name: &str) -> ProgramConfig {
    ProgramConfig {
        name: name.to_string(),
        command: "echo".to_string(),
        args: vec![],
        env: HashMap::new(),
        cwd: None,
        user: None,
        autostart: true,
        retry_limit: 1,
        group: None,
        depends_on: vec![],
        health_check: None,
        hooks: Default::default(),
        artifact: None,
        cron: None,
        created_at: 0,
        updated_at: 0,
        restore_path: None,

        ..Default::default()
    }
}

#[test]
fn test_registry_crud() {
    let mut registry = ProcessRegistry::new(HashMap::new(), HashMap::new());
    let id = Uuid::new_v4();
    let config = mock_config("test-app");

    // 1. Add config
    registry.programs.insert(id, config.clone());
    assert!(registry.get_config(&id).is_some());

    // 2. Dirty flag
    assert!(!registry.dirty);
    registry.mark_dirty();
    assert!(registry.dirty);

    // 3. Runtime state
    // Note: RuntimeState has no is_upgrading field;
    // the design tracks transactions via restore_path in ProgramConfig.
    registry.running.insert(
        id,
        RuntimeState {
            pid: 1234,
            start_time: 100,
            retry_count: 0,
            stopping: false,
            restart_requested: false,
            is_healthy: true,
            health_error: None,
            health_task: None,
            alert_pending_recovery: false,
            cpu_usage: 0.0,
            mem_usage: 0,
        },
    );

    let state = registry.get_running(&id).unwrap();
    assert_eq!(state.pid, 1234);
    assert!(state.is_healthy);

    // 4. Remove
    registry.running.remove(&id);
    registry.programs.remove(&id);
    assert!(registry.get_config(&id).is_none());
}

#[test]
fn test_events_push_caps_at_max() {
    use common::ProgramEventRecord;
    use super_core::manager::registry::MAX_EVENTS_PER_PROGRAM;

    let mut registry = ProcessRegistry::new(HashMap::new(), HashMap::new());
    let id = Uuid::new_v4();

    for i in 0..(MAX_EVENTS_PER_PROGRAM + 25) {
        registry.push_event(
            id,
            ProgramEventRecord {
                ts: i as u64,
                event: "process_backoff".into(),
                exit_code: Some(1),
                signal: None,
                retry_count: Some(1),
                msg: format!("crash {i}"),
            },
        );
    }

    let events = registry.get_events(&id);
    assert_eq!(events.len(), MAX_EVENTS_PER_PROGRAM);
    // Oldest entries were dropped, newest retained
    assert_eq!(events[0].ts, 25);
    assert_eq!(
        events.last().unwrap().ts,
        MAX_EVENTS_PER_PROGRAM as u64 + 24
    );
    assert!(registry.events_dirty);
}

#[test]
fn test_events_remove_drops_bucket() {
    use common::ProgramEventRecord;

    let mut registry = ProcessRegistry::new(HashMap::new(), HashMap::new());
    let id = Uuid::new_v4();
    registry.push_event(
        id,
        ProgramEventRecord {
            ts: 1,
            event: "process_fatal".into(),
            exit_code: None,
            signal: Some(9),
            retry_count: None,
            msg: "oom".into(),
        },
    );

    assert_eq!(registry.get_events(&id).len(), 1);
    registry.remove_events(&id);
    assert!(registry.get_events(&id).is_empty());
}

#[test]
fn test_events_restore_from_initial_state() {
    use common::ProgramEventRecord;

    let id = Uuid::new_v4();
    let mut initial = HashMap::new();
    initial.insert(
        id,
        vec![ProgramEventRecord {
            ts: 1700000000,
            event: "process_exit".into(),
            exit_code: None,
            signal: Some(9),
            retry_count: None,
            msg: "Process killed by signal 9".into(),
        }],
    );

    let registry = ProcessRegistry::new(HashMap::new(), initial);
    let events = registry.get_events(&id);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].signal, Some(9));
    assert!(!registry.events_dirty);
}
