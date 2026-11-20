//! Per-IP request limits for the unauthenticated SSO endpoints.
//!
//! Deliberately separate from `AuthThrottle`: that one counts failed bearer
//! auth and locks an IP out, while these endpoints are hit by ordinary browser
//! navigation. Counting them there would let a busy NAT lock itself out of the
//! API. A simple fixed window is enough — the goal is to stop trivial floods,
//! not to meter usage.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    Login,
    Callback,
    Exchange,
    CliStart,
    CliPoll,
}

impl Bucket {
    fn key(self) -> u8 {
        match self {
            Self::Login => 0,
            Self::Callback => 1,
            Self::Exchange => 2,
            Self::CliStart => 3,
            Self::CliPoll => 4,
        }
    }

    fn limit(self) -> u32 {
        match self {
            // Login starts a browser round trip; a handful per minute is
            // already more than a human needs.
            Self::Login => 20,
            // Multiple tabs and retries after an IdP hiccup.
            Self::Callback => 60,
            Self::Exchange => 20,
            // A CLI starts one request per login attempt.
            Self::CliStart => 10,
            // Polling every few seconds for up to ten minutes.
            Self::CliPoll => 120,
        }
    }
}

const WINDOW_SECS: u64 = 60;
const MAP_SOFT_LIMIT: usize = 4096;

/// `(window start, requests seen in it)`.
type Window = (Instant, u32);
type Limits = HashMap<(IpAddr, u8), Window>;

/// `true` when the request is allowed. The counter is consumed even when the
/// request later fails, which is what limits abusive retries.
pub fn allow(ip: IpAddr, bucket: Bucket) -> bool {
    static LIMITS: OnceLock<Mutex<Limits>> = OnceLock::new();
    let limits = LIMITS.get_or_init(|| Mutex::new(HashMap::new()));
    let now = Instant::now();
    let mut guard = limits.lock().unwrap_or_else(|p| p.into_inner());

    if guard.len() > MAP_SOFT_LIMIT {
        guard.retain(|_, (start, _)| now.duration_since(*start).as_secs() < WINDOW_SECS * 2);
    }

    let entry = guard.entry((ip, bucket.key())).or_insert((now, 0));
    if now.duration_since(entry.0).as_secs() >= WINDOW_SECS {
        *entry = (now, 0);
    }
    if entry.1 >= bucket.limit() {
        return false;
    }
    entry.1 += 1;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn limits_are_per_ip_and_bucket() {
        let a = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        let b = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2));
        for _ in 0..Bucket::Login.limit() {
            assert!(allow(a, Bucket::Login));
        }
        assert!(!allow(a, Bucket::Login), "limit must bite");
        assert!(allow(b, Bucket::Login), "other IPs are unaffected");
        assert!(allow(a, Bucket::Callback), "buckets are independent");
    }
}
