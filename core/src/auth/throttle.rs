//! Per-peer-IP failed-auth throttle.
//!
//! Brute-forcing the Bearer secret (OSS mode) or Access Tokens (licensed mode)
//! used to be unthrottled. This guard counts 401s per peer IP; once
//! `[server].auth_max_failures` is exceeded inside the window, further
//! requests from that IP get 429 until the window elapses. Successful auth
//! clears the counter.
//!
//! Keyed by the socket peer address, not `X-Forwarded-For`: behind a reverse
//! proxy every request shares the proxy IP, so a failure storm locks out the
//! proxy path temporarily — that is the accepted trade-off versus trusting a
//! client-supplied header attackers can rotate. Unix-socket connections have
//! no peer address and are never throttled.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const MAX_TRACKED_IPS: usize = 8192;

struct Entry {
    failures: u32,
    window_started: Instant,
    locked_until: Option<Instant>,
}

/// Shared failure window/lockout state. Cheap to hold in a `OnceLock`.
pub struct AuthThrottle {
    max_failures: AtomicU32,
    window_secs: AtomicU64,
    entries: Mutex<HashMap<IpAddr, Entry>>,
}

impl AuthThrottle {
    pub fn new(max_failures: u32, window: Duration) -> Self {
        Self {
            max_failures: AtomicU32::new(max_failures),
            window_secs: AtomicU64::new(window.as_secs().max(1)),
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Process-wide throttle. Initialized by `configure_shared` at daemon
    /// startup; before that (and in tests) it runs with built-in defaults.
    pub fn shared() -> &'static AuthThrottle {
        static SHARED: std::sync::OnceLock<AuthThrottle> = std::sync::OnceLock::new();
        SHARED.get_or_init(|| {
            AuthThrottle::new(
                common::config::DEFAULT_AUTH_MAX_FAILURES,
                Duration::from_secs(common::config::DEFAULT_AUTH_FAILURE_WINDOW),
            )
        })
    }

    /// Configure the process-wide throttle (call once at startup, before
    /// serving). `max_failures = 0` disables throttling.
    pub fn configure_shared(max_failures: u32, window: Duration) {
        let shared = Self::shared();
        shared.max_failures.store(max_failures, Ordering::Relaxed);
        shared
            .window_secs
            .store(window.as_secs().max(1), Ordering::Relaxed);
        shared
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    pub fn enabled(&self) -> bool {
        self.max_failures.load(Ordering::Relaxed) > 0
    }

    fn max_failures(&self) -> u32 {
        self.max_failures.load(Ordering::Relaxed)
    }

    fn window(&self) -> Duration {
        Duration::from_secs(self.window_secs.load(Ordering::Relaxed))
    }

    /// Whether `ip` may attempt authentication right now.
    pub fn allowed(&self, ip: IpAddr) -> bool {
        if !self.enabled() {
            return true;
        }
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        prune(&mut entries, self.window());
        match entries.get(&ip) {
            Some(Entry {
                locked_until: Some(until),
                ..
            }) => *until <= Instant::now(),
            _ => true,
        }
    }

    pub fn record_failure(&self, ip: IpAddr) {
        if !self.enabled() {
            return;
        }
        let window = self.window();
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        prune(&mut entries, window);
        let now = Instant::now();
        let entry = entries.entry(ip).or_insert(Entry {
            failures: 0,
            window_started: now,
            locked_until: None,
        });
        if now.duration_since(entry.window_started) >= window {
            entry.window_started = now;
            entry.failures = 0;
        }
        entry.failures += 1;
        if entry.failures >= self.max_failures() {
            entry.locked_until = Some(now + window);
            entry.failures = 0;
            entry.window_started = now;
        }
    }

    pub fn record_success(&self, ip: IpAddr) {
        if !self.enabled() {
            return;
        }
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&ip);
    }
}

/// Drop entries that are neither locked nor inside an active failure window.
/// Only runs once the map grows past `MAX_TRACKED_IPS` so the steady-state
/// cost is one length check.
fn prune(entries: &mut HashMap<IpAddr, Entry>, window: Duration) {
    if entries.len() < MAX_TRACKED_IPS {
        return;
    }
    let now = Instant::now();
    entries.retain(|_, e| {
        e.locked_until.is_some_and(|until| until > now)
            || now.duration_since(e.window_started) < window
    });
}

#[cfg(test)]
#[path = "../tests/throttle_tests.rs"]
mod tests;
