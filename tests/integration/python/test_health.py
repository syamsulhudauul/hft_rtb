"""
Health check integration tests for HFT RTB system.
"""

import asyncio
import time
import pytest
import aiohttp
from conftest import assert_response_time

@pytest.mark.health
@pytest.mark.asyncio
async def test_health_endpoint_availability(config, http_session, wait_for_services):
    """Test that health endpoint is available and responds correctly."""
    health_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/health"
    
    start_time = time.time()
    async with http_session.get(health_url) as response:
        duration = time.time() - start_time
        
        assert response.status == 200, f"Health endpoint returned status {response.status}"
        
        data = await response.json()
        assert "status" in data, "Health response missing 'status' field"
        assert data["status"] == "healthy", f"Service status is {data['status']}, expected 'healthy'"
        
        # Response time should be fast for health checks
        assert_response_time(duration, 1.0, "Health check")

@pytest.mark.health
@pytest.mark.asyncio
async def test_health_endpoint_fields(config, http_session, wait_for_services):
    """Test that health endpoint returns expected fields."""
    health_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/health"
    
    async with http_session.get(health_url) as response:
        assert response.status == 200
        data = await response.json()
        
        # Required fields
        required_fields = ["status", "timestamp"]
        for field in required_fields:
            assert field in data, f"Health response missing required field: {field}"
        
        # Optional but expected fields
        expected_fields = ["uptime", "version", "build_time"]
        for field in expected_fields:
            if field in data:
                assert data[field] is not None, f"Field {field} should not be null"
        
        # Validate timestamp format
        assert isinstance(data["timestamp"], (int, str)), "Timestamp should be int or string"
        
        # If uptime is present, it should be positive
        if "uptime" in data:
            assert data["uptime"] >= 0, "Uptime should be non-negative"

@pytest.mark.health
@pytest.mark.asyncio
async def test_health_endpoint_consistency(config, http_session, wait_for_services):
    """Test that health endpoint returns consistent responses."""
    health_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/health"
    
    responses = []
    for i in range(5):
        async with http_session.get(health_url) as response:
            assert response.status == 200
            data = await response.json()
            responses.append(data)
        
        if i < 4:  # Don't sleep after the last request
            await asyncio.sleep(0.5)
    
    # All responses should have the same status
    statuses = [r["status"] for r in responses]
    assert all(status == "healthy" for status in statuses), f"Inconsistent statuses: {statuses}"
    
    # Uptime should be increasing (if present)
    uptimes = [r.get("uptime") for r in responses if "uptime" in r]
    if len(uptimes) > 1:
        for i in range(1, len(uptimes)):
            if uptimes[i] is not None and uptimes[i-1] is not None:
                assert uptimes[i] >= uptimes[i-1], f"Uptime decreased: {uptimes[i-1]} -> {uptimes[i]}"

@pytest.mark.health
@pytest.mark.asyncio
async def test_health_endpoint_concurrent_requests(config, http_session, wait_for_services):
    """Test health endpoint under concurrent load."""
    health_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/health"
    
    async def make_health_request():
        start_time = time.time()
        async with http_session.get(health_url) as response:
            duration = time.time() - start_time
            return response.status, duration
    
    # Make 20 concurrent requests
    tasks = [make_health_request() for _ in range(20)]
    results = await asyncio.gather(*tasks)
    
    # All requests should succeed
    statuses = [result[0] for result in results]
    durations = [result[1] for result in results]
    
    success_count = sum(1 for status in statuses if status == 200)
    assert success_count == 20, f"Only {success_count}/20 health checks succeeded"
    
    # All requests should be reasonably fast
    max_duration = max(durations)
    avg_duration = sum(durations) / len(durations)
    
    assert max_duration < 2.0, f"Slowest health check took {max_duration:.3f}s"
    assert avg_duration < 0.5, f"Average health check took {avg_duration:.3f}s"

@pytest.mark.health
@pytest.mark.asyncio
async def test_metrics_endpoint_availability(config, http_session, wait_for_services):
    """Test that metrics endpoint is available."""
    metrics_url = f"http://{config['hft_host']}:{config['hft_metrics_port']}/metrics"
    
    start_time = time.time()
    async with http_session.get(metrics_url) as response:
        duration = time.time() - start_time
        
        assert response.status == 200, f"Metrics endpoint returned status {response.status}"
        
        content = await response.text()
        assert len(content) > 0, "Metrics endpoint returned empty content"
        
        # Should contain Prometheus-format metrics
        assert "# HELP" in content or "# TYPE" in content, "Metrics don't appear to be in Prometheus format"
        
        # Response time should be reasonable
        assert_response_time(duration, 2.0, "Metrics fetch")

@pytest.mark.health
@pytest.mark.asyncio
async def test_metrics_endpoint_content(config, http_session, wait_for_services):
    """Test that metrics endpoint returns expected metrics."""
    metrics_url = f"http://{config['hft_host']}:{config['hft_metrics_port']}/metrics"
    
    async with http_session.get(metrics_url) as response:
        assert response.status == 200
        content = await response.text()
        
        # Expected metric families for HFT system
        expected_metrics = [
            "hft_messages_processed_total",
            "hft_latency_seconds",
            "hft_orders_total",
            "process_cpu_seconds_total",
            "process_resident_memory_bytes"
        ]
        
        found_metrics = []
        for metric in expected_metrics:
            if metric in content:
                found_metrics.append(metric)
        
        # At least some core metrics should be present
        assert len(found_metrics) >= 2, f"Only found {len(found_metrics)} expected metrics: {found_metrics}"

@pytest.mark.health
@pytest.mark.asyncio
async def test_service_startup_time(config, test_metrics):
    """Test service startup and readiness time."""
    # This test assumes the service was recently started
    # In a real scenario, you might restart the service and measure startup time
    
    health_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/health"
    
    test_metrics.start_timer("service_ready")
    
    max_attempts = 30
    for attempt in range(max_attempts):
        try:
            async with aiohttp.ClientSession() as session:
                async with session.get(health_url, timeout=aiohttp.ClientTimeout(total=2)) as response:
                    if response.status == 200:
                        data = await response.json()
                        if data.get("status") == "healthy":
                            ready_time = test_metrics.end_timer("service_ready")
                            test_metrics.add_metric("startup_attempts", attempt + 1)
                            
                            # Service should be ready within reasonable time
                            assert ready_time < 60, f"Service took {ready_time:.1f}s to become ready"
                            assert attempt < 20, f"Service required {attempt + 1} attempts to become ready"
                            
                            return
        except Exception:
            pass
        
        await asyncio.sleep(1)
    
    pytest.fail(f"Service failed to become ready within {max_attempts} attempts")

@pytest.mark.health
@pytest.mark.asyncio
async def test_health_during_load(config, http_session, kafka_producer, test_symbols, wait_for_services):
    """Test that health endpoint remains responsive during load."""
    health_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/health"
    
    # Start background load
    async def generate_load():
        for i in range(100):
            symbol = test_symbols[i % len(test_symbols)]
            tick_data = {
                "symbol": symbol,
                "price": 100.0 + i,
                "volume": 1000,
                "timestamp": int(time.time() * 1000)
            }
            
            try:
                kafka_producer.send("market-data", value=tick_data, key=symbol)
            except Exception:
                pass  # Ignore send failures for this test
            
            await asyncio.sleep(0.01)  # 100 messages per second
    
    # Start load generation
    load_task = asyncio.create_task(generate_load())
    
    # Check health multiple times during load
    health_checks = []
    for _ in range(10):
        start_time = time.time()
        try:
            async with http_session.get(health_url) as response:
                duration = time.time() - start_time
                health_checks.append((response.status, duration))
        except Exception as e:
            health_checks.append((0, float('inf')))
        
        await asyncio.sleep(0.5)
    
    # Wait for load to complete
    await load_task
    
    # Analyze health check results
    successful_checks = sum(1 for status, _ in health_checks if status == 200)
    avg_duration = sum(duration for _, duration in health_checks if duration != float('inf')) / len(health_checks)
    
    assert successful_checks >= 8, f"Only {successful_checks}/10 health checks succeeded during load"
    assert avg_duration < 1.0, f"Average health check duration {avg_duration:.3f}s too high during load"

@pytest.mark.health
@pytest.mark.slow
@pytest.mark.asyncio
async def test_health_endpoint_reliability(config, http_session, wait_for_services):
    """Test health endpoint reliability over extended period."""
    health_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/health"
    
    test_duration = 60  # 1 minute
    check_interval = 2  # Every 2 seconds
    
    start_time = time.time()
    results = []
    
    while time.time() - start_time < test_duration:
        check_start = time.time()
        try:
            async with http_session.get(health_url) as response:
                duration = time.time() - check_start
                results.append({
                    "timestamp": check_start,
                    "status": response.status,
                    "duration": duration,
                    "success": response.status == 200
                })
        except Exception as e:
            results.append({
                "timestamp": check_start,
                "status": 0,
                "duration": float('inf'),
                "success": False,
                "error": str(e)
            })
        
        await asyncio.sleep(check_interval)
    
    # Analyze results
    total_checks = len(results)
    successful_checks = sum(1 for r in results if r["success"])
    success_rate = successful_checks / total_checks if total_checks > 0 else 0
    
    valid_durations = [r["duration"] for r in results if r["duration"] != float('inf')]
    avg_duration = sum(valid_durations) / len(valid_durations) if valid_durations else float('inf')
    max_duration = max(valid_durations) if valid_durations else float('inf')
    
    # Assertions
    assert success_rate >= 0.95, f"Health check success rate {success_rate:.2%} below 95%"
    assert avg_duration < 0.5, f"Average health check duration {avg_duration:.3f}s too high"
    assert max_duration < 2.0, f"Maximum health check duration {max_duration:.3f}s too high"
    assert total_checks >= 25, f"Only {total_checks} health checks performed in {test_duration}s"