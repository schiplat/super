use super::resolve_hostname;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn prefers_super_hostname_override() {
    let _guard = ENV_LOCK.lock().unwrap();
    // SAFETY: single-threaded under ENV_LOCK for this test process.
    unsafe {
        std::env::set_var("SUPER_HOSTNAME", "  fleet-node-a  ");
    }
    assert_eq!(resolve_hostname(), "fleet-node-a");
    unsafe {
        std::env::remove_var("SUPER_HOSTNAME");
    }
}
