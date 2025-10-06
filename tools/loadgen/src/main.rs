use std::env;
use std::net::SocketAddr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use bytes::Bytes;
use common::{encode_tick, tick_from_parts};
use futures::SinkExt;
use rand::distributions::{Distribution, Uniform};
use rand::rngs::ThreadRng;
use tokio::net::TcpStream;
use tokio::time::sleep;
use tokio_util::codec::{FramedWrite, LengthDelimitedCodec};
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let target = env_socket("TARGET", "127.0.0.1:7000")?;
    let rate = env_rate();

    loop {
        match TcpStream::connect(target).await {
            Ok(stream) => {
                info!(?target, "connected");
                run_stream(stream, rate).await?;
            }
            Err(err) => {
                tracing::error!(?err, "connect failed");
                sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn run_stream(stream: TcpStream, rate: u64) -> Result<()> {
    let mut framed = FramedWrite::new(stream, LengthDelimitedCodec::new());
    let mut rng = rand::thread_rng();
    let price = Uniform::new(10.0, 20.0);
    let size = Uniform::new(50.0, 200.0);
    let symbols = ["AAA", "BBB", "CCC", "DDD"];
    let mut sent = 0u64;
    let mut window = Instant::now();

    loop {
        for symbol in symbols.iter() {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64;
            let tick = tick_from_parts(
                symbol,
                price.sample(&mut rng),
                size.sample(&mut rng),
                ts,
                &[0; 16],
            );
            let payload = encode_tick(&tick)?;
            framed.send(Bytes::from(payload)).await?;
            sent += 1;
        }

        if sent >= rate {
            let elapsed = window.elapsed();
            if elapsed < Duration::from_secs(1) {
                sleep(Duration::from_secs(1) - elapsed).await;
            }
            sent = 0;
            window = Instant::now();
        }
    }
}

fn env_socket(key: &str, default: &str) -> Result<SocketAddr> {
    let value = env::var(key).unwrap_or_else(|_| default.to_string());
    Ok(value.parse()?)
}

fn env_rate() -> u64 {
    env::var("RATE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000)
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(Level::INFO)
        .init();
}
