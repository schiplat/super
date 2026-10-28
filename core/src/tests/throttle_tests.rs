use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use super::AuthThrottle;

fn ip(n: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(10, 0, 0, n))
}

#[test]
fn allows_below_threshold_then_locks_out() {
    let t = AuthThrottle::new(3, Duration::from_secs(60));
    for _ in 0..2 {
        assert!(t.allowed(ip(1)));
        t.record_failure(ip(1));
    }
    assert!(t.allowed(ip(1)));
    t.record_failure(ip(1));
    assert!(!t.allowed(ip(1)));
    // Other peers are unaffected.
    assert!(t.allowed(ip(2)));
}

#[test]
fn success_clears_failures() {
    let t = AuthThrottle::new(3, Duration::from_secs(60));
    t.record_failure(ip(1));
    t.record_failure(ip(1));
    t.record_success(ip(1));
    t.record_failure(ip(1));
    t.record_failure(ip(1));
    assert!(t.allowed(ip(1)));
}

#[test]
fn lockout_expires() {
    // 1-second window so the lockout lapses quickly.
    let t = AuthThrottle::new(1, Duration::from_secs(1));
    t.record_failure(ip(3));
    assert!(!t.allowed(ip(3)));
    std::thread::sleep(Duration::from_millis(1100));
    assert!(t.allowed(ip(3)));
}

#[test]
fn disabled_throttle_never_blocks() {
    let t = AuthThrottle::new(0, Duration::from_secs(60));
    for _ in 0..100 {
        t.record_failure(ip(4));
    }
    assert!(!t.enabled());
    assert!(t.allowed(ip(4)));
}

#[test]
fn failures_outside_window_do_not_accumulate() {
    let t = AuthThrottle::new(3, Duration::from_secs(1));
    t.record_failure(ip(5));
    t.record_failure(ip(5));
    std::thread::sleep(Duration::from_millis(1100));
    t.record_failure(ip(5));
    assert!(t.allowed(ip(5)));
}
