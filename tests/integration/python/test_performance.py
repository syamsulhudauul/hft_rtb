"""
Performance integration tests for HFT RTB system.
"""

import asyncio
import json
import time
import statistics
import uuid
from typing import List, Dict, Any, Tuple
import pytest
import aiohttp
from kafka import KafkaProducer
from conftest import assert_response_time, assert_success_rate, assert_latency_percentile

@pytest.mark.performance
@pytest.mark.asyncio
async def test_system_baseline_performance(config, http_session, wait_for_services):
    """Test baseline system performance metrics."""
    # Get initial metrics
    metrics_url = f"http://{config['hft_host']}:{config['hft_metrics_port']}/metrics"
    
    async with http_session.get(metrics_url) as response:
        assert response.status == 200
        initial_metrics = await response.text()
    
    # Perform some basic operations
    health_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/health"
    positions_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/api/v1/positions"
    
    # Measure response times for basic operations
    operations = []
    
    for _ in range(10):
        # Health check
        start_time = time.time()
        async with http_session.get(health_url) as response:
            health_duration = time.time() - start_time
            operations.append(("health", health_duration, response.status == 200))
        
        # Positions query
        start_time = time.time()
        async with http_session.get(positions_url) as response:
            positions_duration = time.time() - start_time
            operations.append(("positions", positions_duration, response.status in [200, 404]))
        
        await asyncio.sleep(0.1)
    
    # Analyze baseline performance
    health_times = [duration for op, duration, success in operations if op == "health" and success]
    positions_times = [duration for op, duration, success in operations if op == "positions" and success]
    
    if health_times:
        avg_health_time = statistics.mean(health_times)
        p95_health_time = statistics.quantiles(health_times, n=20)[18] if len(health_times) >= 5 else max(health_times)
        
        assert avg_health_time < 0.1, f"Average health check time {avg_health_time:.3f}s too high"
        assert p95_health_time < 0.2, f"P95 health check time {p95_health_time:.3f}s too high"
    
    if positions_times:
        avg_positions_time = statistics.mean(positions_times)
        p95_positions_time = statistics.quantiles(positions_times, n=20)[18] if len(positions_times) >= 5 else max(positions_times)
        
        assert avg_positions_time < 0.5, f"Average positions query time {avg_positions_time:.3f}s too high"
        assert p95_positions_time < 1.0, f"P95 positions query time {p95_positions_time:.3f}s too high"

@pytest.mark.performance
@pytest.mark.asyncio
async def test_concurrent_request_performance(config, http_session, wait_for_services):
    """Test system performance under concurrent requests."""
    health_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/health"
    
    async def make_concurrent_request():
        start_time = time.time()
        try:
            async with http_session.get(health_url) as response:
                duration = time.time() - start_time
                return response.status, duration, None
        except Exception as e:
            duration = time.time() - start_time
            return 0, duration, str(e)
    
    # Test different concurrency levels
    concurrency_levels = [10, 25, 50]
    
    for concurrency in concurrency_levels:
        print(f"Testing concurrency level: {concurrency}")
        
        # Create concurrent tasks
        tasks = [make_concurrent_request() for _ in range(concurrency)]
        start_time = time.time()
        results = await asyncio.gather(*tasks)
        total_time = time.time() - start_time
        
        # Analyze results
        successful_requests = sum(1 for status, _, _ in results if status == 200)
        durations = [duration for _, duration, _ in results if duration < 10.0]  # Exclude timeouts
        
        if durations:
            avg_duration = statistics.mean(durations)
            p95_duration = statistics.quantiles(durations, n=20)[18] if len(durations) >= 5 else max(durations)
            throughput = successful_requests / total_time
            
            # Performance assertions based on concurrency level
            min_success_rate = 0.95 if concurrency <= 25 else 0.90
            max_avg_duration = 0.5 if concurrency <= 25 else 1.0
            max_p95_duration = 1.0 if concurrency <= 25 else 2.0
            min_throughput = concurrency * 0.8  # Should handle at least 80% of theoretical max
            
            assert_success_rate(successful_requests, concurrency, min_success_rate, f"Concurrent requests (n={concurrency})")
            assert avg_duration < max_avg_duration, f"Average response time {avg_duration:.3f}s too high for concurrency {concurrency}"
            assert p95_duration < max_p95_duration, f"P95 response time {p95_duration:.3f}s too high for concurrency {concurrency}"
            assert throughput >= min_throughput, f"Throughput {throughput:.1f} req/s too low for concurrency {concurrency}"
            
            print(f"Concurrency {concurrency}: {throughput:.1f} req/s, {avg_duration*1000:.1f}ms avg, {successful_requests}/{concurrency} success")

@pytest.mark.performance
@pytest.mark.asyncio
async def test_message_processing_latency(config, kafka_producer, test_symbols):
    """Test end-to-end message processing latency."""
    test_topic = "latency-test"
    
    # Send messages with timestamps and measure processing latency
    latencies = []
    successful_sends = 0
    
    for i in range(50):
        symbol = test_symbols[i % len(test_symbols)]
        send_timestamp = time.time() * 1000  # milliseconds
        
        message = {
            "symbol": symbol,
            "price": 100.0 + i,
            "volume": 1000,
            "send_timestamp": send_timestamp,
            "sequence": i,
            "test_id": str(uuid.uuid4())
        }
        
        try:
            # Measure Kafka send latency
            kafka_start = time.time()
            future = kafka_producer.send(test_topic, value=message, key=symbol)
            future.get(timeout=5)
            kafka_latency = (time.time() - kafka_start) * 1000  # Convert to ms
            
            latencies.append(kafka_latency)
            successful_sends += 1
            
        except Exception as e:
            print(f"Send failed: {e}")
        
        await asyncio.sleep(0.02)  # 50 messages per second
    
    if latencies:
        avg_latency = statistics.mean(latencies)
        p50_latency = statistics.median(latencies)
        p95_latency = statistics.quantiles(latencies, n=20)[18] if len(latencies) >= 5 else max(latencies)
        p99_latency = statistics.quantiles(latencies, n=100)[98] if len(latencies) >= 10 else max(latencies)
        
        # Latency assertions for HFT system
        assert avg_latency < 10.0, f"Average message latency {avg_latency:.2f}ms too high for HFT"
        assert p50_latency < 5.0, f"P50 message latency {p50_latency:.2f}ms too high for HFT"
        assert p95_latency < 20.0, f"P95 message latency {p95_latency:.2f}ms too high for HFT"
        assert p99_latency < 50.0, f"P99 message latency {p99_latency:.2f}ms too high for HFT"
        
        print(f"Message latency: avg={avg_latency:.2f}ms, p50={p50_latency:.2f}ms, p95={p95_latency:.2f}ms, p99={p99_latency:.2f}ms")
    
    assert_success_rate(successful_sends, 50, 0.90, "Message processing")

@pytest.mark.performance
@pytest.mark.asyncio
async def test_throughput_scaling(config, kafka_producer, test_symbols):
    """Test system throughput scaling under increasing load."""
    test_topic = "throughput-test"
    
    # Test different message rates
    rates_to_test = [100, 500, 1000, 2000]  # messages per second
    results = {}
    
    for target_rate in rates_to_test:
        print(f"Testing throughput at {target_rate} msg/s")
        
        test_duration = 30  # seconds
        interval = 1.0 / target_rate
        
        start_time = time.time()
        successful_sends = 0
        failed_sends = 0
        send_latencies = []
        
        message_count = 0
        while time.time() - start_time < test_duration:
            symbol = test_symbols[message_count % len(test_symbols)]
            message = {
                "symbol": symbol,
                "sequence": message_count,
                "price": 100.0 + (message_count % 100),
                "timestamp": int(time.time() * 1000),
                "rate_test": target_rate
            }
            
            send_start = time.time()
            try:
                future = kafka_producer.send(test_topic, value=message, key=symbol)
                future.get(timeout=2)
                send_latency = (time.time() - send_start) * 1000
                send_latencies.append(send_latency)
                successful_sends += 1
            except Exception:
                failed_sends += 1
            
            message_count += 1
            
            # Control rate
            await asyncio.sleep(interval)
        
        actual_duration = time.time() - start_time
        actual_rate = successful_sends / actual_duration
        success_rate = successful_sends / (successful_sends + failed_sends) if (successful_sends + failed_sends) > 0 else 0
        
        if send_latencies:
            avg_latency = statistics.mean(send_latencies)
            p95_latency = statistics.quantiles(send_latencies, n=20)[18] if len(send_latencies) >= 5 else max(send_latencies)
        else:
            avg_latency = float('inf')
            p95_latency = float('inf')
        
        results[target_rate] = {
            "actual_rate": actual_rate,
            "success_rate": success_rate,
            "avg_latency": avg_latency,
            "p95_latency": p95_latency,
            "successful_sends": successful_sends,
            "failed_sends": failed_sends
        }
        
        print(f"Rate {target_rate}: actual={actual_rate:.1f} msg/s, success={success_rate:.2%}, latency={avg_latency:.2f}ms")
    
    # Analyze scaling behavior
    for target_rate, result in results.items():
        # Success rate should remain high
        min_success_rate = 0.95 if target_rate <= 1000 else 0.85
        assert result["success_rate"] >= min_success_rate, f"Success rate {result['success_rate']:.2%} too low at {target_rate} msg/s"
        
        # Should achieve at least 80% of target rate
        min_actual_rate = target_rate * 0.8
        assert result["actual_rate"] >= min_actual_rate, f"Actual rate {result['actual_rate']:.1f} too low for target {target_rate} msg/s"
        
        # Latency should remain reasonable
        max_avg_latency = 20.0 if target_rate <= 1000 else 50.0
        max_p95_latency = 50.0 if target_rate <= 1000 else 100.0
        
        if result["avg_latency"] != float('inf'):
            assert result["avg_latency"] < max_avg_latency, f"Average latency {result['avg_latency']:.2f}ms too high at {target_rate} msg/s"
        if result["p95_latency"] != float('inf'):
            assert result["p95_latency"] < max_p95_latency, f"P95 latency {result['p95_latency']:.2f}ms too high at {target_rate} msg/s"

@pytest.mark.performance
@pytest.mark.asyncio
async def test_memory_usage_stability(config, http_session, kafka_producer, test_symbols, wait_for_services):
    """Test memory usage stability under load."""
    metrics_url = f"http://{config['hft_host']}:{config['hft_metrics_port']}/metrics"
    
    async def get_memory_usage():
        try:
            async with http_session.get(metrics_url) as response:
                if response.status == 200:
                    metrics_text = await response.text()
                    # Look for memory metrics
                    for line in metrics_text.split('\n'):
                        if 'process_resident_memory_bytes' in line and not line.startswith('#'):
                            parts = line.split()
                            if len(parts) >= 2:
                                return float(parts[1])
            return None
        except Exception:
            return None
    
    # Get baseline memory usage
    baseline_memory = await get_memory_usage()
    if baseline_memory is None:
        pytest.skip("Memory metrics not available")
    
    memory_samples = [baseline_memory]
    
    # Generate load while monitoring memory
    test_duration = 60  # 1 minute
    message_rate = 500  # messages per second
    
    start_time = time.time()
    message_count = 0
    
    # Background task to monitor memory
    async def monitor_memory():
        while time.time() - start_time < test_duration:
            memory = await get_memory_usage()
            if memory is not None:
                memory_samples.append(memory)
            await asyncio.sleep(5)  # Sample every 5 seconds
    
    monitor_task = asyncio.create_task(monitor_memory())
    
    # Generate load
    while time.time() - start_time < test_duration:
        symbol = test_symbols[message_count % len(test_symbols)]
        message = {
            "symbol": symbol,
            "sequence": message_count,
            "price": 100.0 + (message_count % 100),
            "timestamp": int(time.time() * 1000),
            "memory_test": True
        }
        
        try:
            kafka_producer.send("memory-test", value=message, key=symbol)
        except Exception:
            pass  # Ignore send failures for this test
        
        message_count += 1
        await asyncio.sleep(1.0 / message_rate)
    
    # Wait for monitoring to complete
    await monitor_task
    
    # Analyze memory usage
    if len(memory_samples) >= 3:
        memory_mb = [m / (1024 * 1024) for m in memory_samples]  # Convert to MB
        baseline_mb = memory_mb[0]
        max_memory_mb = max(memory_mb)
        final_memory_mb = memory_mb[-1]
        
        # Memory growth analysis
        memory_growth = max_memory_mb - baseline_mb
        memory_leak_indicator = final_memory_mb - baseline_mb
        
        print(f"Memory usage: baseline={baseline_mb:.1f}MB, max={max_memory_mb:.1f}MB, final={final_memory_mb:.1f}MB")
        print(f"Memory growth: {memory_growth:.1f}MB, potential leak: {memory_leak_indicator:.1f}MB")
        
        # Memory stability assertions
        max_growth_mb = 100  # Allow up to 100MB growth under load
        max_leak_mb = 50     # Allow up to 50MB permanent growth
        
        assert memory_growth < max_growth_mb, f"Memory growth {memory_growth:.1f}MB exceeds limit {max_growth_mb}MB"
        assert memory_leak_indicator < max_leak_mb, f"Potential memory leak {memory_leak_indicator:.1f}MB exceeds limit {max_leak_mb}MB"
        
        # Check for memory stability (no continuous growth)
        if len(memory_mb) >= 6:
            # Compare first half vs second half
            first_half = memory_mb[:len(memory_mb)//2]
            second_half = memory_mb[len(memory_mb)//2:]
            
            avg_first_half = statistics.mean(first_half)
            avg_second_half = statistics.mean(second_half)
            
            growth_rate = (avg_second_half - avg_first_half) / avg_first_half
            assert growth_rate < 0.2, f"Memory growth rate {growth_rate:.2%} indicates potential leak"

@pytest.mark.performance
@pytest.mark.slow
@pytest.mark.asyncio
async def test_sustained_performance(config, http_session, kafka_producer, test_symbols, wait_for_services):
    """Test sustained performance over extended period."""
    test_duration = 300  # 5 minutes
    message_rate = 200   # messages per second
    health_check_interval = 30  # seconds
    
    start_time = time.time()
    message_count = 0
    successful_sends = 0
    failed_sends = 0
    health_check_times = []
    send_latencies = []
    
    # Background health monitoring
    async def monitor_health():
        health_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/health"
        while time.time() - start_time < test_duration:
            health_start = time.time()
            try:
                async with http_session.get(health_url) as response:
                    health_duration = time.time() - health_start
                    health_check_times.append((time.time() - start_time, health_duration, response.status == 200))
            except Exception:
                health_duration = time.time() - health_start
                health_check_times.append((time.time() - start_time, health_duration, False))
            
            await asyncio.sleep(health_check_interval)
    
    health_task = asyncio.create_task(monitor_health())
    
    # Generate sustained load
    while time.time() - start_time < test_duration:
        symbol = test_symbols[message_count % len(test_symbols)]
        message = {
            "symbol": symbol,
            "sequence": message_count,
            "price": 100.0 + (message_count % 100),
            "timestamp": int(time.time() * 1000),
            "sustained_test": True
        }
        
        send_start = time.time()
        try:
            future = kafka_producer.send("sustained-test", value=message, key=symbol)
            future.get(timeout=2)
            send_latency = (time.time() - send_start) * 1000
            send_latencies.append(send_latency)
            successful_sends += 1
        except Exception:
            failed_sends += 1
        
        message_count += 1
        await asyncio.sleep(1.0 / message_rate)
    
    # Wait for health monitoring to complete
    await health_task
    
    actual_duration = time.time() - start_time
    total_attempts = successful_sends + failed_sends
    
    # Analyze sustained performance
    actual_rate = successful_sends / actual_duration
    success_rate = successful_sends / total_attempts if total_attempts > 0 else 0
    
    # Health check analysis
    successful_health_checks = sum(1 for _, _, success in health_check_times if success)
    total_health_checks = len(health_check_times)
    health_success_rate = successful_health_checks / total_health_checks if total_health_checks > 0 else 0
    
    avg_health_time = statistics.mean([duration for _, duration, success in health_check_times if success])
    
    # Latency analysis over time
    if send_latencies:
        # Divide into time windows to check for degradation
        window_size = len(send_latencies) // 5  # 5 windows
        windows = [send_latencies[i:i+window_size] for i in range(0, len(send_latencies), window_size)]
        
        window_avg_latencies = [statistics.mean(window) for window in windows if window]
        
        # Check for performance degradation
        if len(window_avg_latencies) >= 2:
            first_window_avg = window_avg_latencies[0]
            last_window_avg = window_avg_latencies[-1]
            degradation = (last_window_avg - first_window_avg) / first_window_avg
            
            assert degradation < 0.5, f"Performance degraded by {degradation:.2%} over {test_duration/60:.1f} minutes"
        
        overall_avg_latency = statistics.mean(send_latencies)
        overall_p95_latency = statistics.quantiles(send_latencies, n=20)[18] if len(send_latencies) >= 5 else max(send_latencies)
    else:
        overall_avg_latency = float('inf')
        overall_p95_latency = float('inf')
    
    # Sustained performance assertions
    assert_success_rate(successful_sends, total_attempts, 0.90, "Sustained message processing")
    assert actual_rate >= message_rate * 0.8, f"Sustained rate {actual_rate:.1f} msg/s below 80% of target {message_rate} msg/s"
    assert health_success_rate >= 0.95, f"Health check success rate {health_success_rate:.2%} too low during sustained test"
    assert avg_health_time < 1.0, f"Average health check time {avg_health_time:.3f}s too high during sustained test"
    
    if overall_avg_latency != float('inf'):
        assert overall_avg_latency < 30.0, f"Sustained average latency {overall_avg_latency:.2f}ms too high"
    if overall_p95_latency != float('inf'):
        assert overall_p95_latency < 100.0, f"Sustained P95 latency {overall_p95_latency:.2f}ms too high"
    
    print(f"Sustained performance over {actual_duration/60:.1f} minutes:")
    print(f"  Message rate: {actual_rate:.1f} msg/s ({success_rate:.2%} success)")
    print(f"  Health checks: {health_success_rate:.2%} success, {avg_health_time*1000:.1f}ms avg")
    print(f"  Latency: {overall_avg_latency:.2f}ms avg, {overall_p95_latency:.2f}ms P95")