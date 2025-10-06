use anyhow::Result;
use std::time::Duration;
use tracing::{info, warn};

use crate::utils::{get_metrics, parse_prometheus_metrics, TestConfig};
use crate::TestResults;

pub async fn run_metrics_tests(config: &TestConfig, results: &mut TestResults) -> Result<()> {
    info!("📊 Running Metrics Tests");

    // Test 1: Metrics Endpoint Accessibility
    match get_metrics(config).await {
        Ok(metrics_text) => {
            results.pass("Metrics Endpoint Accessibility");
            
            // Test 2: Metrics Format Validation
            if metrics_text.contains("# HELP") || metrics_text.contains("# TYPE") {
                results.pass("Prometheus Metrics Format");
            } else {
                results.fail("Prometheus Metrics Format", "Missing Prometheus format headers");
            }

            // Test 3: Essential Metrics Presence
            let essential_metrics = [
                "process_cpu_seconds_total",
                "process_resident_memory_bytes",
                "http_requests_total",
            ];

            let mut found_metrics = 0;
            for metric in &essential_metrics {
                if metrics_text.contains(metric) {
                    found_metrics += 1;
                }
            }

            if found_metrics >= 2 {
                results.pass("Essential Metrics Presence");
            } else {
                results.fail("Essential Metrics Presence", &format!("Found only {}/{} essential metrics", found_metrics, essential_metrics.len()));
            }

            // Test 4: Custom HFT Metrics
            let hft_metrics = [
                "latency",
                "throughput",
                "processed",
                "connections",
                "orders",
            ];

            let mut found_hft_metrics = 0;
            for metric in &hft_metrics {
                if metrics_text.to_lowercase().contains(metric) {
                    found_hft_metrics += 1;
                }
            }

            if found_hft_metrics >= 2 {
                results.pass("HFT-Specific Metrics");
                info!("Found {}/{} HFT-specific metrics", found_hft_metrics, hft_metrics.len());
            } else {
                results.fail("HFT-Specific Metrics", &format!("Found only {}/{} HFT metrics", found_hft_metrics, hft_metrics.len()));
            }

            // Test 5: Metrics Parsing
            match parse_prometheus_metrics(&metrics_text) {
                Ok(snapshot) => {
                    results.pass("Metrics Parsing");
                    info!("Parsed metrics snapshot: {:?}", snapshot);

                    // Test 6: Metrics Value Validation
                    let mut valid_values = 0;
                    let mut total_values = 0;

                    if let Some(latency) = snapshot.latency_p50 {
                        total_values += 1;
                        if latency >= 0.0 && latency < 1000.0 { // Reasonable latency range
                            valid_values += 1;
                        }
                    }

                    if let Some(throughput) = snapshot.throughput_per_second {
                        total_values += 1;
                        if throughput >= 0.0 {
                            valid_values += 1;
                        }
                    }

                    if let Some(connections) = snapshot.active_connections {
                        total_values += 1;
                        if connections < 10000 { // Reasonable connection limit
                            valid_values += 1;
                        }
                    }

                    if total_values > 0 && valid_values == total_values {
                        results.pass("Metrics Value Validation");
                    } else if total_values > 0 {
                        results.fail("Metrics Value Validation", &format!("{}/{} values are valid", valid_values, total_values));
                    } else {
                        results.skip("Metrics Value Validation", "No parseable metric values found");
                    }
                }
                Err(e) => {
                    results.fail("Metrics Parsing", &e.to_string());
                    results.skip("Metrics Value Validation", "Parsing failed");
                }
            }
        }
        Err(e) => {
            results.fail("Metrics Endpoint Accessibility", &e.to_string());
            results.skip("Prometheus Metrics Format", "Endpoint not accessible");
            results.skip("Essential Metrics Presence", "Endpoint not accessible");
            results.skip("HFT-Specific Metrics", "Endpoint not accessible");
            results.skip("Metrics Parsing", "Endpoint not accessible");
            results.skip("Metrics Value Validation", "Endpoint not accessible");
            return Ok(());
        }
    }

    // Test 7: Metrics Response Time
    let start = std::time::Instant::now();
    match get_metrics(config).await {
        Ok(_) => {
            let response_time = start.elapsed();
            if response_time < Duration::from_millis(50) {
                results.pass("Metrics Response Time (<50ms)");
            } else if response_time < Duration::from_millis(200) {
                results.pass("Metrics Response Time (<200ms)");
                warn!("Metrics response time: {:?} (acceptable but not optimal)", response_time);
            } else {
                results.fail("Metrics Response Time", &format!("Too slow: {:?}", response_time));
            }
        }
        Err(e) => {
            results.fail("Metrics Response Time", &e.to_string());
        }
    }

    // Test 8: Metrics Consistency Over Time
    info!("Testing metrics consistency over 5 seconds...");
    let mut snapshots = Vec::new();
    
    for i in 0..5 {
        match get_metrics(config).await {
            Ok(metrics_text) => {
                if let Ok(snapshot) = parse_prometheus_metrics(&metrics_text) {
                    snapshots.push(snapshot);
                }
            }
            Err(e) => {
                warn!("Failed to get metrics in consistency test iteration {}: {}", i, e);
            }
        }
        
        if i < 4 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    if snapshots.len() >= 3 {
        results.pass("Metrics Consistency Collection");
        
        // Check if metrics are updating (not static)
        let mut has_changes = false;
        if snapshots.len() >= 2 {
            let first = &snapshots[0];
            let last = &snapshots[snapshots.len() - 1];
            
            if first.timestamp != last.timestamp {
                has_changes = true;
            }
            
            // Check if any metric values changed (indicating live system)
            if first.processed_messages != last.processed_messages ||
               first.active_connections != last.active_connections {
                has_changes = true;
            }
        }
        
        if has_changes {
            results.pass("Metrics Live Updates");
        } else {
            results.skip("Metrics Live Updates", "No metric changes detected (system might be idle)");
        }
    } else {
        results.fail("Metrics Consistency Collection", &format!("Only collected {}/5 snapshots", snapshots.len()));
        results.skip("Metrics Live Updates", "Insufficient data");
    }

    // Test 9: Concurrent Metrics Access
    let mut handles = vec![];
    for i in 0..3 {
        let config = config.clone();
        let handle = tokio::spawn(async move {
            let start = std::time::Instant::now();
            let result = get_metrics(&config).await;
            (i, result, start.elapsed())
        });
        handles.push(handle);
    }

    let mut concurrent_success = 0;
    for handle in handles {
        match handle.await {
            Ok((id, Ok(_), duration)) => {
                concurrent_success += 1;
                info!("Concurrent metrics request {} completed in {:?}", id, duration);
            }
            Ok((id, Err(e), _)) => {
                warn!("Concurrent metrics request {} failed: {}", id, e);
            }
            Err(e) => {
                warn!("Concurrent metrics task failed: {}", e);
            }
        }
    }

    if concurrent_success >= 2 {
        results.pass("Concurrent Metrics Access");
    } else {
        results.fail("Concurrent Metrics Access", &format!("Only {}/3 succeeded", concurrent_success));
    }

    info!("✅ Metrics tests completed");
    Ok(())
}