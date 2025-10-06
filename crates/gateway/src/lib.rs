use std::net::SocketAddr;

use anyhow::Result;
use common::{IngestEnvelope, IngestSource, PipelineError};
use decoder::decode_bytes;
use futures::StreamExt;
use metrics::{counter, histogram};
use tokio::net::TcpListener;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::{error, info, instrument};

#[instrument(skip(tx))]
pub async fn run_tcp_gateway(addr: SocketAddr, tx: Sender<IngestEnvelope<'static>>) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(?addr, "tcp gateway bound");
    let mut incoming = TcpListenerStream::new(listener);
    while let Some(stream) = incoming.next().await {
        let tx = tx.clone();
        match stream {
            Ok(stream) => {
                tokio::spawn(async move {
                    let codec = LengthDelimitedCodec::builder().big_endian().new_codec();
                    let mut framed = Framed::new(stream, codec);
                    while let Some(frame) = framed.next().await {
                        match frame {
                            Ok(mut chunk) => {
                                counter!("gateway.tcp.frames", 1);
                                let len = chunk.len();
                                histogram!("gateway.tcp.frame_bytes", len as f64);
                                let bytes = chunk.freeze();
                                match decode_bytes(bytes) {
                                    Ok(tick) => {
                                        if tx.send(IngestEnvelope { tick, source: IngestSource::Tcp }).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(err) => {
                                        error!(?err, "decode error");
                                        counter!("gateway.tcp.decode_fail", 1);
                                    }
                                }
                            }
                            Err(err) => {
                                error!(?err, "frame read error");
                                counter!("gateway.tcp.read_fail", 1);
                                break;
                            }
                        }
                    }
                });
            }
            Err(err) => {
                error!(?err, "accept error");
                counter!("gateway.tcp.accept_fail", 1);
            }
        }
    }
    Ok(())
}

pub fn map_error(err: impl ToString, kind: &'static str) -> PipelineError {
    match kind {
        "decode" => PipelineError::Decode(err.to_string()),
        "ingest" => PipelineError::Ingest(err.to_string()),
        _ => PipelineError::Ingest(err.to_string()),
    }
}

#[cfg(feature = "with-kafka")]
pub mod kafka {
    use std::time::Duration;

    use anyhow::Result;
    use common::{IngestEnvelope, IngestSource, KafkaMetadata, PipelineError};
    use metrics::counter;
    use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
    use rdkafka::message::BorrowedMessage;
    use rdkafka::{ClientConfig, Message};
    use tokio::sync::mpsc::Sender;
    use tokio_stream::StreamExt;
    use tracing::{error, info, instrument};

    use crate::map_error;

    #[instrument(skip(tx))]
    pub async fn run_kafka_consumer(
        brokers: &str,
        group: &str,
        topic: &str,
        tx: Sender<IngestEnvelope<'static>>,
    ) -> Result<()> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "false")
            .create()?;

        consumer.subscribe(&[topic])?;
        info!(topic, "kafka consumer started");

        let mut stream = consumer.stream();
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(message) => {
                    counter!("gateway.kafka.messages", 1);
                    if let Some(payload) = message.payload() {
                        match common::decode_tick(payload) {
                            Ok(tick) => {
                                let env = IngestEnvelope {
                                    tick: tick.owned(),
                                    source: IngestSource::Kafka(KafkaMetadata {
                                        partition: message.partition(),
                                        offset: message.offset(),
                                    }),
                                };
                                if tx.send(env).await.is_err() {
                                    break;
                                }
                                consumer.commit_message(&message, CommitMode::Async)?;
                            }
                            Err(err) => {
                                error!(?err, "decode failure");
                                counter!("gateway.kafka.decode_fail", 1);
                            }
                        }
                    }
                }
                Err(err) => {
                    counter!("gateway.kafka.poll_fail", 1);
                    error!(?err, "kafka poll error");
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }

        Ok(())
    }

    pub fn resume_message(msg: &BorrowedMessage<'_>) -> Result<IngestEnvelope<'static>, PipelineError> {
        let payload = msg
            .payload()
            .ok_or_else(|| map_error("missing payload", "decode"))?;
        let tick =
            common::decode_tick(payload).map_err(|e| map_error(e, "decode"))?.owned();
        Ok(IngestEnvelope {
            tick,
            source: IngestSource::Kafka(KafkaMetadata {
                partition: msg.partition(),
                offset: msg.offset(),
            }),
        })
    }
}
