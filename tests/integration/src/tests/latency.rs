use anyhow::Result;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::Message;
use std::time::{Duration, Instant};
use tracing::{info, warn};
use uuid::Uuid;

use crate::utils::{calculate_latency_stats, generate_test_symbols, TestConfig, TestMessage};
use crate::TestResults;

pub async fn run_latency_tests(config: &TestConfig, results: &mut TestResults) -> Result<()> {
    info!("⚡ Running Latency Performance Tests");

    // Setup Kafka producer with low-latency configuration
    let producer_config = ClientConfig::new()
        .set("bootstrap.servers", &config.kafka_brokers)
        .set("message.timeout.ms", "1000")
        .set("queue.buffering.max.ms", "0") // No buffering for minimum latency
        .set("batch.num.messages", "1") // Send immediately
        .set("linger.ms", "0") // No lingering
        .set("acks", "1") // Wait for leader acknowledgment only
        .set("retries", "0") // No retries for latency testing
        .clone();

    let producer: FutureProducer = match producer_config.create() {
        Ok(p) => {
            results.pass("Low-Latency Producer Setup");
            p
        }
        Err(e) => {
            results.fail("Low-Latency Producer Setup", &e.to_string());
            results.skip("All Latency Tests", "Producer setup failed");
            return Ok(());
        }
    };

    // Setup Kafka consumer with low-latency configuration
    let consumer_config = ClientConfig::new()
        .set("bootstrap.servers", &config.kafka_brokers)
        .set("group.id", &format!("latency-test-{}", Uuid::new_v4()))
        .set("auto.offset.reset", "latest")
        .set("enable.auto.commit", "false")
        .set("fetch.min.bytes", "1") // Fetch immediately
        .set("fetch.wait.max.ms", "1") // Minimal wait time
        .clone();

    let consumer: StreamConsumer = match consumer_config.create() {
        Ok(c) => {
            results.pass("Low-Latency Consumer Setup");
            c
        }
        Err(e) => {
            results.fail("Low-Latency Consumer Setup", &e.to_string());
            results.skip("Remaining Latency Tests", "Consumer setup failed");
            return Ok(());
        }
    };

    let test_topic = "latency-test";
    if let Err(e) = consumer.subscribe(&[test_topic]) {
        results.fail("Latency Test Topic Subscription", &e.to_string());
        return Ok(());
    }

    // Test 1: Single Message Latency
    info!("Testing single message latency...");
    let test_symbols = generate_test_symbols();
    let test_message = TestMessage::new(&test_symbols[0], 100.0, 50);
    let tick = test_message.to_tick();
    
    let payload = bincode::serialize(&tick).unwrap();
    let record = FutureRecord::to(test_topic)
        .key(&test_message.symbol)
        .payload(&payload);

    let send_start = Instant::now();
    match producer.send(record, Duration::from_millis(100)).await {
        Ok(_) => {
            let send_latency = send_start.elapsed();
            results.pass("Single Message Send");
            
            if send_latency < Duration::from_micros(500) {
                results.pass("Ultra-Low Send Latency (<500μs)");
            } else if send_latency < Duration::from_millis(1) {
                results.pass("Low Send Latency (<1ms)");
            } else if send_latency < Duration::from_millis(5) {
                results.pass("Acceptable Send Latency (<5ms)");
                warn!("Send latency: {:?} (acceptable but not optimal for HFT)", send_latency);
            } else {
                results.fail("Send Latency", &format!("Too high: {:?}", send_latency));
            }
        }
        Err(e) => {
            results.fail("Single Message Send", &e.to_string());
            results.skip("Send Latency Tests", "Send failed");
        }
    }

    // Test 2: Round-Trip Latency
    info!("Testing round-trip latency...");
    let round_trip_start = Instant::now();
    
    // Send message
    let test_message = TestMessage::new(&test_symbols[1], 101.0, 51);
    let tick = test_message.to_tick();
    let payload = bincode::serialize(&tick).unwrap();
    let record = FutureRecord::to(test_topic)
        .key(&test_message.symbol)
        .payload(&payload);

    if let Ok(_) = producer.send(record, Duration::from_millis(100)).await {
        // Receive message
        match tokio::time::timeout(Duration::from_secs(5), consumer.recv()).await {
            Ok(Ok(_)) => {
                let round_trip_latency = round_trip_start.elapsed();
                results.pass("Round-Trip Message");
                
                if round_trip_latency < Duration::from_millis(1) {
                    results.pass("Ultra-Low Round-Trip Latency (<1ms)");
                } else if round_trip_latency < Duration::from_millis(5) {
                    results.pass("Low Round-Trip Latency (<5ms)");
                } else if round_trip_latency < Duration::from_millis(20) {
                    results.pass("Acceptable Round-Trip Latency (<20ms)");
                    warn!("Round-trip latency: {:?} (acceptable but not optimal for HFT)", round_trip_latency);
                } else {
                    results.fail("Round-Trip Latency", &format!("Too high: {:?}", round_trip_latency));
                }
            }
            Ok(Err(e)) => {
                results.fail("Round-Trip Message", &e.to_string());
                results.skip("Round-Trip Latency", "Receive failed");
            }
            Err(_) => {
                results.fail("Round-Trip Message", "Timeout");
                results.skip("Round-Trip Latency", "Timeout");
            }
        }
    } else {
        results.skip("Round-Trip Tests", "Send failed");
    }

    // Test 3: Batch Latency Performance
    info!("Testing batch latency performance...");
    let batch_size = 100;
    let mut send_latencies = Vec::new();
    
    for i in 0..batch_size {
        let symbol = &test_symbols[i % test_symbols.len()];
        let test_message = TestMessage::new(symbol, 100.0 + i as f64, 50 + i);
        let tick = test_message.to_tick();
        let payload = bincode::serialize(&tick).unwrap();
        
        let record = FutureRecord::to(test_topic)
            .key(symbol)
            .payload(&payload);
        
        let send_start = Instant::now();
        match producer.send(record, Duration::from_millis(50)).await {
            Ok(_) => {
                send_latencies.push(send_start.elapsed());
            }
            Err(e) => {
                warn!("Batch send {} failed: {}", i, e);
            }
        }
    }
    
    if send_latencies.len() >= batch_size * 8 / 10 { // 80% success rate
        results.pass("Batch Latency Test Completion");
        
        let (avg, p50, p95, p99) = calculate_latency_stats(&send_latencies);
        info!("Batch latency stats - Avg: {:.2}ms, P50: {:.2}ms, P95: {:.2}ms, P99: {:.2}ms", avg, p50, p95, p99);
        
        if p99 < 5.0 {
            results.pass("Batch P99 Latency (<5ms)");
        } else if p99 < 10.0 {
            results.pass("Batch P99 Latency (<10ms)");
            warn!("P99 latency: {:.2}ms (acceptable but not optimal)", p99);
        } else {
            results.fail("Batch P99 Latency", &format!("Too high: {:.2}ms", p99));
        }
        
        if p95 < 2.0 {
            results.pass("Batch P95 Latency (<2ms)");
        } else if p95 < 5.0 {
            results.pass("Batch P95 Latency (<5ms)");
        } else {
            results.fail("Batch P95 Latency", &format!("Too high: {:.2}ms", p95));
        }
        
        if avg < 1.0 {
            results.pass("Batch Average Latency (<1ms)");
        } else if avg < 3.0 {
            results.pass("Batch Average Latency (<3ms)");
        } else {
            results.fail("Batch Average Latency", &format!("Too high: {:.2}ms", avg));
        }
    } else {
        results.fail("Batch Latency Test Completion", &format!("Only {}/{} messages succeeded", send_latencies.len(), batch_size));
        results.skip("Batch Latency Statistics", "Insufficient data");
    }

    // Test 4: Sustained Latency Performance
    info!("Testing sustained latency performance over 10 seconds...");
    let test_duration = Duration::from_secs(10);
    let target_rate = 100; // messages per second
    let interval = Duration::from_millis(1000 / target_rate);
    
    let mut sustained_latencies = Vec::new();
    let sustained_start = Instant::now();
    let mut message_count = 0;
    
    while sustained_start.elapsed() < test_duration {
        let symbol = &test_symbols[message_count % test_symbols.len()];
        let test_message = TestMessage::new(symbol, 100.0 + message_count as f64, 50);
        let tick = test_message.to_tick();
        let payload = bincode::serialize(&tick).unwrap();
        
        let record = FutureRecord::to(test_topic)
            .key(symbol)
            .payload(&payload);
        
        let send_start = Instant::now();
        match producer.send(record, Duration::from_millis(20)).await {
            Ok(_) => {
                sustained_latencies.push(send_start.elapsed());
            }
            Err(e) => {
                warn!("Sustained test message {} failed: {}", message_count, e);
            }
        }
        
        message_count += 1;
        tokio::time::sleep(interval).await;
    }
    
    let actual_duration = sustained_start.elapsed();
    let actual_rate = sustained_latencies.len() as f64 / actual_duration.as_secs_f64();
    
    if sustained_latencies.len() >= target_rate * 8 { // 80% of expected messages
        results.pass("Sustained Latency Test Completion");
        info!("Sustained test: {} messages in {:?} ({:.1} msg/s)", sustained_latencies.len(), actual_duration, actual_rate);
        
        let (avg, p50, p95, p99) = calculate_latency_stats(&sustained_latencies);
        info!("Sustained latency stats - Avg: {:.2}ms, P50: {:.2}ms, P95: {:.2}ms, P99: {:.2}ms", avg, p50, p95, p99);
        
        // More stringent requirements for sustained performance
        if p99 < 10.0 {
            results.pass("Sustained P99 Latency (<10ms)");
        } else {
            results.fail("Sustained P99 Latency", &format!("Too high: {:.2}ms", p99));
        }
        
        if avg < 5.0 {
            results.pass("Sustained Average Latency (<5ms)");
        } else {
            results.fail("Sustained Average Latency", &format!("Too high: {:.2}ms", avg));
        }
        
        // Check for latency stability (low variance)
        let variance = sustained_latencies.iter()
            .map(|d| {
                let diff = d.as_nanos() as f64 / 1_000_000.0 - avg;
                diff * diff
            })
            .sum::<f64>() / sustained_latencies.len() as f64;
        let std_dev = variance.sqrt();
        
        if std_dev < 2.0 {
            results.pass("Latency Stability (σ < 2ms)");
        } else if std_dev < 5.0 {
            results.pass("Latency Stability (σ < 5ms)");
            warn!("Latency standard deviation: {:.2}ms (acceptable but not optimal)", std_dev);
        } else {
            results.fail("Latency Stability", &format!("High variance: σ = {:.2}ms", std_dev));
        }
    } else {
        results.fail("Sustained Latency Test Completion", &format!("Only {} messages in {:?}", sustained_latencies.len(), actual_duration));
        results.skip("Sustained Latency Statistics", "Insufficient data");
    }

    // Test 5: Latency Under Load
    info!("Testing latency under concurrent load...");
    let concurrent_producers = 5;
    let messages_per_producer = 20;
    
    let mut handles = Vec::new();
    for producer_id in 0..concurrent_producers {
        let config = config.clone();
        let symbols = test_symbols.clone();
        
        let handle = tokio::spawn(async move {
            let producer_config = ClientConfig::new()
                .set("bootstrap.servers", &config.kafka_brokers)
                .set("message.timeout.ms", "1000")
                .set("queue.buffering.max.ms", "0")
                .set("batch.num.messages", "1")
                .set("linger.ms", "0")
                .clone();
            
            let producer: FutureProducer = producer_config.create().unwrap();
            let mut latencies = Vec::new();
            
            for i in 0..messages_per_producer {
                let symbol = &symbols[i % symbols.len()];
                let test_message = TestMessage::new(symbol, 100.0 + i as f64, 50);
                let tick = test_message.to_tick();
                let payload = bincode::serialize(&tick).unwrap();
                
                let record = FutureRecord::to("latency-test")
                    .key(symbol)
                    .payload(&payload);
                
                let send_start = Instant::now();
                if let Ok(_) = producer.send(record, Duration::from_millis(50)).await {
                    latencies.push(send_start.elapsed());
                }
            }
            
            (producer_id, latencies)
        });
        
        handles.push(handle);
    }
    
    let mut all_load_latencies = Vec::new();
    let mut successful_producers = 0;
    
    for handle in handles {
        match handle.await {
            Ok((producer_id, latencies)) => {
                if !latencies.is_empty() {
                    successful_producers += 1;
                    all_load_latencies.extend(latencies);
                    info!("Producer {} completed with {} messages", producer_id, latencies.len());
                }
            }
            Err(e) => {
                warn!("Producer task failed: {}", e);
            }
        }
    }
    
    if successful_producers >= concurrent_producers * 4 / 5 { // 80% success
        results.pass("Concurrent Load Test Completion");
        
        if !all_load_latencies.is_empty() {
            let (avg, p50, p95, p99) = calculate_latency_stats(&all_load_latencies);
            info!("Load test latency stats - Avg: {:.2}ms, P50: {:.2}ms, P95: {:.2}ms, P99: {:.2}ms", avg, p50, p95, p99);
            
            // Under load, we expect slightly higher latencies
            if p99 < 20.0 {
                results.pass("Load Test P99 Latency (<20ms)");
            } else {
                results.fail("Load Test P99 Latency", &format!("Too high: {:.2}ms", p99));
            }
            
            if avg < 10.0 {
                results.pass("Load Test Average Latency (<10ms)");
            } else {
                results.fail("Load Test Average Latency", &format!("Too high: {:.2}ms", avg));
            }
        } else {
            results.skip("Load Test Latency Statistics", "No latency data collected");
        }
    } else {
        results.fail("Concurrent Load Test Completion", &format!("Only {}/{} producers succeeded", successful_producers, concurrent_producers));
        results.skip("Load Test Latency Statistics", "Insufficient producers");
    }

    info!("✅ Latency performance tests completed");
    Ok(())
}