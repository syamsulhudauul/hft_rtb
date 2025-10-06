use std::borrow::Cow;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use thiserror::Error;
use uuid::Uuid;

pub type Symbol<'a> = Cow<'a, str>;

#[derive(Debug, Clone)]
pub struct Tick<'a> {
    pub symbol: Symbol<'a>,
    pub price: f64,
    pub size: f64,
    pub ts: u64,
    pub raw: Cow<'a, [u8]>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OwnedTick {
    pub symbol: String,
    pub price: f64,
    pub size: f64,
    pub ts: u64,
    pub raw: Vec<u8>,
}

impl<'a> Tick<'a> {
    pub fn owned(self) -> Tick<'static> {
        Tick {
            symbol: Cow::Owned(self.symbol.into_owned()),
            price: self.price,
            size: self.size,
            ts: self.ts,
            raw: Cow::Owned(self.raw.into_owned()),
        }
    }

    pub fn into_bytes(self) -> Bytes {
        Bytes::from(self.raw.into_owned())
    }

    pub fn to_owned_tick(&self) -> OwnedTick {
        OwnedTick {
            symbol: self.symbol.to_string(),
            price: self.price,
            size: self.size,
            ts: self.ts,
            raw: self.raw.to_vec(),
        }
    }
}

impl From<OwnedTick> for Tick<'static> {
    fn from(owned: OwnedTick) -> Self {
        Tick {
            symbol: Cow::Owned(owned.symbol),
            price: owned.price,
            size: owned.size,
            ts: owned.ts,
            raw: Cow::Owned(owned.raw),
        }
    }
}

impl<'a> From<&Tick<'a>> for OwnedTick {
    fn from(tick: &Tick<'a>) -> Self {
        tick.to_owned_tick()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionContext {
    pub correlation_id: Uuid,
    pub ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionKind {
    Bid { venue: String, price: f64, size: f64 },
    Ask { venue: String, price: f64, size: f64 },
    Cancel { venue: String },
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub ctx: ActionContext,
    pub kind: ActionKind,
}

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("decode error: {0}")]
    Decode(String),
    #[error("ingest error: {0}")]
    Ingest(String),
    #[error("strategy error: {0}")]
    Strategy(String),
    #[error("dispatch error: {0}")]
    Dispatch(String),
}

pub fn encode_tick<'a>(tick: &Tick<'a>) -> Result<Vec<u8>, bincode::Error> {
    let owned_tick = tick.to_owned_tick();
    bincode::serialize(&owned_tick)
}

pub fn decode_tick(data: &[u8]) -> Result<Tick<'static>, bincode::Error> {
    let owned_tick: OwnedTick = bincode::deserialize(data)?;
    Ok(owned_tick.into())
}

pub fn tick_from_parts(symbol: &str, price: f64, size: f64, ts: u64, raw: &[u8]) -> Tick<'static> {
    Tick {
        symbol: Cow::Owned(symbol.to_string()),
        price,
        size,
        ts,
        raw: Cow::Owned(raw.to_vec()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub symbol: String,
    pub price: f64,
    pub ts: u64,
}

impl From<&Tick<'_>> for Snapshot {
    fn from(value: &Tick<'_>) -> Self {
        Snapshot {
            symbol: value.symbol.to_string(),
            price: value.price,
            ts: value.ts,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlCommand {
    pub strategy: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KafkaMetadata {
    pub partition: i32,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IngestSource {
    Tcp,
    Kafka(KafkaMetadata),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestEnvelope {
    pub tick: OwnedTick,
    pub source: IngestSource,
}

impl IngestEnvelope {
    pub fn new(tick: Tick<'_>) -> Self {
        IngestEnvelope {
            tick: tick.to_owned_tick(),
            source: IngestSource::Tcp,
        }
    }
}

impl<'a> From<Tick<'a>> for IngestEnvelope {
    fn from(tick: Tick<'a>) -> Self {
        IngestEnvelope::new(tick)
    }
}

impl From<Action> for ByteBuf {
    fn from(value: Action) -> Self {
        ByteBuf::from(bincode::serialize(&value).unwrap())
    }
}
