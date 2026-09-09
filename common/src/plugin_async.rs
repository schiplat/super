//! Async helpers for cdylib plugins (separate Tokio from the host runtime).

use std::collections::HashMap;
use std::future::Future;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

type Job = Box<dyn FnOnce(&tokio::runtime::Runtime) + Send>;

struct WorkerPool {
    tx: std::sync::mpsc::Sender<Job>,
    _handle: thread::JoinHandle<()>,
}

impl WorkerPool {
    fn new(thread_name: &str) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        let name = thread_name.to_string();
        let handle = thread::Builder::new()
            .name(name)
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()
                    .expect("plugin tokio runtime");
                while let Ok(job) = rx.recv() {
                    // A panicking future must not poison-kill the worker thread:
                    // that would hang every subsequent `block_on` caller forever
                    // (worse than a crash — the daemon stays up, dead API).
                    // Swallow the panic and keep the loop alive.
                    let _ = panic::catch_unwind(AssertUnwindSafe(|| job(&rt)));
                }
            })
            .expect("spawn plugin worker thread");
        Self {
            tx,
            _handle: handle,
        }
    }

    fn submit(&self, job: Job) -> bool {
        self.tx.send(job).is_ok()
    }
}

static POOLS: OnceLock<Mutex<HashMap<String, Arc<WorkerPool>>>> = OnceLock::new();

fn pool(name: &str) -> Arc<WorkerPool> {
    let pools = POOLS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = pools.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .entry(name.to_string())
        .or_insert_with(|| Arc::new(WorkerPool::new(name)))
        .clone()
}

/// Run an async future to completion on a shared plugin worker runtime.
///
/// Use from synchronous C ABI entry points (HTTP handlers, init). Blocks the caller
/// until the future completes. Reuses one worker thread per `thread_name` instead of
/// spawning a new OS thread per call.
pub fn block_on<F, R>(thread_name: &str, future: F) -> Option<R>
where
    F: Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    if !pool(thread_name).submit(Box::new(move |rt| {
        let _ = tx.send(rt.block_on(future));
    })) {
        return None;
    }
    rx.recv().ok()
}

/// Fire-and-forget async work on the shared plugin worker runtime (e.g. `on_event`).
///
/// Returns `false` if the job could not be queued.
pub fn spawn_detached<F>(thread_name: &str, future: F) -> bool
where
    F: Future<Output = ()> + Send + 'static,
{
    pool(thread_name).submit(Box::new(move |rt| {
        rt.spawn(future);
    }))
}
