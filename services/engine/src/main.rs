use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use common::{Action, IngestEnvelope, PipelineError};
use control_plane::{spawn_admin, StrategyManager};
use engine_core::{MomentumStrategy, Strategy, StrategyEngine};
use gateway::run_tcp_gateway;
use metrics::counter;
use order_router::OrderRouter;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{error, info, Level};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let gateway_addr = env_socket("GATEWAY_ADDR", "0.0.0.0:7000")?;
    let metrics_addr = env_socket("METRICS_ADDR", "0.0.0.0:9000")?;
    let admin_addr = env_socket("ADMIN_ADDR", "0.0.0.0:9100")?;

    control_plane::init_metrics(metrics_addr).await?;

    let (ingest_tx, ingest_rx) = mpsc::channel::<IngestEnvelope<'static>>(1024);
    let (action_tx, action_rx) = mpsc::channel::<Action>(1024);

    let strategy: Arc<dyn Strategy> = Arc::new(MomentumStrategy::new(0.5, "SIM"));
    let engine = Arc::new(StrategyEngine::new(strategy, 64));
    let engine_handle = engine.clone().spawn(ingest_rx, action_tx);

    let router_handle = spawn_router(action_rx);
    let gateway_handle = tokio::spawn(async move {
        run_tcp_gateway(gateway_addr, ingest_tx)
            .await
            .map_err(|e| PipelineError::Ingest(e.to_string()))
    });

    let manager = Arc::new(EngineManager { engine: engine.clone() });
    let (shutdown_tx, admin_task) = spawn_admin(manager, admin_addr);
    let admin_handle = admin_task;

    info!(?gateway_addr, ?metrics_addr, ?admin_addr, "engine started");

    tokio::signal::ctrl_c().await?;
    info!("ctrl_c received, shutting down");

    let _ = shutdown_tx.send(());

    engine_handle.abort();
    router_handle.abort();
    gateway_handle.abort();
    admin_handle.abort();

    let _ = engine_handle.await;
    let _ = router_handle.await;
    let _ = gateway_handle.await;
    let _ = admin_handle.await;

    Ok(())
}

fn init_tracing() {
    let filter = env::var("RUST_LOG").unwrap_or_else(|_| "info".into());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_max_level(Level::INFO)
        .init();
}

fn env_socket(key: &str, default: &str) -> Result<SocketAddr> {
    let value = env::var(key).unwrap_or_else(|_| default.to_string());
    Ok(value.parse()?)
}

fn spawn_router(mut rx: mpsc::Receiver<Action>) -> JoinHandle<Result<(), PipelineError>> {
    tokio::spawn(async move {
        let mut router = OrderRouter::with_default(Duration::from_millis(2));
        while let Some(action) = rx.recv().await {
            if let Err(err) = router.dispatch(action).await {
                counter!("router.errors", 1);
                error!(?err, "router dispatch error");
            }
        }
        Ok(())
    })
}

struct EngineManager {
    engine: Arc<StrategyEngine>,
}

impl StrategyManager for EngineManager {
    fn apply(&self, strategy: &str, params: serde_json::Value) -> Result<()> {
        match strategy {
            "momentum" => {
                let threshold = params
                    .get("threshold")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.5);
                let venue = params
                    .get("venue")
                    .and_then(|v| v.as_str())
                    .unwrap_or("SIM");
                let strat: Arc<dyn Strategy> = Arc::new(MomentumStrategy::new(threshold, venue));
                self.engine.swap_strategy(strat);
                Ok(())
            }
            _ => Err(anyhow::anyhow!("strategy not found")),
        }
    }
}
