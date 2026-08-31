use nix::sys::signal::Signal;
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use common::{
    BatchProgramRequest, BatchProgramResponse, CreateProgramRequest, HealthResponse, ProgramConfig,
    ProgramInfo, ProgramSummary, StackApplyRequest, SystemStats, UpdateProgramRequest,
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

    pub async fn reload(&self) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::Reload { reply: tx }).await?;
        rx.await?
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
