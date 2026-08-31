use common::{ProgramConfig, ProgramEventRecord};
use std::collections::{HashMap, HashSet};
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Maximum number of persisted lifecycle events kept per program (oldest dropped).
pub const MAX_EVENTS_PER_PROGRAM: usize = 50;

/// Runtime state (formerly private to Manager; now public).
pub struct RuntimeState {
    pub pid: u32,
    pub start_time: u64,
    pub retry_count: u32,
    pub stopping: bool,
    pub restart_requested: bool,

    // Current health status
    pub is_healthy: bool,
    /// Latest health_check failure (cleared when healthy again).
    pub health_error: Option<String>,
    // Background health check task handle
    pub health_task: Option<JoinHandle<()>>,

    // Pending recovery notification flag
    pub alert_pending_recovery: bool,

    // Cached resource metrics
    pub cpu_usage: f32,
    pub mem_usage: u64,
}

/// Process registry: static config and dynamic runtime state.
pub struct ProcessRegistry {
    pub programs: HashMap<Uuid, ProgramConfig>,
    pub running: HashMap<Uuid, RuntimeState>,

    // State queues
    pub restarting: HashSet<Uuid>,
    pub waiting: HashSet<Uuid>,
    pub crashed: HashSet<Uuid>,

    // Startup error cache
    pub startup_errors: HashMap<Uuid, String>,

    /// Persisted lifecycle/exception event history (keyed by program id).
    pub events: HashMap<Uuid, Vec<ProgramEventRecord>>,

    // Dirty flag (persistence)
    pub dirty: bool,
    /// Whether the event history changed since last flush.
    pub events_dirty: bool,
}

impl ProcessRegistry {
    pub fn new(
        initial_programs: HashMap<Uuid, ProgramConfig>,
        initial_events: HashMap<Uuid, Vec<ProgramEventRecord>>,
    ) -> Self {
        Self {
            programs: initial_programs,
            running: HashMap::new(),
            restarting: HashSet::new(),
            waiting: HashSet::new(),
            crashed: HashSet::new(),
            startup_errors: HashMap::new(),
            events: initial_events,
            dirty: false,
            events_dirty: false,
        }
    }

    /// Append a lifecycle event to a program's history, capping the retained size.
    pub fn push_event(&mut self, id: Uuid, record: ProgramEventRecord) {
        let bucket = self.events.entry(id).or_default();
        bucket.push(record);
        if bucket.len() > MAX_EVENTS_PER_PROGRAM {
            let excess = bucket.len() - MAX_EVENTS_PER_PROGRAM;
            bucket.drain(..excess);
        }
        self.dirty = true;
        self.events_dirty = true;
    }

    /// Immutable event history for a program (empty if none).
    pub fn get_events(&self, id: &Uuid) -> Vec<ProgramEventRecord> {
        self.events.get(id).cloned().unwrap_or_default()
    }

    /// Drop event history for a program (used on remove).
    pub fn remove_events(&mut self, id: &Uuid) {
        if self.events.remove(id).is_some() {
            self.dirty = true;
            self.events_dirty = true;
        }
    }

    /// Mark state changed (needs flush to disk)
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Get program config
    pub fn get_config(&self, id: &Uuid) -> Option<&ProgramConfig> {
        self.programs.get(id)
    }

    /// Get mutable program config
    pub fn get_config_mut(&mut self, id: &Uuid) -> Option<&mut ProgramConfig> {
        self.programs.get_mut(id)
    }

    /// Get runtime state
    pub fn get_running(&self, id: &Uuid) -> Option<&RuntimeState> {
        self.running.get(id)
    }

    /// Get mutable runtime state
    pub fn get_running_mut(&mut self, id: &Uuid) -> Option<&mut RuntimeState> {
        self.running.get_mut(id)
    }
}
