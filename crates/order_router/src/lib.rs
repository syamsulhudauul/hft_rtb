use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use common::{Action, ActionKind, PipelineError};
use metrics::{counter, histogram};
use tokio::time::timeout;
use tower::util::BoxCloneService;
use tower::{Service, ServiceExt};
use tracing::{info, instrument, warn};

type DynService = BoxCloneService<Action, (), PipelineError>;

pub struct OrderRouter {
    adapters: HashMap<String, DynService>,
    timeout: Duration,
}

impl OrderRouter {
    pub fn new(timeout: Duration) -> Self {
        Self {
            adapters: HashMap::new(),
            timeout,
        }
    }

    pub fn with_default(timeout: Duration) -> Self {
        let mut router = Self::new(timeout);
        router.register_adapter("SIM", noop_adapter());
        router
    }

    pub fn register_adapter(&mut self, venue: &str, svc: DynService) {
        self.adapters.insert(venue.to_string(), svc);
    }

    #[instrument(skip(self, action))]
    pub async fn dispatch(&mut self, action: Action) -> Result<(), PipelineError> {
        match &action.kind {
            ActionKind::Hold => {
                counter!("router.hold", 1);
                Ok(())
            }
            ActionKind::Cancel { venue } => self.send(venue, action).await,
            ActionKind::Bid { venue, .. } | ActionKind::Ask { venue, .. } => {
                self.send(venue, action).await
            }
        }
    }

    async fn send(&mut self, venue: &str, action: Action) -> Result<(), PipelineError> {
        let svc = self
            .adapters
            .get_mut(venue)
            .ok_or_else(|| PipelineError::Dispatch(format!("missing venue {venue}")))?;
        let started = std::time::Instant::now();
        match timeout(self.timeout, svc.ready()).await {
            Ok(ready) => {
                ready.map_err(|e| PipelineError::Dispatch(e.to_string()))?;
                svc.call(action)
                    .await
                    .map_err(|e| PipelineError::Dispatch(e.to_string()))?;
                histogram!("router.dispatch.latency", started.elapsed().as_micros() as f64);
                counter!("router.dispatch.success", 1);
                Ok(())
            }
            Err(_) => {
                counter!("router.dispatch.timeout", 1);
                Err(PipelineError::Dispatch(format!("timeout for {venue}")))
            }
        }
    }
}

fn noop_adapter() -> DynService {
    BoxCloneService::new(tower::service_fn(|action: Action| async move {
        info!(?action.ctx.correlation_id, "simulated dispatch");
        counter!("router.simulated", 1);
        Ok(())
    }))
}

pub fn http_adapter(url: &str) -> DynService {
    let endpoint = url.to_string();
    BoxCloneService::new(tower::service_fn(move |action: Action| {
        let endpoint = endpoint.clone();
        async move {
            warn!(?endpoint, ?action.kind, "http adapter invoked");
            Ok(())
        }
    }))
}
