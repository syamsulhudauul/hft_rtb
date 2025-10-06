use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use std::sync::{Mutex, RwLock as StdRwLock};
use common::{Action, ActionContext, ActionKind, IngestEnvelope, OwnedTick, PipelineError, Snapshot, Tick};
use core_affinity::CoreId;
use crossbeam::queue::SegQueue;
use metrics::{counter, gauge, histogram};
use parking_lot::RwLock;
use rand::distributions::{Distribution, Uniform};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{debug, error, info, instrument};
use uuid::Uuid;

#[repr(align(64))]
struct Aligned<T>(T);

#[derive(Clone)]
pub struct StrategyContext {
    pub tick: OwnedTick,
    pub snapshot: Snapshot,
    pub mean: f64,
    pub momentum: f64,
}

pub trait Strategy: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn on_event(&self, ctx: &StrategyContext) -> Result<ActionKind>;
    fn warmup(&self) -> usize {
        100
    }
}

struct SymbolState {
    prices: VecDeque<f64>,
    last_ts: u64,
}

impl SymbolState {
    fn new(capacity: usize) -> Self {
        Self {
            prices: VecDeque::with_capacity(capacity),
            last_ts: 0,
        }
    }

    fn update(&mut self, tick: &Tick<'_>, capacity: usize) {
        if self.prices.len() == capacity {
            self.prices.pop_front();
        }
        self.prices.push_back(tick.price);
        self.last_ts = tick.ts;
    }

    fn mean(&self) -> f64 {
        let sum: f64 = self.prices.iter().copied().sum();
        if self.prices.is_empty() {
            0.0
        } else {
            sum / self.prices.len() as f64
        }
    }

    fn momentum(&self) -> f64 {
        if self.prices.len() < 2 {
            return 0.0;
        }
        let first = self.prices.front().copied().unwrap_or_default();
        let last = self.prices.back().copied().unwrap_or_default();
        last - first
    }
}

pub struct StateCache {
    capacity: usize,
    inner: Aligned<RwLock<HashMap<String, SymbolState>>>,
}

impl StateCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            inner: Aligned(RwLock::new(HashMap::new())),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    fn update(&self, tick: &OwnedTick) -> StrategyContext {
        let tick_ref: Tick<'static> = tick.clone().into();
        let mut guard = self.inner.0.write();
        let entry = guard
            .entry(tick.symbol.clone())
            .or_insert_with(|| SymbolState::new(self.capacity));
        entry.update(&tick_ref, self.capacity);
        let snapshot = Snapshot::from(&tick_ref);
        let mean = entry.mean();
        let momentum = entry.momentum();
        StrategyContext {
            tick: tick.clone(),
            snapshot,
            mean,
            momentum,
        }
    }
}

pub struct StrategyEngine {
    strategy: Arc<StdRwLock<Arc<dyn Strategy>>>,
    state: Arc<StateCache>,
    prng: Arc<Mutex<ChaCha8Rng>>,
    jitter: Uniform<u64>,
    fallback: SegQueue<ActionKind>,
}

impl StrategyEngine {
    pub fn new(strategy: Arc<dyn Strategy>, window: usize) -> Self {
        Self {
            strategy: Arc::new(StdRwLock::new(strategy)),
            state: Arc::new(StateCache::new(window)),
            prng: Arc::new(Mutex::new(ChaCha8Rng::from_entropy())),
            jitter: Uniform::new_inclusive(0, 500),
            fallback: SegQueue::new(),
        }
    }

    pub fn swap_strategy(&self, strategy: Arc<dyn Strategy>) {
        *self.strategy.write().unwrap() = strategy;
    }

    pub fn state(&self) -> Arc<StateCache> {
        self.state.clone()
    }

    #[instrument(skip(self, rx, tx))]
    pub fn spawn(self: Arc<Self>, mut rx: Receiver<IngestEnvelope>, tx: Sender<Action>) -> JoinHandle<Result<(), PipelineError>> {
        tokio::spawn(async move {
            let core = select_core();
            if let Some(core) = core {
                let _ = core_affinity::set_for_current(core);
            }
            while let Some(env) = rx.recv().await {
                let tick = &env.tick;
                let ctx = self.state.update(tick);
                gauge!("engine.state.window", self.state.capacity() as f64);
                let strategy = self.strategy.read().unwrap().clone();
                let started = Instant::now();
                let decision = strategy
                    .on_event(&ctx)
                    .map_err(|e| PipelineError::Strategy(e.to_string()))
                    .or_else(|err| self.dequeue_fallback(err));

                match decision {
                    Ok(kind) => {
                        histogram!("engine.strategy.latency", started.elapsed().as_micros() as f64);
                        let action = Action {
                            ctx: ActionContext {
                                correlation_id: Uuid::new_v4(),
                                ts: tick.ts,
                            },
                            kind,
                        };
                        if tx.send(action).await.is_err() {
                            return Err(PipelineError::Dispatch("action channel closed".into()));
                        }
                    }
                    Err(err) => {
                        counter!("engine.strategy.errors", 1);
                        error!(?err, symbol = ?ctx.snapshot.symbol, "strategy failure");
                    }
                }
                let jitter = self.jitter.sample(&mut *self.prng.lock().unwrap());
                if jitter > 0 {
                    sleep(Duration::from_micros(jitter)).await;
                }
            }
            Ok(())
        })
    }

    fn dequeue_fallback(&self, err: PipelineError) -> Result<ActionKind, PipelineError> {
        if let Some(kind) = self.fallback.pop() {
            Ok(kind)
        } else {
            Err(err)
        }
    }

    pub fn enqueue_fallback(&self, action: ActionKind) {
        self.fallback.push(action);
    }
}

fn select_core() -> Option<CoreId> {
    core_affinity::get_core_ids()?.into_iter().next()
}

pub struct MomentumStrategy {
    threshold: f64,
    venue: String,
}

impl MomentumStrategy {
    pub fn new(threshold: f64, venue: impl Into<String>) -> Self {
        Self {
            threshold,
            venue: venue.into(),
        }
    }
}

impl Strategy for MomentumStrategy {
    fn name(&self) -> &str {
        "momentum"
    }

    fn warmup(&self) -> usize {
        10
    }

    fn on_event(&self, ctx: &StrategyContext) -> Result<ActionKind> {
        if ctx.momentum > self.threshold {
            return Ok(ActionKind::Bid {
                venue: self.venue.clone(),
                price: ctx.tick.price + 0.01,
                size: ctx.tick.size,
            });
        }
        if ctx.momentum < -self.threshold {
            return Ok(ActionKind::Ask {
                venue: self.venue.clone(),
                price: ctx.tick.price - 0.01,
                size: ctx.tick.size,
            });
        }
        Ok(ActionKind::Hold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{tick_from_parts, IngestSource};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn momentum_strategy_flags_trend() {
        let strategy: Arc<dyn Strategy> = Arc::new(MomentumStrategy::new(0.25, "TEST"));
        let engine = Arc::new(StrategyEngine::new(strategy, 16));
        let (tx_tick, rx_tick) = mpsc::channel(16);
        let (tx_action, mut rx_action) = mpsc::channel(16);

        let handle = engine.clone().spawn(rx_tick, tx_action);

        let mut price = 1.0;
        for i in 0..20 {
            price += 0.1;
            let tick = tick_from_parts("ABC", price, 100.0, i, &[0; 8]);
            tx_tick
                .send(IngestEnvelope {
                    tick: tick.to_owned_tick(),
                    source: IngestSource::Tcp,
                })
                .await
                .unwrap();
        }

        // Wait for multiple actions and find the first bid
        let mut found_bid = false;
        for _ in 0..10 {
            if let Some(action) = rx_action.recv().await {
                match action.kind {
                    ActionKind::Bid { .. } => {
                        found_bid = true;
                        break;
                    }
                    _ => continue,
                }
            } else {
                break;
            }
        }
        
        if !found_bid {
            panic!("expected to receive a bid action");
        }

        drop(tx_tick);
        handle.await.unwrap().unwrap();
    }
}
