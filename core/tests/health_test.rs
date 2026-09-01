use common::HealthCheck;
use std::time::Duration;
use super_core::health;
use tokio::net::TcpListener;

#[tokio::test]
async fn test_tcp_health_check() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let check = HealthCheck::Tcp {
        host: "127.0.0.1".to_string(),
        port,
        interval_secs: 0,
        timeout_secs: 0,
        start_period_secs: 0,
        max_failures: 0,
    };

    assert!(
        health::perform_check(&check).await.healthy,
        "TCP check should pass when port is open"
    );

    drop(listener);
    tokio::time::sleep(Duration::from_millis(100)).await;

    let outcome = health::perform_check(&check).await;
    assert!(
        !outcome.healthy,
        "TCP check should fail when port is closed"
    );
    assert!(outcome.detail.is_some());
}

#[tokio::test]
async fn test_tcp_health_check_honors_timeout() {
    // A non-routable address makes connect() hang until the timeout fires.
    // 10.255.255.1 is a TEST-NET address; on hosts without a route a connect
    // to it blocks, and the 1s timeout bounds the probe (some sandboxes refuse
    // outbound connects immediately instead — both are "quick failure").
    let check = HealthCheck::Tcp {
        host: "10.255.255.1".to_string(),
        port: 65000,
        interval_secs: 0,
        timeout_secs: 1,
        start_period_secs: 0,
        max_failures: 0,
    };
    let start = std::time::Instant::now();
    let outcome = health::perform_check(&check).await;
    let elapsed = start.elapsed();
    assert!(!outcome.healthy, "unreachable TCP should fail");
    assert!(
        elapsed < Duration::from_secs(3),
        "TCP probe should respect timeout_secs=1 (took {elapsed:?})"
    );
}

#[tokio::test]
async fn test_exec_health_check() {
    let check_ok = HealthCheck::Exec {
        command: "exit 0".to_string(),
        interval_secs: 0,
        timeout_secs: 0,
        start_period_secs: 0,
        max_failures: 0,
    };
    assert!(
        health::perform_check(&check_ok).await.healthy,
        "Exit 0 should be healthy"
    );

    let check_fail = HealthCheck::Exec {
        command: "exit 1".to_string(),
        interval_secs: 0,
        timeout_secs: 0,
        start_period_secs: 0,
        max_failures: 0,
    };
    let outcome = health::perform_check(&check_fail).await;
    assert!(!outcome.healthy, "Exit 1 should be unhealthy");
    assert!(outcome.detail.is_some());
}

#[tokio::test]
async fn test_exec_health_check_reports_stderr() {
    let check = HealthCheck::Exec {
        command: "echo oops 1>&2; exit 1".to_string(),
        interval_secs: 0,
        timeout_secs: 0,
        start_period_secs: 0,
        max_failures: 0,
    };
    let outcome = health::perform_check(&check).await;
    assert!(!outcome.healthy);
    assert!(outcome.detail.unwrap().contains("oops"));
}

#[tokio::test]
async fn test_exec_health_check_honors_timeout() {
    let check = HealthCheck::Exec {
        command: "sleep 30".to_string(),
        interval_secs: 0,
        timeout_secs: 1,
        start_period_secs: 0,
        max_failures: 0,
    };
    let start = std::time::Instant::now();
    let outcome = health::perform_check(&check).await;
    let elapsed = start.elapsed();
    assert!(!outcome.healthy, "long-running exec should time out");
    assert!(
        elapsed < Duration::from_secs(5),
        "exec probe should respect timeout_secs=1 (took {elapsed:?})"
    );
    assert!(
        outcome
            .detail
            .as_deref()
            .unwrap_or("")
            .contains("timed out"),
        "expected a timeout detail, got {:?}",
        outcome.detail
    );
}

#[tokio::test]
async fn test_health_tuning_defaults() {
    let check = HealthCheck::Exec {
        command: "exit 0".to_string(),
        interval_secs: 0,
        timeout_secs: 0,
        start_period_secs: 0,
        max_failures: 0,
    };
    assert_eq!(check.interval_secs(), 5);
    assert_eq!(check.timeout_secs(), 7);
    assert_eq!(check.start_period_secs(), 1);
    assert_eq!(check.max_failures(), 0); // explicit 0 disables auto-restart

    let http = HealthCheck::Http {
        url: "http://127.0.0.1:1/".to_string(),
        method: None,
        interval_secs: 2,
        timeout_secs: 9,
        start_period_secs: 0,
        max_failures: 3,
    };
    assert_eq!(http.interval_secs(), 2);
    assert_eq!(http.timeout_secs(), 9);
    assert_eq!(http.start_period_secs(), 1);
    assert_eq!(http.max_failures(), 3);
    assert!(http.is_enabled());
    assert!(!HealthCheck::Disabled.is_enabled());
}
