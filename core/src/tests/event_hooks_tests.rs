use super::*;

#[test]
fn matches_event_and_program_filters() {
    let hook = EventHookConfig {
        command: "true".into(),
        url: None,
        headers: None,
        events: vec!["process_fatal".into()],
        programs: vec!["web".into()],
        r#async: true,
        timeout_secs: 5,
        id: None,
    };
    let id = uuid::Uuid::new_v4();
    let event = SystemEvent::ProcessFatal {
        program_id: id,
        program_name: "web".into(),
        pid: None,
        uptime_secs: 0,
        exit_code: None,
        signal: None,
        msg: "x".into(),
        log_tail: None,
    };
    assert!(matches_hook(&hook, &event));
    let other = SystemEvent::ProcessFatal {
        program_id: id,
        program_name: "worker".into(),
        pid: None,
        uptime_secs: 0,
        exit_code: None,
        signal: None,
        msg: "x".into(),
        log_tail: None,
    };
    assert!(!matches_hook(&hook, &other));
}

#[test]
fn health_restart_event_has_type_and_filter() {
    let id = uuid::Uuid::new_v4();
    let event = SystemEvent::HealthRestart {
        program_id: id,
        program_name: "api".into(),
        pid: Some(42),
        uptime_secs: 9,
        retry_count: 2,
        msg: "probe failed".into(),
    };
    assert_eq!(event.event_type(), "health_restart");
    assert_eq!(event.program_name(), Some("api"));

    let hook = EventHookConfig {
        command: "true".into(),
        url: None,
        headers: None,
        events: vec!["health_restart".into()],
        programs: vec!["api".into()],
        r#async: true,
        timeout_secs: 5,
        id: None,
    };
    assert!(matches_hook(&hook, &event));
    assert!(!matches_hook(&hook, &SystemEvent::SystemShutdown));
}

#[test]
fn health_restart_payload_serializes_retry_and_msg() {
    let id = uuid::Uuid::new_v4();
    let event = SystemEvent::HealthRestart {
        program_id: id,
        program_name: "api".into(),
        pid: Some(42),
        uptime_secs: 9,
        retry_count: 2,
        msg: "probe failed".into(),
    };
    let payload = build_payload(&event).unwrap();
    let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(v["event"], "health_restart");
    assert_eq!(v["program"]["name"], "api");
    assert_eq!(v["payload"]["retry_count"], 2);
    assert_eq!(v["payload"]["msg"], "probe failed");

    // Event hooks receive SUPER_RETRY_COUNT in the environment.
    let env = build_env(&event);
    assert_eq!(env.get("SUPER_RETRY_COUNT").map(String::as_str), Some("2"));
    assert_eq!(env.get("SUPER_UPTIME_SECS").map(String::as_str), Some("9"));
    assert_eq!(env.get("SUPER_PID").map(String::as_str), Some("42"));
}
