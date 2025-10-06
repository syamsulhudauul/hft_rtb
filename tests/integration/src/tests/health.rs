use anyhow::Result;
use std::time::Duration;
use tracing::{info, warn};

use crate::utils::{check_health, wait_for_service, TestConfig};
use crate::TestResults;

pub async fn run_health_tests(config: &TestConfig, results: &mut TestResults) -> Result<()> {
    info!("🏥 Running Health Check Tests");

    // Test 1: Service Availability
    match wait_for_service(&config.gateway_url(), Duration::from_secs(30)).await {
        Ok(_) => results.pass("Service Availability"),
        Err(e) => {
            results.fail("Service Availability", &e.to_string());
            return Ok(()); // Skip remaining health tests if service is not available
        }
    }

    // Test 2: Health Endpoint Response
    match check_health(config).await {
        Ok(health) => {
            results.pass("Health Endpoint Response");
            info!("Health status: {}, uptime: {}s", health.status, health.uptime_seconds);
            
            // Test 3: Health Status Validation
            if health.status == "healthy" || health.status == "ok" {
                results.pass("Health Status Validation");
            } else {
                results.fail("Health Status Validation", &format!("Unexpected status: {}", health.status));
            }
            
            // Test 4: Uptime Check
            if health.uptime_seconds > 0 {
                results.pass("Uptime Check");
            } else {
                results.fail("Uptime Check", "Uptime should be greater than 0");
            }
        }
        Err(e) => {
            results.fail("Health Endpoint Response", &e.to_string());
            results.skip("Health Status Validation", "Health endpoint failed");
            results.skip("Uptime Check", "Health endpoint failed");
        }
    }

    // Test 5: Metrics Endpoint Availability
    let client = reqwest::Client::new();
    match client.get(&config.metrics_url()).send().await {
        Ok(response) if response.status().is_success() => {
            results.pass("Metrics Endpoint Availability");
        }
        Ok(response) => {
            results.fail("Metrics Endpoint Availability", &format!("Status: {}", response.status()));
        }
        Err(e) => {
            results.fail("Metrics Endpoint Availability", &e.to_string());
        }
    }

    // Test 6: Admin gRPC Endpoint Check
    let grpc_endpoint = config.grpc_endpoint();
    match tokio::net::TcpStream::connect(&grpc_endpoint).await {
        Ok(_) => results.pass("Admin gRPC Endpoint Connectivity"),
        Err(e) => {
            warn!("gRPC endpoint not available: {}", e);
            results.skip("Admin gRPC Endpoint Connectivity", "gRPC endpoint not reachable");
        }
    }

    // Test 7: Response Time Check
    let start = std::time::Instant::now();
    match check_health(config).await {
        Ok(_) => {
            let response_time = start.elapsed();
            if response_time < Duration::from_millis(100) {
                results.pass("Health Response Time (<100ms)");
            } else if response_time < Duration::from_millis(500) {
                results.pass("Health Response Time (<500ms)");
                warn!("Health response time: {:?} (acceptable but not optimal)", response_time);
            } else {
                results.fail("Health Response Time", &format!("Too slow: {:?}", response_time));
            }
        }
        Err(e) => {
            results.fail("Health Response Time", &e.to_string());
        }
    }

    // Test 8: Multiple Concurrent Health Checks
    let mut handles = vec![];
    for i in 0..5 {
        let config = config.clone();
        let handle = tokio::spawn(async move {
            let start = std::time::Instant::now();
            let result = check_health(&config).await;
            (i, result, start.elapsed())
        });
        handles.push(handle);
    }

    let mut concurrent_success = 0;
    let mut max_response_time = Duration::from_secs(0);

    for handle in handles {
        match handle.await {
            Ok((id, Ok(_), duration)) => {
                concurrent_success += 1;
                max_response_time = max_response_time.max(duration);
                info!("Concurrent health check {} completed in {:?}", id, duration);
            }
            Ok((id, Err(e), _)) => {
                warn!("Concurrent health check {} failed: {}", id, e);
            }
            Err(e) => {
                warn!("Concurrent health check task failed: {}", e);
            }
        }
    }

    if concurrent_success >= 4 {
        results.pass("Concurrent Health Checks (4/5 success)");
    } else {
        results.fail("Concurrent Health Checks", &format!("Only {}/5 succeeded", concurrent_success));
    }

    if max_response_time < Duration::from_millis(200) {
        results.pass("Concurrent Health Check Performance");
    } else {
        results.fail("Concurrent Health Check Performance", &format!("Max response time: {:?}", max_response_time));
    }

    info!("✅ Health check tests completed");
    Ok(())
}