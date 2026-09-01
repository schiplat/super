use nix::sys::signal::Signal;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use common::{
    BatchProgramRequest, BatchProgramResponse, CreateProgramRequest, HealthResponse, ProcessStatus,
    ProgramConfig, ProgramInfo, ProgramSummary, ReloadResponse, StackApplyRequest, SystemStats,
    UpdateProgramRequest,
};

use crate::manager::Command;

#[derive(Clone)]
pub struct ManagerHandle {
    tx: mpsc::Sender<Command>,
}

impl ManagerHandle {
    pub fn new(tx: mpsc::Sender<Command>) -> Self {
        Self { tx }
    }

    pub async fn reload(&self, wait: bool, timeout_sec: u64) -> anyhow::Result<ReloadResponse> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::Reload { reply: tx }).await?;
        let affected: Vec<ProgramSummary> = rx.await??;

        if !wait || affected.is_empty() {
            return Ok(ReloadResponse {
                affected,
                ready: true,
                waited_secs: 0,
            });
        }

        // Readiness wait: runs in the caller task (HTTP handler), never inside the
        // manager actor loop, so other commands keep flowing while we wait.
        let deadline = Instant::now() + Duration::from_secs(timeout_sec.max(1));
        let started = Instant::now();

        loop {
            let mut all_ready = true;
            for p in &affected {
                match self.get_program(p.id).await {
                    Ok(info) => match info.state {
                        ProcessStatus::Healthy | ProcessStatus::Stopped => {}
                        ProcessStatus::Fatal | ProcessStatus::Backoff => {
                            return Err(anyhow::anyhow!(
                                "Reload failed: program '{}' is {:?} (health_error: {})",
                                info.config.name,
                                info.state,
                                info.health_error.as_deref().unwrap_or("none")
                            ));
                        }
                        _ => all_ready = false,
                    },
                    Err(e) => {
                        return Err(anyhow::anyhow!("Reload readiness check failed: {e}"));
                    }
                }
            }

            if all_ready {
                return Ok(ReloadResponse {
                    waited_secs: started.elapsed().as_secs(),
                    ready: true,
                    affected,
                });
            }
            if Instant::now() >= deadline {
                return Ok(ReloadResponse {
                    waited_secs: started.elapsed().as_secs(),
                    ready: false,
                    affected,
                });
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    pub async fn list_programs(&self) -> anyhow::Result<Vec<ProgramSummary>> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::ListPrograms { reply: tx }).await?;
        Ok(rx.await?)
    }

    pub async fn get_program(&self, id: Uuid) -> anyhow::Result<ProgramInfo> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::GetProgram { id, reply: tx }).await?;
        rx.await?
    }

    pub async fn get_program_events(
        &self,
        id: Uuid,
    ) -> anyhow::Result<Vec<common::ProgramEventRecord>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::GetProgramEvents { id, reply: tx })
            .await?;
        Ok(rx.await?)
    }

    /// Query persisted events with filters. `program_id` scopes to one program
    /// when set; all other filters (time window, type, exit code, free-text)
    /// apply globally otherwise.
    pub async fn query_events(
        &self,
        query: crate::event_db::EventQuery,
    ) -> anyhow::Result<Vec<common::ProgramEventRecord>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::QueryEvents { query, reply: tx })
            .await?;
        Ok(rx.await?)
    }

    /// Event retention statistics, optionally scoped to one program.
    pub async fn event_stats(
        &self,
        program_id: Option<Uuid>,
    ) -> anyhow::Result<common::EventStats> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::EventStats {
                program_id,
                reply: tx,
            })
            .await?;
        Ok(rx.await?)
    }

    pub async fn shutdown(&self) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::Shutdown { reply: tx }).await?;
        Ok(rx.await?)
    }

    pub async fn create_program(&self, req: CreateProgramRequest) -> anyhow::Result<Vec<Uuid>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::CreateProgram {
                config: req,
                reply: tx,
            })
            .await?;
        rx.await?
    }

    pub async fn update_program(&self, id: Uuid, req: UpdateProgramRequest) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::UpdateProgram {
                id,
                request: req,
                reply: tx,
            })
            .await?;
        rx.await?
    }

    pub async fn start_program(&self, id: Uuid) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::StartProgram { id, reply: tx })
            .await?;
        rx.await?
    }

    pub async fn stop_program(&self, id: Uuid, force: bool) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::StopProgram {
                id,
                force,
                reply: tx,
            })
            .await?;
        rx.await?
    }

    pub async fn restart_program(&self, id: Uuid) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::RestartProgram { id, reply: tx })
            .await?;
        rx.await?
    }

    pub async fn remove_program(&self, id: Uuid) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::RemoveProgram { id, reply: tx })
            .await?;
        rx.await?
    }

    pub async fn start_group(&self, group: String) -> anyhow::Result<Vec<Uuid>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::StartGroup { group, reply: tx })
            .await?;
        rx.await?
    }

    pub async fn stop_group(&self, group: String, force: bool) -> anyhow::Result<Vec<Uuid>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::StopGroup {
                group,
                force,
                reply: tx,
            })
            .await?;
        rx.await?
    }

    pub async fn batch_programs(
        &self,
        req: BatchProgramRequest,
    ) -> anyhow::Result<BatchProgramResponse> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::BatchPrograms {
                request: req,
                reply: tx,
            })
            .await?;

        // Unwrap oneshot result:
        // rx.await? -> Result<BatchProgramResponse, RecvError>
        // ?         -> BatchProgramResponse (inner business Result)
        rx.await?
    }

    pub async fn restart_group(&self, group: String) -> anyhow::Result<Vec<Uuid>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::RestartGroup { group, reply: tx })
            .await?;
        rx.await?
    }

    pub async fn health_check(&self) -> anyhow::Result<HealthResponse> {
        let (tx, rx) = oneshot::channel();
        if self
            .tx
            .send(Command::HealthCheck { reply: tx })
            .await
            .is_err()
        {
            return Ok(HealthResponse {
                status: "down".to_string(),
                components: HashMap::new(),
            });
        }
        Ok(rx.await?)
    }

    pub async fn apply_stack(&self, req: StackApplyRequest) -> anyhow::Result<Vec<String>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::ApplyStack {
                request: req,
                reply: tx,
            })
            .await?;
        rx.await?
    }

    pub async fn dump_programs(&self) -> anyhow::Result<Vec<ProgramConfig>> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::DumpPrograms { reply: tx }).await?;
        Ok(rx.await?)
    }

    pub async fn signal_program(&self, id: Uuid, signal: Signal) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::SignalProgram {
                id,
                signal,
                reply: tx,
            })
            .await?;
        rx.await?
    }

    pub async fn generate_metrics(&self) -> anyhow::Result<String> {
        let (tx, rx) = oneshot::channel();
        if self
            .tx
            .send(Command::GenerateMetrics { reply: tx })
            .await
            .is_err()
        {
            return Err(anyhow::anyhow!("Manager is down"));
        }
        Ok(rx.await?)
    }

    pub async fn get_system_stats(&self) -> anyhow::Result<SystemStats> {
        let (tx, rx) = oneshot::channel();
        if self
            .tx
            .send(Command::GetSystemStats { reply: tx })
            .await
            .is_err()
        {
            return Err(anyhow::anyhow!("Manager is down"));
        }
        Ok(rx.await?)
    }
}
