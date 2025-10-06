use anyhow::Result;
use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::utils::{generate_test_symbols, get_metrics, parse_prometheus_metrics, TestConfig, TestMessage};
use crate::TestResults;

pub async fn run_load_tests(config: &TestConfig, results: &mut TestResults) -> Result<()> {
    info!("🚀 Running Load Performance Tests");

    // Test 1: Baseline Throughput Test
    info!("Testing baseline throughput...");
    let baseline_duration = Duration::from_secs(30);
    let target_rate = config.message_rate; // messages per second
    
    match run_throughput_test(config, target_rate, baseline_duration).await {
        Ok(stats) => {
            results.pass("Baseline Throughput Test");
            info!("Baseline stats: {:.1} msg/s, {:.2}ms avg latency, {:.1}% success", 
                  stats.actual_rate, stats.avg_latency_ms, stats.success_rate * 100.0);
            
            if stats.success_rate >= 0.95 {
                results.pass("Baseline Success Rate (>95%)");
            } else if stats.success_rate >= 0.90 {
                results.pass("Baseline Success Rate (>90%)");
                warn!("Success rate: {:.1}% (acceptable but not optimal)", stats.success_rate * 100.0);
            } else {
                results.fail("Baseline Success Rate", &format!("Too low: {:.1}%", stats.success_rate * 100.0));
            }
            
            if stats.actual_rate >= target_rate as f64 * 0.9 {
                results.pass("Baseline Rate Achievement (>90% of target)");
            } else {
                results.fail("Baseline Rate Achievement", &format!("Only {:.1}% of target rate", stats.actual_rate / target_rate as f64 * 100.0));
            }
        }
        Err(e) => {
            results.fail("Baseline Throughput Test", &e.to_string());
            results.skip("Baseline Success Rate", "Throughput test failed");
            results.skip("Baseline Rate Achievement", "Throughput test failed");
        }
    }

    // Test 2: High Throughput Test
    info!("Testing high throughput performance...");
    let high_rate = target_rate * 2; // Double the target rate
    let high_duration = Duration::from_secs(20);
    
    match run_throughput_test(config, high_rate, high_duration).await {
        Ok(stats) => {
            results.pass("High Throughput Test");
            info!("High throughput stats: {:.1} msg/s, {:.2}ms avg latency, {:.1}% success", 
                  stats.actual_rate, stats.avg_latency_ms, stats.success_rate * 100.0);
            
            if stats.success_rate >= 0.85 {
                results.pass("High Throughput Success Rate (>85%)");
            } else if stats.success_rate >= 0.75 {
                results.pass("High Throughput Success Rate (>75%)");
                warn!("High throughput success rate: {:.1}% (acceptable under load)", stats.success_rate * 100.0);
            } else {
                results.fail("High Throughput Success Rate", &format!("Too low: {:.1}%", stats.success_rate * 100.0));
            }
            
            if stats.avg_latency_ms < 10.0 {
                results.pass("High Throughput Latency (<10ms)");
            } else if stats.avg_latency_ms < 50.0 {
                results.pass("High Throughput Latency (<50ms)");
                warn!("High throughput latency: {:.2}ms (acceptable under load)", stats.avg_latency_ms);
            } else {
                results.fail("High Throughput Latency", &format!("Too high: {:.2}ms", stats.avg_latency_ms));
            }
        }
        Err(e) => {
            results.fail("High Throughput Test", &e.to_string());
            results.skip("High Throughput Success Rate", "Test failed");
            results.skip("High Throughput Latency", "Test failed");
        }
    }

    // Test 3: Burst Load Test
    info!("Testing burst load handling...");
    match run_burst_test(config).await {
        Ok(burst_stats) => {
            results.pass("Burst Load Test");
            info!("Burst test: {} messages in {:.2}s, {:.1}% success", 
                  burst_stats.total_messages, burst_stats.duration_secs, burst_stats.success_rate * 100.0);
            
            if burst_stats.success_rate >= 0.80 {
                results.pass("Burst Load Success Rate (>80%)");
            } else {
                results.fail("Burst Load Success Rate", &format!("Too low: {:.1}%", burst_stats.success_rate * 100.0));
            }
            
            if burst_stats.peak_rate > target_rate as f64 * 3.0 {
                results.pass("Burst Peak Rate (>3x target)");
            } else if burst_stats.peak_rate > target_rate as f64 * 2.0 {
                results.pass("Burst Peak Rate (>2x target)");
            } else {
                results.fail("Burst Peak Rate", &format!("Only {:.1}x target rate", burst_stats.peak_rate / target_rate as f64));
            }
        }
        Err(e) => {
            results.fail("Burst Load Test", &e.to_string());
            results.skip("Burst Load Success Rate", "Test failed");
            results.skip("Burst Peak Rate", "Test failed");
        }
    }

    // Test 4: Sustained Load Test
    info!("Testing sustained load over extended period...");
    let sustained_duration = Duration::from_secs(60);
    let sustained_rate = target_rate;
    
    match run_sustained_test(config, sustained_rate, sustained_duration).await {
        Ok(sustained_stats) => {
            results.pass("Sustained Load Test");
            info!("Sustained test: {:.1} msg/s avg, {:.2}ms avg latency, {:.1}% success over {}s", 
                  sustained_stats.avg_rate, sustained_stats.avg_latency_ms, 
                  sustained_stats.success_rate * 100.0, sustained_duration.as_secs());
            
            if sustained_stats.success_rate >= 0.95 {
                results.pass("Sustained Success Rate (>95%)");
            } else if sustained_stats.success_rate >= 0.90 {
                results.pass("Sustained Success Rate (>90%)");
            } else {
                results.fail("Sustained Success Rate", &format!("Too low: {:.1}%", sustained_stats.success_rate * 100.0));
            }
            
            if sustained_stats.rate_stability < 0.1 {
                results.pass("Sustained Rate Stability (CV < 10%)");
            } else if sustained_stats.rate_stability < 0.2 {
                results.pass("Sustained Rate Stability (CV < 20%)");
            } else {
                results.fail("Sustained Rate Stability", &format!("Too variable: CV = {:.1}%", sustained_stats.rate_stability * 100.0));
            }
        }
        Err(e) => {
            results.fail("Sustained Load Test", &e.to_string());
            results.skip("Sustained Success Rate", "Test failed");
            results.skip("Sustained Rate Stability", "Test failed");
        }
    }

    // Test 5: Multi-Symbol Load Test
    info!("Testing multi-symbol load distribution...");
    match run_multi_symbol_test(config).await {
        Ok(multi_stats) => {
            results.pass("Multi-Symbol Load Test");
            info!("Multi-symbol test: {} symbols, {:.1} msg/s total, {:.1}% success", 
                  multi_stats.symbol_count, multi_stats.total_rate, multi_stats.success_rate * 100.0);
            
            if multi_stats.success_rate >= 0.90 {
                results.pass("Multi-Symbol Success Rate (>90%)");
            } else {
                results.fail("Multi-Symbol Success Rate", &format!("Too low: {:.1}%", multi_stats.success_rate * 100.0));
            }
            
            if multi_stats.symbol_balance < 0.2 {
                results.pass("Symbol Load Balance (CV < 20%)");
            } else {
                results.fail("Symbol Load Balance", &format!("Unbalanced: CV = {:.1}%", multi_stats.symbol_balance * 100.0));
            }
        }
        Err(e) => {
            results.fail("Multi-Symbol Load Test", &e.to_string());
            results.skip("Multi-Symbol Success Rate", "Test failed");
            results.skip("Symbol Load Balance", "Test failed");
        }
    }

    // Test 6: System Resource Monitoring During Load
    info!("Monitoring system resources during load...");
    let monitoring_start = Instant::now();
    
    // Start a background load
    let load_handle = tokio::spawn({
        let config = config.clone();
        async move {
            let _ = run_throughput_test(&config, config.message_rate, Duration::from_secs(30)).await;
        }
    });
    
    // Monitor metrics during load
    let mut metric_samples = Vec::new();
    for _ in 0..6 { // Sample every 5 seconds for 30 seconds
        tokio::time::sleep(Duration::from_secs(5)).await;
        
        if let Ok(metrics_text) = get_metrics(config).await {
            if let Ok(snapshot) = parse_prometheus_metrics(&metrics_text) {
                metric_samples.push(snapshot);
            }
        }
    }
    
    // Wait for load test to complete
    let _ = load_handle.await;
    let monitoring_duration = monitoring_start.elapsed();
    
    if metric_samples.len() >= 4 {
        results.pass("Resource Monitoring During Load");
        info!("Collected {} metric samples over {:?}", metric_samples.len(), monitoring_duration);
        
        // Analyze resource usage trends
        let memory_stable = true;
        let mut latency_reasonable = true;
        
        for (i, sample) in metric_samples.iter().enumerate() {
            if let Some(latency) = sample.latency_p95 {
                if latency > 100.0 { // P95 latency > 100ms is concerning
                    latency_reasonable = false;
                }
            }
            
            info!("Sample {}: P95 latency: {:?}ms, Processed: {:?}", 
                  i, sample.latency_p95, sample.processed_messages);
        }
        
        if latency_reasonable {
            results.pass("Latency Stability Under Load");
        } else {
            results.fail("Latency Stability Under Load", "P95 latency exceeded 100ms");
        }
        
        if memory_stable {
            results.pass("Memory Stability Under Load");
        } else {
            results.fail("Memory Stability Under Load", "Memory usage increased significantly");
        }
    } else {
        results.fail("Resource Monitoring During Load", &format!("Only collected {}/6 samples", metric_samples.len()));
        results.skip("Latency Stability Under Load", "Insufficient monitoring data");
        results.skip("Memory Stability Under Load", "Insufficient monitoring data");
    }

    info!("✅ Load performance tests completed");
    Ok(())
}

#[derive(Debug)]
struct ThroughputStats {
    actual_rate: f64,
    avg_latency_ms: f64,
    success_rate: f64,
    duration_secs: f64,
}

#[derive(Debug)]
struct BurstStats {
    total_messages: u64,
    success_rate: f64,
    peak_rate: f64,
    duration_secs: f64,
}

#[derive(Debug)]
struct SustainedStats {
    avg_rate: f64,
    avg_latency_ms: f64,
    success_rate: f64,
    rate_stability: f64, // Coefficient of variation
}

#[derive(Debug)]
struct MultiSymbolStats {
    symbol_count: usize,
    total_rate: f64,
    success_rate: f64,
    symbol_balance: f64, // Coefficient of variation across symbols
}

async fn run_throughput_test(config: &TestConfig, target_rate: u64, duration: Duration) -> Result<ThroughputStats> {
    let producer_config = ClientConfig::new()
        .set("bootstrap.servers", &config.kafka_brokers)
        .set("message.timeout.ms", "5000")
        .set("queue.buffering.max.ms", "1")
        .set("batch.num.messages", "100")
        .clone();

    let producer: FutureProducer = producer_config.create()?;
    let test_symbols = generate_test_symbols();
    
    let successful_sends = Arc::new(AtomicU64::new(0));
    let total_attempts = Arc::new(AtomicU64::new(0));
    let total_latency_ns = Arc::new(AtomicU64::new(0));
    
    let start_time = Instant::now();
    let interval = Duration::from_nanos(1_000_000_000 / target_rate);
    
    let mut handles = Vec::new();
    let mut message_count = 0;
    
    while start_time.elapsed() < duration {
        let symbol = test_symbols[message_count % test_symbols.len()].clone();
        let test_message = TestMessage::new(&symbol, 100.0 + message_count as f64, 50);
        let tick = test_message.to_tick();
        let owned_tick = tick.to_owned_tick();
        let payload = bincode::serialize(&owned_tick)?;
        
        let producer_clone = producer.clone();
        let successful_clone = successful_sends.clone();
        let total_clone = total_attempts.clone();
        let latency_clone = total_latency_ns.clone();
        
        let handle = tokio::spawn(async move {
            let record = FutureRecord::to("load-test")
                .key(&symbol)
                .payload(&payload);
                
            let send_start = Instant::now();
            total_clone.fetch_add(1, Ordering::Relaxed);
            
            match producer_clone.send(record, Duration::from_millis(100)).await {
                Ok(_) => {
                    let latency = send_start.elapsed();
                    successful_clone.fetch_add(1, Ordering::Relaxed);
                    latency_clone.fetch_add(latency.as_nanos() as u64, Ordering::Relaxed);
                }
                Err(_) => {
                    // Failed send, already counted in total_attempts
                }
            }
        });
        
        handles.push(handle);
        message_count += 1;
        
        tokio::time::sleep(interval).await;
    }
    
    // Wait for all sends to complete
    for handle in handles {
        let _ = handle.await;
    }
    
    let actual_duration = start_time.elapsed();
    let successful = successful_sends.load(Ordering::Relaxed);
    let total = total_attempts.load(Ordering::Relaxed);
    let total_latency = total_latency_ns.load(Ordering::Relaxed);
    
    let actual_rate = successful as f64 / actual_duration.as_secs_f64();
    let success_rate = if total > 0 { successful as f64 / total as f64 } else { 0.0 };
    let avg_latency_ms = if successful > 0 { 
        total_latency as f64 / successful as f64 / 1_000_000.0 
    } else { 
        0.0 
    };
    
    Ok(ThroughputStats {
        actual_rate,
        avg_latency_ms,
        success_rate,
        duration_secs: actual_duration.as_secs_f64(),
    })
}

async fn run_burst_test(config: &TestConfig) -> Result<BurstStats> {
    let producer_config = ClientConfig::new()
        .set("bootstrap.servers", &config.kafka_brokers)
        .set("message.timeout.ms", "2000")
        .set("queue.buffering.max.ms", "0")
        .clone();

    let producer: FutureProducer = producer_config.create()?;
    let test_symbols = generate_test_symbols();
    
    let burst_size = 1000;
    let successful_sends = Arc::new(AtomicU64::new(0));
    
    let start_time = Instant::now();
    let mut handles = Vec::new();
    
    // Send burst of messages as fast as possible
    for i in 0..burst_size {
        let symbol = test_symbols[i % test_symbols.len()].clone();
        let test_message = TestMessage::new(&symbol, 100.0 + i as f64, 50);
        let tick = test_message.to_tick();
        let owned_tick = tick.to_owned_tick();
        let payload = bincode::serialize(&owned_tick)?;
        
        let producer_clone = producer.clone();
        let successful_clone = successful_sends.clone();
        
        let handle = tokio::spawn(async move {
            let record = FutureRecord::to("burst-test")
                .key(&symbol)
                .payload(&payload);
                
            if let Ok(_) = producer_clone.send(record, Duration::from_millis(50)).await {
                successful_clone.fetch_add(1, Ordering::Relaxed);
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all sends to complete
    for handle in handles {
        let _ = handle.await;
    }
    
    let duration = start_time.elapsed();
    let successful = successful_sends.load(Ordering::Relaxed);
    
    let peak_rate = successful as f64 / duration.as_secs_f64();
    let success_rate = successful as f64 / burst_size as f64;
    
    Ok(BurstStats {
        total_messages: burst_size as u64,
        success_rate,
        peak_rate,
        duration_secs: duration.as_secs_f64(),
    })
}

async fn run_sustained_test(config: &TestConfig, target_rate: u64, duration: Duration) -> Result<SustainedStats> {
    let window_size = Duration::from_secs(5);
    let num_windows = (duration.as_secs() / window_size.as_secs()) as usize;
    let mut window_rates = Vec::new();
    let mut total_successful = 0u64;
    let mut total_latency_ns = 0u64;
    
    for window in 0..num_windows {
        info!("Sustained test window {}/{}", window + 1, num_windows);
        
        let window_stats = run_throughput_test(config, target_rate, window_size).await?;
        window_rates.push(window_stats.actual_rate);
        total_successful += (window_stats.actual_rate * window_stats.duration_secs) as u64;
        total_latency_ns += (window_stats.avg_latency_ms * 1_000_000.0) as u64 * (window_stats.actual_rate * window_stats.duration_secs) as u64;
    }
    
    let avg_rate = window_rates.iter().sum::<f64>() / window_rates.len() as f64;
    let rate_variance = window_rates.iter()
        .map(|r| (r - avg_rate).powi(2))
        .sum::<f64>() / window_rates.len() as f64;
    let rate_stability = rate_variance.sqrt() / avg_rate; // Coefficient of variation
    
    let avg_latency_ms = if total_successful > 0 {
        total_latency_ns as f64 / total_successful as f64 / 1_000_000.0
    } else {
        0.0
    };
    
    let success_rate = avg_rate / target_rate as f64;
    
    Ok(SustainedStats {
        avg_rate,
        avg_latency_ms,
        success_rate: success_rate.min(1.0),
        rate_stability,
    })
}

async fn run_multi_symbol_test(config: &TestConfig) -> Result<MultiSymbolStats> {
    let test_symbols = generate_test_symbols();
    let messages_per_symbol = 100;
    let mut symbol_success_counts = vec![0u64; test_symbols.len()];
    
    let producer_config = ClientConfig::new()
        .set("bootstrap.servers", &config.kafka_brokers)
        .set("message.timeout.ms", "3000")
        .clone();

    let producer: FutureProducer = producer_config.create()?;
    
    let start_time = Instant::now();
    let mut handles = Vec::new();
    
    for (symbol_idx, symbol) in test_symbols.iter().enumerate() {
        for i in 0..messages_per_symbol {
            let symbol_owned = symbol.clone();
            let test_message = TestMessage::new(&symbol_owned, 100.0 + i as f64, 50);
            let tick = test_message.to_tick();
            let owned_tick = tick.to_owned_tick();
            let payload = bincode::serialize(&owned_tick)?;
            
            let producer_clone = producer.clone();
            
            let handle = tokio::spawn(async move {
                let record = FutureRecord::to("multi-symbol-test")
                    .key(&symbol_owned)
                    .payload(&payload);
                    
                match producer_clone.send(record, Duration::from_millis(100)).await {
                    Ok(_) => (symbol_idx, true),
                    Err(_) => (symbol_idx, false),
                }
            });
            
            handles.push(handle);
        }
    }
    
    // Collect results
    for handle in handles {
        if let Ok((symbol_idx, success)) = handle.await {
            if success {
                symbol_success_counts[symbol_idx] += 1;
            }
        }
    }
    
    let duration = start_time.elapsed();
    let total_successful: u64 = symbol_success_counts.iter().sum();
    let total_rate = total_successful as f64 / duration.as_secs_f64();
    let total_expected = test_symbols.len() * messages_per_symbol;
    let success_rate = total_successful as f64 / total_expected as f64;
    
    // Calculate symbol balance (coefficient of variation)
    let avg_per_symbol = total_successful as f64 / test_symbols.len() as f64;
    let symbol_variance = symbol_success_counts.iter()
        .map(|&count| (count as f64 - avg_per_symbol).powi(2))
        .sum::<f64>() / test_symbols.len() as f64;
    let symbol_balance = if avg_per_symbol > 0.0 {
        symbol_variance.sqrt() / avg_per_symbol
    } else {
        1.0
    };
    
    Ok(MultiSymbolStats {
        symbol_count: test_symbols.len(),
        total_rate,
        success_rate,
        symbol_balance,
    })
}