use common::ProgramConfig;
use std::collections::{HashMap, HashSet};
use tokio::task::JoinHandle;
use uuid::Uuid;

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
///
/// `running` holds one entry per program, except scheduled tasks with
/// `max_concurrent > 1` which may hold several overlapping instances (each a
/// separate OS process). The first entry is the "primary" instance used for
/// status display and single-instance operations.
pub struct ProcessRegistry {
    pub programs: HashMap<Uuid, ProgramConfig>,
    pub running: HashMap<Uuid, Vec<RuntimeState>>,

    // State queues
    pub restarting: HashSet<Uuid>,
    pub waiting: HashSet<Uuid>,
    pub crashed: HashSet<Uuid>,

    // Startup error cache
    pub startup_errors: HashMap<Uuid, String>,

    /// Consecutive health-triggered restarts per program (reset as soon as the
    /// program reports healthy again). Guards the `max_failures` auto-restart
    /// with `retry_limit`.
    pub health_restart_count: HashMap<Uuid, u32>,

    // Dirty flag (persistence)
    pub dirty: bool,
}

impl ProcessRegistry {
    pub fn new(initial_programs: HashMap<Uuid, ProgramConfig>) -> Self {
        Self {
            programs: initial_programs,
            running: HashMap::new(),
            restarting: HashSet::new(),
            waiting: HashSet::new(),
            crashed: HashSet::new(),
            startup_errors: HashMap::new(),
            health_restart_count: HashMap::new(),
            dirty: false,
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

    /// Whether any instance of the program is currently running.
    pub fn is_running(&self, id: &Uuid) -> bool {
        self.running.get(id).is_some_and(|v| !v.is_empty())
    }

    /// Number of running instances of the program (≥ 1 only when a scheduled
    /// task runs with `max_concurrent > 1`).
    pub fn running_count(&self, id: &Uuid) -> usize {
        self.running.get(id).map_or(0, |v| v.len())
    }

    /// Total number of running instances across all programs.
    pub fn total_running(&self) -> usize {
        self.running.values().map(|v| v.len()).sum()
    }

    /// Whether no process is running at all.
    pub fn running_empty(&self) -> bool {
        self.running.values().all(|v| v.is_empty())
    }

    /// All running instances as `(program_id, pid)` pairs (shutdown/force-kill).
    pub fn all_running_pids(&self) -> Vec<(Uuid, u32)> {
        self.running
            .iter()
            .flat_map(|(id, states)| states.iter().map(|s| (*id, s.pid)))
            .collect()
    }

    /// All instances of a program (empty if none).
    pub fn get_running_all(&self, id: &Uuid) -> &[RuntimeState] {
        self.running.get(id).map_or(&[], |v| v.as_slice())
    }

    /// Primary instance of a program (the first one started), if any.
    pub fn get_running(&self, id: &Uuid) -> Option<&RuntimeState> {
        self.running.get(id).and_then(|v| v.first())
    }

    /// Mutable primary instance of a program, if any.
    pub fn get_running_mut(&mut self, id: &Uuid) -> Option<&mut RuntimeState> {
        self.running.get_mut(id).and_then(|v| v.first_mut())
    }

    /// Mutable instances of a program, if any.
    pub fn get_running_all_mut(&mut self, id: &Uuid) -> Option<&mut Vec<RuntimeState>> {
        self.running.get_mut(id)
    }

    /// Register a newly spawned instance.
    pub fn insert_running(&mut self, id: Uuid, state: RuntimeState) {
        self.running.entry(id).or_default().push(state);
    }

    /// Remove the instance with the given pid. If no entry matches (e.g. a race
    /// with a restart), falls back to removing the primary instance.
    pub fn remove_running_by_pid(&mut self, id: &Uuid, pid: u32) -> Option<RuntimeState> {
        let states = self.running.get_mut(id)?;
        let pos = states.iter().position(|s| s.pid == pid).unwrap_or(0);
        let removed = states.remove(pos);
        if states.is_empty() {
            self.running.remove(id);
        }
        Some(removed)
    }

    /// Remove the primary instance (single-instance exit path).
    pub fn remove_running(&mut self, id: &Uuid) -> Option<RuntimeState> {
        let states = self.running.get_mut(id)?;
        let removed = states.remove(0);
        if states.is_empty() {
            self.running.remove(id);
        }
        Some(removed)
    }
}
