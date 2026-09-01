use chrono::{DateTime, Duration as ChronoDuration, Utc};
use cron::Schedule;
use rand::Rng;
use std::collections::HashMap;
use std::str::FromStr;
use uuid::Uuid;

/// One due cron slot produced by [`CronScheduler::tick`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CronTrigger {
    pub id: Uuid,
    /// Number of schedule slots that became due since the previous trigger.
    /// `1` means the run is on time; `> 1` means slots were missed while the
    /// daemon was down or the loop lagged (used by the catchup policy).
    pub missed_slots: u32,
}

struct CronTask {
    /// Next raw schedule slot. Bookkeeping anchor for missed-slot counting.
    next_slot: DateTime<Utc>,
    /// Actual trigger deadline = `next_slot` + random jitter.
    next_trigger: DateTime<Utc>,
    expr: String,
    jitter_sec: u64,
}

pub struct CronScheduler {
    tasks: HashMap<Uuid, CronTask>,
}

impl Default for CronScheduler {
    fn default() -> Self {
        Self::new()
    }
}

fn jitter_offset(jitter_sec: u64) -> ChronoDuration {
    if jitter_sec == 0 {
        return ChronoDuration::zero();
    }
    let j = rand::thread_rng().gen_range(0..=jitter_sec);
    ChronoDuration::seconds(j as i64)
}

impl CronScheduler {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    /// Register or update a cron task. `jitter_sec` adds a uniform random
    /// delay in `[0, jitter_sec]` to each trigger deadline.
    ///
    /// `last_run` anchors the missed-slot count: when recovering after a daemon
    /// restart, slots between `last_run` and now are counted as missed, which
    /// lets the manager's catchup policy backfill them.
    pub fn upsert(
        &mut self,
        id: Uuid,
        cron_expr: &str,
        jitter_sec: u64,
        last_run: Option<DateTime<Utc>>,
    ) {
        if let Ok(schedule) = Schedule::from_str(cron_expr) {
            let anchor = last_run.unwrap_or_else(Utc::now);
            if let Some(next) = schedule.after(&anchor).next() {
                let next_trigger = next + jitter_offset(jitter_sec);
                self.tasks.insert(
                    id,
                    CronTask {
                        next_slot: next,
                        next_trigger,
                        expr: cron_expr.to_string(),
                        jitter_sec,
                    },
                );
                tracing::debug!(
                    "Scheduler: registered {} for {} (jitter {}s)",
                    id,
                    next_trigger,
                    jitter_sec
                );
            }
        } else {
            tracing::warn!("Scheduler: invalid cron expression '{}'", cron_expr);
        }
    }

    /// Update the jitter for an existing task (e.g. after `super update`).
    pub fn set_jitter(&mut self, id: &Uuid, jitter_sec: u64) {
        if let Some(task) = self.tasks.get_mut(id) {
            task.jitter_sec = jitter_sec;
            task.next_trigger = task.next_slot + jitter_offset(jitter_sec);
        }
    }

    /// Remove a task
    pub fn remove(&mut self, id: &Uuid) {
        self.tasks.remove(id);
    }

    /// Return due tasks and reschedule them. Each trigger carries the number
    /// of raw slots that became due since the previous tick (`missed_slots`).
    pub fn tick(&mut self) -> Vec<CronTrigger> {
        let now = Utc::now();
        let mut triggered = Vec::new();
        let mut to_update = Vec::new();

        for (id, task) in &self.tasks {
            if now < task.next_trigger {
                continue;
            }

            // Count missed slots. Jitter absorbs the slots between `next_slot`
            // and `next_trigger` (the trigger is intentionally delayed), so they
            // are NOT missed. Only slots that became due AFTER the trigger
            // deadline and before `now` are missed — i.e. a daemon outage or a
            // lagged tick loop that skipped several schedule slots.
            let mut missed: u32 = 1;
            if let Ok(schedule) = Schedule::from_str(&task.expr) {
                // Advance to the first slot after the trigger deadline,
                // skipping any slots absorbed by the jitter window.
                let mut cursor = task.next_slot;
                loop {
                    match schedule.after(&cursor).next() {
                        Some(c) if c <= task.next_trigger => cursor = c,
                        Some(c) => {
                            cursor = c;
                            break;
                        }
                        None => break, // schedule exhausted
                    }
                }
                // Count slots that expired after the trigger deadline.
                let mut probe = cursor;
                while probe <= now {
                    missed += 1;
                    match schedule.after(&probe).next() {
                        Some(n) if n <= now => probe = n,
                        _ => break,
                    }
                }
            }
            triggered.push(CronTrigger {
                id: *id,
                missed_slots: missed,
            });
            to_update.push((*id, task.expr.clone(), task.jitter_sec));
        }

        for (id, expr, jitter_sec) in to_update {
            let next = Schedule::from_str(&expr)
                .ok()
                .and_then(|s| s.upcoming(Utc).next());
            match next {
                Some(next_slot) => {
                    if let Some(task) = self.tasks.get_mut(&id) {
                        task.next_slot = next_slot;
                        task.next_trigger = next_slot + jitter_offset(jitter_sec);
                        tracing::debug!("Scheduler: rescheduled {} for {}", id, task.next_trigger);
                    }
                }
                // Schedule exhausted (e.g. a one-shot cron in the past): drop it.
                None => {
                    tracing::debug!("Scheduler: schedule exhausted for {}", id);
                    self.tasks.remove(&id);
                }
            }
        }

        triggered
    }

    /// Next run time for API display (includes jitter).
    pub fn get_next_run(&self, id: &Uuid) -> Option<DateTime<Utc>> {
        self.tasks.get(id).map(|t| t.next_trigger)
    }
}
