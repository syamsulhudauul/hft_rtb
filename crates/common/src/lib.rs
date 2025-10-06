use std::borrow::Cow;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use thiserror::Error;
use uuid::Uuid;

pub type Symbol<'a> = Cow<'a, str>;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(borrow)]
pub struct Tick<'a> {
    #[serde(borrow)]
    pub symbol: Symbol<'a>,
    pub price: f64,
    pub size: f64,
    pub ts: u64,
    #[serde(borrow)]
    pub raw: Cow<'a, [u8]>,
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
    bincode::serialize(tick)
}

pub fn decode_tick<'a>(data: &'a [u8]) -> Result<Tick<'a>, bincode::Error> {
    bincode::deserialize(data)
}

pub fn tick_from_parts(symbol: &str, price: f64, size: f64, ts: u64, raw: &[u8]) -> Tick {
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
pub struct IngestEnvelope<'a> {
    pub tick: Tick<'a>,
    pub source: IngestSource,
}

impl<'a> IngestEnvelope<'a> {
    pub fn owned(self) -> IngestEnvelope<'static> {
        IngestEnvelope {
            tick: self.tick.owned(),
            source: self.source,
        }
    }
}

impl<'a> From<Tick<'a>> for IngestEnvelope<'a> {
    fn from(tick: Tick<'a>) -> Self {
        IngestEnvelope {
            tick,
            source: IngestSource::Tcp,
        }
    }
}

impl From<Action> for ByteBuf {
    fn from(value: Action) -> Self {
        ByteBuf::from(bincode::serialize(&value).unwrap())
    }
}
