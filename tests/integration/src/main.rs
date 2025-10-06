use anyhow::Result;
use clap::{Arg, Command};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

mod tests;
mod utils;

use tests::*;
use utils::TestConfig;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let matches = Command::new("HFT RTB Integration Tests")
        .version("1.0")
        .about("Comprehensive integration tests for HFT RTB system")
        .arg(
            Arg::new("host")
                .long("host")
                .value_name("HOST")
                .help("Engine host address")
                .default_value("localhost"),
        )
        .arg(
            Arg::new("gateway-port")
                .long("gateway-port")
                .value_name("PORT")
                .help("Gateway port")
                .default_value("7001"),
        )
        .arg(
            Arg::new("metrics-port")
                .long("metrics-port")
                .value_name("PORT")
                .help("Metrics port")
                .default_value("9002"),
        )
        .arg(
            Arg::new("admin-port")
                .long("admin-port")
                .value_name("PORT")
                .help("Admin gRPC port")
                .default_value("9100"),
        )
        .arg(
            Arg::new("kafka-brokers")
                .long("kafka-brokers")
                .value_name("BROKERS")
                .help("Kafka broker addresses")
                .default_value("localhost:9092"),
        )
        .arg(
            Arg::new("redis-url")
                .long("redis-url")
                .value_name("URL")
                .help("Redis connection URL")
                .default_value("redis://localhost:6379"),
        )
        .arg(
            Arg::new("test")
                .long("test")
                .value_name("TEST_NAME")
                .help("Run specific test (health, metrics, kafka, grpc, latency, load, all)")
                .default_value("all"),
        )
        .arg(
            Arg::new("duration")
                .long("duration")
                .value_name("SECONDS")
                .help("Test duration in seconds")
                .default_value("30"),
        )
        .arg(
            Arg::new("rate")
                .long("rate")
                .value_name("RATE")
                .help("Message rate for load tests")
                .default_value("1000"),
        )
        .get_matches();

    let config = TestConfig {
        host: matches.get_one::<String>("host").unwrap().clone(),
        gateway_port: matches.get_one::<String>("gateway-port").unwrap().parse()?,
        metrics_port: matches.get_one::<String>("metrics-port").unwrap().parse()?,
        admin_port: matches.get_one::<String>("admin-port").unwrap().parse()?,
        kafka_brokers: matches.get_one::<String>("kafka-brokers").unwrap().clone(),
        redis_url: matches.get_one::<String>("redis-url").unwrap().clone(),
        test_duration: Duration::from_secs(matches.get_one::<String>("duration").unwrap().parse()?),
        message_rate: matches.get_one::<String>("rate").unwrap().parse()?,
    };

    let test_name = matches.get_one::<String>("test").unwrap();

    info!("Starting HFT RTB Integration Tests");
    info!("Configuration: {:?}", config);

    let start_time = Instant::now();
    let mut results = TestResults::new();

    match test_name.as_str() {
        "health" => {
            run_health_tests(&config, &mut results).await?;
        }
        "metrics" => {
            run_metrics_tests(&config, &mut results).await?;
        }
        "kafka" => {
            run_kafka_tests(&config, &mut results).await?;
        }
        "grpc" => {
            run_grpc_tests(&config, &mut results).await?;
        }
        "latency" => {
            run_latency_tests(&config, &mut results).await?;
        }
        "load" => {
            run_load_tests(&config, &mut results).await?;
        }
        "all" => {
            info!("Running all integration tests...");
            
            // Run tests in order of complexity
            run_health_tests(&config, &mut results).await?;
            run_metrics_tests(&config, &mut results).await?;
            run_kafka_tests(&config, &mut results).await?;
            run_grpc_tests(&config, &mut results).await?;
            run_latency_tests(&config, &mut results).await?;
            run_load_tests(&config, &mut results).await?;
        }
        _ => {
            error!("Unknown test: {}", test_name);
            std::process::exit(1);
        }
    }

    let total_duration = start_time.elapsed();
    
    // Print final results
    print_test_summary(&results, total_duration);

    if results.has_failures() {
        std::process::exit(1);
    }

    Ok(())
}

#[derive(Debug, Default)]
pub struct TestResults {
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub failures: Vec<String>,
}

impl TestResults {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pass(&mut self, test_name: &str) {
        self.passed += 1;
        info!("✅ {}", test_name);
    }

    pub fn fail(&mut self, test_name: &str, error: &str) {
        self.failed += 1;
        self.failures.push(format!("{}: {}", test_name, error));
        error!("❌ {}: {}", test_name, error);
    }

    pub fn skip(&mut self, test_name: &str, reason: &str) {
        self.skipped += 1;
        warn!("⏭️  {}: {}", test_name, reason);
    }

    pub fn has_failures(&self) -> bool {
        self.failed > 0
    }
}

fn print_test_summary(results: &TestResults, duration: Duration) {
    println!("\n{}", "=".repeat(60));
    println!("🧪 HFT RTB Integration Test Summary");
    println!("{}", "=".repeat(60));
    println!("⏱️  Total Duration: {:.2}s", duration.as_secs_f64());
    println!("✅ Passed: {}", results.passed);
    println!("❌ Failed: {}", results.failed);
    println!("⏭️  Skipped: {}", results.skipped);
    println!("📊 Total: {}", results.passed + results.failed + results.skipped);
    
    if !results.failures.is_empty() {
        println!("\n🔍 Failure Details:");
        for failure in &results.failures {
            println!("   • {}", failure);
        }
    }
    
    println!("{}", "=".repeat(60));
    
    if results.has_failures() {
        println!("❌ Some tests failed!");
    } else {
        println!("🎉 All tests passed!");
    }
}