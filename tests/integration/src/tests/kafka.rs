use anyhow::Result;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::producer::{FutureProducer, FutureRecord, Producer};
use rdkafka::Message;
use std::time::Duration;
use tracing::{info, warn};
use uuid::Uuid;

use crate::utils::{generate_test_symbols, TestConfig, TestMessage};
use crate::TestResults;

pub async fn run_kafka_tests(config: &TestConfig, results: &mut TestResults) -> Result<()> {
    info!("🚀 Running Kafka Integration Tests");

    // Test 1: Kafka Connectivity
    let producer_config = ClientConfig::new()
        .set("bootstrap.servers", &config.kafka_brokers)
        .set("message.timeout.ms", "5000")
        .set("queue.buffering.max.ms", "0") // Minimize latency
        .set("batch.num.messages", "1")
        .clone();

    let producer: FutureProducer = match producer_config.create() {
        Ok(p) => {
            results.pass("Kafka Producer Creation");
            p
        }
        Err(e) => {
            results.fail("Kafka Producer Creation", &e.to_string());
            results.skip("All Kafka Tests", "Producer creation failed");
            return Ok(());
        }
    };

    // Test 2: Consumer Creation
    let consumer_config = ClientConfig::new()
        .set("bootstrap.servers", &config.kafka_brokers)
        .set("group.id", &format!("integration-test-{}", Uuid::new_v4()))
        .set("auto.offset.reset", "latest")
        .set("enable.auto.commit", "false")
        .clone();

    let consumer: StreamConsumer = match consumer_config.create() {
        Ok(c) => {
            results.pass("Kafka Consumer Creation");
            c
        }
        Err(e) => {
            results.fail("Kafka Consumer Creation", &e.to_string());
            results.skip("Remaining Kafka Tests", "Consumer creation failed");
            return Ok(());
        }
    };

    // Test 3: Topic Subscription
    let test_topic = "market-data-test";
    match consumer.subscribe(&[test_topic]) {
        Ok(_) => results.pass("Kafka Topic Subscription"),
        Err(e) => {
            results.fail("Kafka Topic Subscription", &e.to_string());
            results.skip("Message Tests", "Topic subscription failed");
            return Ok(());
        }
    }

    // Test 4: Message Production
    let test_symbols = generate_test_symbols();
    let test_message = TestMessage::new(&test_symbols[0], 150.25, 100);
    let tick = test_message.to_tick();
    
    let owned_tick = tick.to_owned_tick();
    let payload = match bincode::serialize(&owned_tick) {
        Ok(data) => data,
        Err(e) => {
            results.fail("Message Serialization", &e.to_string());
            return Ok(());
        }
    };

    let record = FutureRecord::to(test_topic)
        .key(&test_message.symbol)
        .payload(&payload);

    let produce_start = std::time::Instant::now();
    match producer.send(record, Duration::from_secs(5)).await {
        Ok(_) => {
            let produce_time = produce_start.elapsed();
            results.pass("Message Production");
            info!("Message produced in {:?}", produce_time);
            
            if produce_time < Duration::from_millis(10) {
                results.pass("Production Latency (<10ms)");
            } else if produce_time < Duration::from_millis(50) {
                results.pass("Production Latency (<50ms)");
                warn!("Production latency: {:?} (acceptable but not optimal)", produce_time);
            } else {
                results.fail("Production Latency", &format!("Too slow: {:?}", produce_time));
            }
        }
        Err(e) => {
            results.fail("Message Production", &format!("{:?}", e));
            results.skip("Production Latency", "Production failed");
            results.skip("Message Consumption", "No message to consume");
            return Ok(());
        }
    }

    // Test 5: Message Consumption
    info!("Waiting for message consumption...");
    let consume_timeout = Duration::from_secs(10);
    let consume_start = std::time::Instant::now();
    
    match tokio::time::timeout(consume_timeout, consumer.recv()).await {
        Ok(Ok(message)) => {
            let consume_time = consume_start.elapsed();
            results.pass("Message Consumption");
            info!("Message consumed in {:?}", consume_time);
            
            // Test 6: Message Content Validation
            if let Some(payload) = message.payload() {
                match bincode::deserialize::<common::OwnedTick>(payload) {
                    Ok(received_tick) => {
                        results.pass("Message Deserialization");
                        
                        if received_tick.symbol == tick.symbol &&
                           received_tick.price == tick.price &&
                           received_tick.size == tick.size {
                            results.pass("Message Content Validation");
                        } else {
                            results.fail("Message Content Validation", "Message content mismatch");
                        }
                    }
                    Err(e) => {
                        results.fail("Message Deserialization", &e.to_string());
                        results.skip("Message Content Validation", "Deserialization failed");
                    }
                }
            } else {
                results.fail("Message Content Validation", "No payload in received message");
            }
            
            // Test 7: End-to-End Latency
            if consume_time < Duration::from_millis(50) {
                results.pass("End-to-End Latency (<50ms)");
            } else if consume_time < Duration::from_millis(200) {
                results.pass("End-to-End Latency (<200ms)");
                warn!("End-to-end latency: {:?} (acceptable but not optimal)", consume_time);
            } else {
                results.fail("End-to-End Latency", &format!("Too slow: {:?}", consume_time));
            }
        }
        Ok(Err(e)) => {
            results.fail("Message Consumption", &e.to_string());
            results.skip("Message Deserialization", "Consumption failed");
            results.skip("Message Content Validation", "Consumption failed");
            results.skip("End-to-End Latency", "Consumption failed");
        }
        Err(_) => {
            results.fail("Message Consumption", "Timeout waiting for message");
            results.skip("Message Deserialization", "Consumption timeout");
            results.skip("Message Content Validation", "Consumption timeout");
            results.skip("End-to-End Latency", "Consumption timeout");
        }
    }

    // Test 8: Batch Message Production
    info!("Testing batch message production...");
    let batch_size = 10;
    let mut batch_futures = Vec::new();
    
    let batch_start = std::time::Instant::now();
    for i in 0..batch_size {
        let symbol = &test_symbols[i % test_symbols.len()];
        let test_msg = TestMessage::new(symbol, 100.0 + i as f64, 50 + i as u64);
        let tick = test_msg.to_tick();
        let owned_tick = tick.to_owned_tick();
        
        if let Ok(payload) = bincode::serialize(&owned_tick) {
            let payload_ref: &'static [u8] = Box::leak(payload.into_boxed_slice());
            let record = FutureRecord::to(test_topic)
                .key(symbol)
                .payload(payload_ref);
            
            batch_futures.push(producer.send(record, Duration::from_secs(5)));
        }
    }
    
    let batch_results = futures::future::join_all(batch_futures).await;
    let successful_sends = batch_results.iter().filter(|r| r.is_ok()).count();
    let batch_time = batch_start.elapsed();
    
    if successful_sends >= batch_size * 8 / 10 { // 80% success rate
        results.pass("Batch Message Production");
        info!("Batch production: {}/{} messages in {:?}", successful_sends, batch_size, batch_time);
        
        let throughput = successful_sends as f64 / batch_time.as_secs_f64();
        if throughput > 100.0 {
            results.pass("Batch Production Throughput (>100 msg/s)");
        } else if throughput > 50.0 {
            results.pass("Batch Production Throughput (>50 msg/s)");
            warn!("Batch throughput: {:.1} msg/s (acceptable but not optimal)", throughput);
        } else {
            results.fail("Batch Production Throughput", &format!("Too low: {:.1} msg/s", throughput));
        }
    } else {
        results.fail("Batch Message Production", &format!("Only {}/{} messages succeeded", successful_sends, batch_size));
        results.skip("Batch Production Throughput", "Batch production failed");
    }

    // Test 9: Multiple Symbol Support
    info!("Testing multiple symbol support...");
    let mut symbol_success = 0;
    
    for symbol in &test_symbols[0..3] { // Test first 3 symbols
        let test_msg = TestMessage::new(symbol, 200.0, 75);
        let tick = test_msg.to_tick();
        let owned_tick = tick.to_owned_tick();
        
        if let Ok(payload) = bincode::serialize(&owned_tick) {
            let record = FutureRecord::to(test_topic)
                .key(symbol)
                .payload(&payload);
            
            match producer.send(record, Duration::from_secs(5)).await {
                Ok(_) => {
                    symbol_success += 1;
                    info!("Successfully sent message for symbol: {}", symbol);
                }
                Err(e) => {
                    warn!("Failed to send message for symbol {}: {:?}", symbol, e);
                }
            }
        }
    }
    
    if symbol_success >= 2 {
        results.pass("Multiple Symbol Support");
    } else {
        results.fail("Multiple Symbol Support", &format!("Only {}/3 symbols succeeded", symbol_success));
    }

    // Test 10: Producer Flush
    match producer.flush(Duration::from_secs(5)) {
        Ok(_) => results.pass("Producer Flush"),
        Err(e) => results.fail("Producer Flush", &e.to_string()),
    }

    info!("✅ Kafka integration tests completed");
    Ok(())
}