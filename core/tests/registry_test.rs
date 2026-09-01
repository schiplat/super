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

fn running_state(pid: u32) -> RuntimeState {
    RuntimeState {
        pid,
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
    registry.insert_running(
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
    assert!(registry.is_running(&id));
    assert_eq!(registry.running_count(&id), 1);

    // 4. Remove
    registry.remove_running_by_pid(&id, 1234);
    assert!(!registry.is_running(&id));
    registry.programs.remove(&id);
    assert!(registry.get_config(&id).is_none());
}

#[test]
fn test_registry_concurrent_instances() {
    let mut registry = ProcessRegistry::new(HashMap::new(), HashMap::new());
    let id = Uuid::new_v4();

    for (pid, healthy) in [(1001, true), (1002, false), (1003, true)] {
        registry.insert_running(
            id,
            RuntimeState {
                pid,
                start_time: 100,
                retry_count: 0,
                stopping: false,
                restart_requested: false,
                is_healthy: healthy,
                health_error: None,
                health_task: None,
                alert_pending_recovery: false,
                cpu_usage: 0.0,
                mem_usage: 0,
            },
        );
    }

    assert!(registry.is_running(&id));
    assert_eq!(registry.running_count(&id), 3);
    assert_eq!(registry.total_running(), 3);
    assert!(!registry.running_empty());
    // Primary is the first instance.
    assert_eq!(registry.get_running(&id).unwrap().pid, 1001);

    // Remove a non-primary instance by pid.
    let removed = registry.remove_running_by_pid(&id, 1002).unwrap();
    assert_eq!(removed.pid, 1002);
    assert_eq!(registry.running_count(&id), 2);
    // Primary unchanged.
    assert_eq!(registry.get_running(&id).unwrap().pid, 1001);

    // Remove the rest; the key is dropped when the last instance goes.
    registry.remove_running_by_pid(&id, 1001).unwrap();
    registry.remove_running_by_pid(&id, 1003).unwrap();
    assert!(!registry.is_running(&id));
    assert_eq!(registry.running_count(&id), 0);
    assert!(registry.running_empty());
    assert!(registry.all_running_pids().is_empty());
}

#[test]
fn remove_running_by_pid_unknown_pid_falls_back_to_primary() {
    // Documented fallback: when the reported pid no longer matches any instance
    // (a race with a restart), the primary instance is removed instead.
    let mut registry = ProcessRegistry::new(HashMap::new(), HashMap::new());
    let id = Uuid::new_v4();
    for pid in [2001u32, 2002, 2003] {
        registry.insert_running(id, running_state(pid));
    }

    let removed = registry.remove_running_by_pid(&id, 9999).unwrap();
    assert_eq!(
        removed.pid, 2001,
        "unknown pid must fall back to the primary"
    );
    assert_eq!(registry.running_count(&id), 2);
    assert_eq!(registry.get_running(&id).unwrap().pid, 2002);

    // Removing the remaining instances drops the program's key.
    registry.remove_running_by_pid(&id, 2002).unwrap();
    registry.remove_running_by_pid(&id, 2003).unwrap();
    assert!(!registry.is_running(&id));
    assert!(registry.running_empty());
}

#[test]
fn remove_running_primary_keeps_siblings_then_drops_key() {
    // `remove_running` (single-instance exit path) only removes the primary;
    // siblings keep the program registered until the last one exits.
    let mut registry = ProcessRegistry::new(HashMap::new(), HashMap::new());
    let id = Uuid::new_v4();
    for pid in [3001u32, 3002, 3003] {
        registry.insert_running(id, running_state(pid));
    }

    let removed = registry.remove_running(&id).unwrap();
    assert_eq!(removed.pid, 3001);
    assert!(
        registry.is_running(&id),
        "siblings must keep the program running"
    );
    assert_eq!(registry.running_count(&id), 2);

    registry.remove_running(&id).unwrap();
    registry.remove_running(&id).unwrap();
    assert!(!registry.is_running(&id));
    assert_eq!(registry.running_count(&id), 0);
    assert!(registry.running_empty());
}

#[test]
fn all_running_pids_spans_programs_and_siblings() {
    let mut registry = ProcessRegistry::new(HashMap::new(), HashMap::new());
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    registry.insert_running(a, running_state(4001));
    registry.insert_running(a, running_state(4002));
    registry.insert_running(b, running_state(4003));

    let mut pids = registry.all_running_pids();
    pids.sort();
    let mut expected = vec![(a, 4001), (a, 4002), (b, 4003)];
    expected.sort();
    assert_eq!(pids, expected);
    assert_eq!(registry.total_running(), 3);
    assert!(!registry.running_empty());

    registry.remove_running_by_pid(&a, 4001).unwrap();
    registry.remove_running_by_pid(&a, 4002).unwrap();
    registry.remove_running_by_pid(&b, 4003).unwrap();
    assert_eq!(registry.all_running_pids(), Vec::new());
    assert!(registry.running_empty());
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
