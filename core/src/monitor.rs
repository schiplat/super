use crate::manager::Command;
use common::{DiskPartitionStats, SystemStats};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use sysinfo::{
    CpuRefreshKind, Disks, MemoryRefreshKind, Pid as SysPid, ProcessRefreshKind, ProcessesToUpdate,
    RefreshKind, System,
};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Resource monitor: collects CPU/mem in a background thread and sends to Manager.
pub struct ResourceMonitor {
    pid_mapping: Arc<RwLock<HashMap<Uuid, i32>>>,
    system_stats: Arc<RwLock<SystemStats>>,
}

impl ResourceMonitor {
    pub fn new(tx_manager: mpsc::Sender<Command>) -> Self {
        let pid_mapping = Arc::new(RwLock::new(HashMap::new()));
        let system_stats = Arc::new(RwLock::new(SystemStats::default()));

        let mapping_clone = pid_mapping.clone();
        let stats_clone = system_stats.clone();

        thread::Builder::new()
            .name("super-monitor".to_string())
            .spawn(move || {
                Self::run_loop(mapping_clone, stats_clone, tx_manager);
            })
            .expect("Failed to spawn monitor thread");

        Self {
            pid_mapping,
            system_stats,
        }
    }

    pub fn watch(&self, id: Uuid, pid: u32) {
        if let Ok(mut map) = self.pid_mapping.write() {
            map.insert(id, pid as i32);
        }
    }

    pub fn unwatch(&self, id: &Uuid) {
        if let Ok(mut map) = self.pid_mapping.write() {
            map.remove(id);
        }
    }

    pub fn system_stats(&self) -> SystemStats {
        self.system_stats
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn collect_disks(disks: &mut Disks) -> Vec<DiskPartitionStats> {
        disks.refresh(true);
        let mut out: Vec<DiskPartitionStats> = disks
            .list()
            .iter()
            .filter_map(|d| {
                let total = d.total_space();
                // Skip tiny / pseudo volumes (macOS system slices, empty mounts).
                if total < 1_073_741_824 {
                    return None;
                }
                let fs = d.file_system().to_string_lossy().to_ascii_lowercase();
                if matches!(
                    fs.as_str(),
                    "autofs"
                        | "devfs"
                        | "devtmpfs"
                        | "proc"
                        | "sysfs"
                        | "cgroup"
                        | "cgroup2"
                        | "squashfs"
                        | "overlay"
                ) {
                    return None;
                }
                let mount = d.mount_point().to_string_lossy().to_string();
                if mount.is_empty() {
                    return None;
                }
                Some(DiskPartitionStats {
                    mount_point: mount,
                    available_bytes: d.available_space(),
                    total_bytes: total,
                    name: {
                        let n = d.name().to_string_lossy();
                        if n.is_empty() {
                            None
                        } else {
                            Some(n.into_owned())
                        }
                    },
                })
            })
            .collect();
        out.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
        out.truncate(12);
        out
    }

    fn run_loop(
        mapping: Arc<RwLock<HashMap<Uuid, i32>>>,
        system_stats: Arc<RwLock<SystemStats>>,
        tx: mpsc::Sender<Command>,
    ) {
        let mut sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything())
                .with_processes(ProcessRefreshKind::nothing().with_cpu().with_memory()),
        );
        let mut disks = Disks::new();

        loop {
            thread::sleep(Duration::from_secs(3));

            sys.refresh_cpu_all();
            sys.refresh_memory();
            let disk_stats = Self::collect_disks(&mut disks);

            if let Ok(mut stats) = system_stats.write() {
                *stats = SystemStats {
                    cpu_percent: sys.global_cpu_usage(),
                    memory_used_bytes: sys.used_memory(),
                    memory_total_bytes: sys.total_memory(),
                    timestamp: chrono::Utc::now().timestamp() as u64,
                    disks: disk_stats,
                };
            }

            let targets: Vec<(Uuid, i32)> = {
                if let Ok(map) = mapping.read() {
                    map.iter().map(|(id, pid)| (*id, *pid)).collect()
                } else {
                    vec![]
                }
            };

            if targets.is_empty() {
                continue;
            }

            let mut updates = HashMap::new();

            let sys_pids: Vec<SysPid> = targets
                .iter()
                .map(|(_, raw)| SysPid::from(*raw as usize))
                .collect();

            sys.refresh_processes(ProcessesToUpdate::Some(&sys_pids), true);

            for (id, raw_pid) in targets {
                let pid = SysPid::from(raw_pid as usize);
                if let Some(proc) = sys.process(pid) {
                    updates.insert(id, (proc.cpu_usage(), proc.memory()));
                }
            }

            if !updates.is_empty() {
                match tx.try_send(Command::InternalMetricsUpdate { metrics: updates }) {
                    Ok(_) => {}
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => break,
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {}
                }
            }
        }
    }
}
