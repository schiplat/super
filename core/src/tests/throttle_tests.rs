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

/// IPv6 addresses within one /64 share a throttle entry: an attacker rotating
/// addresses inside their subnet must not reset the failure counter per hop.
/// Addresses from a different /64 throttle independently.
#[test]
fn ipv6_addresses_aggregate_per_subnet() {
    use std::net::Ipv6Addr;
    let t = AuthThrottle::new(3, Duration::from_secs(60));

    let base = Ipv6Addr::new(0x2001, 0xdb8, 0x1, 0x2, 0, 0, 0, 0);
    let mates = [
        base,
        Ipv6Addr::new(0x2001, 0xdb8, 0x1, 0x2, 0xaabb, 0xccdd, 0xeeff, 0x1),
        Ipv6Addr::new(0x2001, 0xdb8, 0x1, 0x2, 0xdead, 0xbeef, 0, 0xffff),
    ];
    // Three failures spread across three addresses in the same /64.
    for mate in mates {
        t.record_failure(IpAddr::V6(mate));
    }
    // The /64 as a whole is locked out: a fourth address from it is denied.
    assert!(
        !t.allowed(IpAddr::V6(Ipv6Addr::new(
            0x2001, 0xdb8, 0x1, 0x2, 0x1111, 0x2222, 0x3333, 0x4444
        ))),
        "rotation within one /64 must not evade the lockout"
    );
    // A different /64 is unaffected.
    assert!(t.allowed(IpAddr::V6(Ipv6Addr::new(
        0x2001, 0xdb8, 0x99, 0x99, 0, 0, 0, 1
    ))));

    // A successful auth from the locked /64 clears the shared counter.
    t.record_success(IpAddr::V6(mates[1]));
    assert!(t.allowed(IpAddr::V6(base)));
}
