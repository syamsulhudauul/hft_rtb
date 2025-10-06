use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use metrics_exporter_prometheus::PrometheusBuilder;
use serde_json::Value;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tonic::{transport::Server, Request, Response, Status};
use tracing::info;

pub mod proto {
    tonic::include_proto!("control");
}

use proto::control_service_server::{ControlService, ControlServiceServer};
use proto::{ControlRequest, ControlResponse};

pub trait StrategyManager: Send + Sync + 'static {
    fn apply(&self, strategy: &str, params: Value) -> Result<()>;
}

pub async fn init_metrics(addr: SocketAddr) -> Result<()> {
    PrometheusBuilder::new()
        .with_http_listener(addr)
        .install()?;
    info!(?addr, "metrics exporter running");
    Ok(())
}

pub struct ControlServer<M: StrategyManager> {
    manager: Arc<M>,
}

impl<M: StrategyManager> ControlServer<M> {
    pub fn new(manager: Arc<M>) -> Self {
        Self { manager }
    }

    pub async fn serve(self, addr: SocketAddr, shutdown: oneshot::Receiver<()>) -> Result<()> {
        info!(?addr, "admin server starting");
        Server::builder()
            .add_service(ControlServiceServer::new(self))
            .serve_with_shutdown(addr, async {
                let _ = shutdown.await;
            })
            .await?;
        Ok(())
    }
}

#[tonic::async_trait]
impl<M: StrategyManager> ControlService for ControlServer<M> {
    async fn update_strategy(
        &self,
        request: Request<ControlRequest>,
    ) -> Result<Response<ControlResponse>, Status> {
        let req = request.into_inner();
        let params: Value = if req.params.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&req.params).map_err(|e| Status::invalid_argument(e.to_string()))?
        };
        match self.manager.apply(&req.strategy, params) {
            Ok(()) => Ok(Response::new(ControlResponse {
                accepted: true,
                message: "strategy swapped".into(),
            })),
            Err(err) => Ok(Response::new(ControlResponse {
                accepted: false,
                message: err.to_string(),
            })),
        }
    }
}

pub fn spawn_admin<M: StrategyManager>(
    manager: Arc<M>,
    addr: SocketAddr,
) -> (oneshot::Sender<()>, JoinHandle<Result<()>>) {
    let (tx, rx) = oneshot::channel();
    let server = ControlServer::new(manager);
    let handle = tokio::spawn(async move { server.serve(addr, rx).await });
    (tx, handle)
}
