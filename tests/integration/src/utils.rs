use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub struct TestConfig {
    pub host: String,
    pub gateway_port: u16,
    pub metrics_port: u16,
    pub admin_port: u16,
    pub kafka_brokers: String,
    #[allow(dead_code)]
    pub redis_url: String,
    #[allow(dead_code)]
    pub test_duration: Duration,
    pub message_rate: u64,
}

impl TestConfig {
    pub fn gateway_url(&self) -> String {
        format!("http://{}:{}", self.host, self.gateway_port)
    }

    pub fn metrics_url(&self) -> String {
        format!("http://{}:{}/metrics", self.host, self.metrics_port)
    }

    #[allow(dead_code)]
    pub fn admin_url(&self) -> String {
        format!("http://{}:{}", self.host, self.admin_port)
    }

    pub fn grpc_endpoint(&self) -> String {
        format!("{}:{}", self.host, self.admin_port)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: u64,
    pub uptime_seconds: u64,
    pub version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub timestamp: u64,
    pub latency_p50: Option<f64>,
    pub latency_p95: Option<f64>,
    pub latency_p99: Option<f64>,
    pub throughput_per_second: Option<f64>,
    pub active_connections: Option<u64>,
    pub processed_messages: Option<u64>,
    pub error_count: Option<u64>,
}

pub struct TestMessage {
    pub symbol: String,
    pub price: f64,
    pub size: u64,
    pub timestamp: u64,
}

impl TestMessage {
    pub fn new(symbol: &str, price: f64, size: u64) -> Self {
        Self {
            symbol: symbol.to_string(),
            price,
            size,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
        }
    }

    pub fn to_tick(&self) -> common::Tick<'static> {
        use std::borrow::Cow;
        common::Tick {
            symbol: Cow::Owned(self.symbol.clone()),
            price: self.price,
            size: self.size as f64,
            ts: self.timestamp,
            raw: Cow::Owned(vec![]),
        }
    }
}

pub async fn wait_for_service(url: &str, timeout: Duration) -> Result<()> {
    let client = reqwest::Client::new();
    let start = std::time::Instant::now();
    
    info!("Waiting for service at {} (timeout: {:?})", url, timeout);
    
    while start.elapsed() < timeout {
        match client.get(url).send().await {
            Ok(response) if response.status().is_success() => {
                info!("Service at {} is ready", url);
                return Ok(());
            }
            Ok(response) => {
                debug!("Service at {} returned status: {}", url, response.status());
            }
            Err(e) => {
                debug!("Failed to connect to {}: {}", url, e);
            }
        }
        
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    
    Err(anyhow::anyhow!("Service at {} not ready within {:?}", url, timeout))
}

pub async fn check_health(config: &TestConfig) -> Result<HealthResponse> {
    let client = reqwest::Client::new();
    let url = format!("{}/health", config.gateway_url());
    
    let response = client
        .get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .context("Failed to send health check request")?;
    
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Health check failed with status: {}", response.status()));
    }
    
    let health: HealthResponse = response
        .json()
        .await
        .context("Failed to parse health response")?;
    
    Ok(health)
}

pub async fn get_metrics(config: &TestConfig) -> Result<String> {
    let client = reqwest::Client::new();
    
    let response = client
        .get(&config.metrics_url())
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .context("Failed to fetch metrics")?;
    
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Metrics request failed with status: {}", response.status()));
    }
    
    let metrics_text = response
        .text()
        .await
        .context("Failed to read metrics response")?;
    
    Ok(metrics_text)
}

pub fn parse_prometheus_metrics(metrics_text: &str) -> Result<MetricsSnapshot> {
    let mut snapshot = MetricsSnapshot {
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        latency_p50: None,
        latency_p95: None,
        latency_p99: None,
        throughput_per_second: None,
        active_connections: None,
        processed_messages: None,
        error_count: None,
    };

    for line in metrics_text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let metric_name = parts[0];
        let value: f64 = parts[1].parse().unwrap_or(0.0);

        match metric_name {
            name if name.contains("latency") && name.contains("0.5") => {
                snapshot.latency_p50 = Some(value);
            }
            name if name.contains("latency") && name.contains("0.95") => {
                snapshot.latency_p95 = Some(value);
            }
            name if name.contains("latency") && name.contains("0.99") => {
                snapshot.latency_p99 = Some(value);
            }
            name if name.contains("throughput") => {
                snapshot.throughput_per_second = Some(value);
            }
            name if name.contains("connections") => {
                snapshot.active_connections = Some(value as u64);
            }
            name if name.contains("processed") => {
                snapshot.processed_messages = Some(value as u64);
            }
            name if name.contains("error") => {
                snapshot.error_count = Some(value as u64);
            }
            _ => {}
        }
    }

    Ok(snapshot)
}

pub fn generate_test_symbols() -> Vec<String> {
    vec![
        "AAPL".to_string(),
        "GOOGL".to_string(),
        "MSFT".to_string(),
        "AMZN".to_string(),
        "TSLA".to_string(),
        "META".to_string(),
        "NVDA".to_string(),
        "NFLX".to_string(),
    ]
}

pub fn calculate_latency_stats(latencies: &[Duration]) -> (f64, f64, f64, f64) {
    if latencies.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }

    let mut sorted = latencies.to_vec();
    sorted.sort();

    let len = sorted.len();
    let sum: Duration = sorted.iter().sum();
    let avg = sum.as_nanos() as f64 / len as f64 / 1_000_000.0; // Convert to milliseconds

    let p50 = sorted[len * 50 / 100].as_nanos() as f64 / 1_000_000.0;
    let p95 = sorted[len * 95 / 100].as_nanos() as f64 / 1_000_000.0;
    let p99 = sorted[len * 99 / 100].as_nanos() as f64 / 1_000_000.0;

    (avg, p50, p95, p99)
}