"""
Pytest configuration and fixtures for HFT RTB integration tests.
"""

import asyncio
import json
import os
import time
from typing import Dict, Any, Optional
import pytest
import aiohttp
import redis
from kafka import KafkaProducer, KafkaConsumer
from kafka.errors import KafkaError
import logging

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Test configuration
DEFAULT_CONFIG = {
    "hft_host": "localhost",
    "hft_gateway_port": 8080,
    "hft_metrics_port": 9090,
    "hft_admin_port": 50051,
    "kafka_brokers": "localhost:9092",
    "redis_url": "redis://localhost:6379",
    "test_timeout": 30,
    "health_check_retries": 10,
    "health_check_delay": 2,
}

def get_config() -> Dict[str, Any]:
    """Get test configuration from environment variables or defaults."""
    config = DEFAULT_CONFIG.copy()
    
    # Override with environment variables
    for key in config:
        env_key = f"HFT_TEST_{key.upper()}"
        if env_key in os.environ:
            value = os.environ[env_key]
            # Try to convert to appropriate type
            if key.endswith("_port") or key == "test_timeout" or key.endswith("_retries") or key.endswith("_delay"):
                config[key] = int(value)
            else:
                config[key] = value
    
    return config

@pytest.fixture(scope="session")
def config():
    """Test configuration fixture."""
    return get_config()

@pytest.fixture(scope="session")
def event_loop():
    """Create an instance of the default event loop for the test session."""
    loop = asyncio.get_event_loop_policy().new_event_loop()
    yield loop
    loop.close()

@pytest.fixture(scope="session")
async def http_session():
    """HTTP session for making requests."""
    timeout = aiohttp.ClientTimeout(total=30)
    async with aiohttp.ClientSession(timeout=timeout) as session:
        yield session

@pytest.fixture(scope="session")
async def wait_for_services(config):
    """Wait for all services to be ready before running tests."""
    logger.info("Waiting for services to be ready...")
    
    # Wait for HFT engine health endpoint
    health_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/health"
    
    for attempt in range(config["health_check_retries"]):
        try:
            async with aiohttp.ClientSession() as session:
                async with session.get(health_url) as response:
                    if response.status == 200:
                        data = await response.json()
                        if data.get("status") == "healthy":
                            logger.info("HFT engine is ready")
                            break
        except Exception as e:
            logger.debug(f"Health check attempt {attempt + 1} failed: {e}")
        
        if attempt < config["health_check_retries"] - 1:
            await asyncio.sleep(config["health_check_delay"])
    else:
        pytest.fail("HFT engine failed to become ready within timeout")
    
    # Wait for Kafka
    try:
        producer = KafkaProducer(
            bootstrap_servers=config["kafka_brokers"],
            value_serializer=lambda v: json.dumps(v).encode('utf-8'),
            request_timeout_ms=5000
        )
        producer.close()
        logger.info("Kafka is ready")
    except KafkaError as e:
        pytest.fail(f"Kafka is not ready: {e}")
    
    # Wait for Redis
    try:
        r = redis.from_url(config["redis_url"], socket_timeout=5)
        r.ping()
        logger.info("Redis is ready")
    except Exception as e:
        pytest.fail(f"Redis is not ready: {e}")
    
    logger.info("All services are ready")

@pytest.fixture
async def kafka_producer(config):
    """Kafka producer fixture."""
    producer = KafkaProducer(
        bootstrap_servers=config["kafka_brokers"],
        value_serializer=lambda v: json.dumps(v).encode('utf-8'),
        key_serializer=lambda k: k.encode('utf-8') if k else None,
        request_timeout_ms=10000,
        retries=3
    )
    yield producer
    producer.close()

@pytest.fixture
async def kafka_consumer(config):
    """Kafka consumer fixture."""
    consumer = KafkaConsumer(
        bootstrap_servers=config["kafka_brokers"],
        value_deserializer=lambda m: json.loads(m.decode('utf-8')),
        key_deserializer=lambda k: k.decode('utf-8') if k else None,
        auto_offset_reset='latest',
        enable_auto_commit=True,
        group_id='test-group',
        consumer_timeout_ms=5000
    )
    yield consumer
    consumer.close()

@pytest.fixture
async def redis_client(config):
    """Redis client fixture."""
    client = redis.from_url(config["redis_url"], decode_responses=True)
    yield client
    client.close()

@pytest.fixture
def test_symbols():
    """Common test symbols."""
    return ["AAPL", "GOOGL", "MSFT", "TSLA", "AMZN", "META", "NVDA", "NFLX"]

@pytest.fixture
def sample_tick_data():
    """Sample tick data for testing."""
    return {
        "symbol": "AAPL",
        "price": 150.25,
        "volume": 1000,
        "timestamp": int(time.time() * 1000),
        "bid": 150.20,
        "ask": 150.30,
        "bid_size": 500,
        "ask_size": 750
    }

@pytest.fixture
def sample_order_data():
    """Sample order data for testing."""
    return {
        "symbol": "AAPL",
        "side": "buy",
        "quantity": 100,
        "price": 150.25,
        "order_type": "limit",
        "time_in_force": "GTC"
    }

class TestMetrics:
    """Helper class for collecting and analyzing test metrics."""
    
    def __init__(self):
        self.metrics = {}
        self.start_time = None
        self.end_time = None
    
    def start_timer(self, name: str):
        """Start a timer for a metric."""
        self.metrics[f"{name}_start"] = time.time()
    
    def end_timer(self, name: str):
        """End a timer and calculate duration."""
        start_key = f"{name}_start"
        if start_key in self.metrics:
            duration = time.time() - self.metrics[start_key]
            self.metrics[f"{name}_duration"] = duration
            return duration
        return None
    
    def add_metric(self, name: str, value: Any):
        """Add a custom metric."""
        self.metrics[name] = value
    
    def get_summary(self) -> Dict[str, Any]:
        """Get a summary of all metrics."""
        return self.metrics.copy()

@pytest.fixture
def test_metrics():
    """Test metrics collector fixture."""
    return TestMetrics()

# Pytest hooks for custom reporting
def pytest_configure(config):
    """Configure pytest with custom markers."""
    config.addinivalue_line(
        "markers", "health: mark test as health check test"
    )
    config.addinivalue_line(
        "markers", "api: mark test as API test"
    )
    config.addinivalue_line(
        "markers", "kafka: mark test as Kafka integration test"
    )
    config.addinivalue_line(
        "markers", "redis: mark test as Redis integration test"
    )
    config.addinivalue_line(
        "markers", "grpc: mark test as gRPC test"
    )
    config.addinivalue_line(
        "markers", "performance: mark test as performance test"
    )
    config.addinivalue_line(
        "markers", "load: mark test as load test"
    )
    config.addinivalue_line(
        "markers", "slow: mark test as slow running test"
    )

def pytest_collection_modifyitems(config, items):
    """Modify test collection to add markers based on test names."""
    for item in items:
        # Add markers based on test file names
        if "health" in item.nodeid:
            item.add_marker(pytest.mark.health)
        if "api" in item.nodeid:
            item.add_marker(pytest.mark.api)
        if "kafka" in item.nodeid:
            item.add_marker(pytest.mark.kafka)
        if "redis" in item.nodeid:
            item.add_marker(pytest.mark.redis)
        if "grpc" in item.nodeid:
            item.add_marker(pytest.mark.grpc)
        if "performance" in item.nodeid or "load" in item.nodeid:
            item.add_marker(pytest.mark.performance)
            item.add_marker(pytest.mark.slow)

# Custom assertions
def assert_response_time(duration: float, max_time: float, operation: str = "operation"):
    """Assert that an operation completed within the expected time."""
    assert duration <= max_time, f"{operation} took {duration:.3f}s, expected <= {max_time}s"

def assert_success_rate(successful: int, total: int, min_rate: float = 0.95, operation: str = "operation"):
    """Assert that success rate meets minimum threshold."""
    if total == 0:
        pytest.fail(f"No {operation} attempts recorded")
    
    success_rate = successful / total
    assert success_rate >= min_rate, f"{operation} success rate {success_rate:.2%} below minimum {min_rate:.2%} ({successful}/{total})"

def assert_latency_percentile(latencies: list, percentile: int, max_latency: float, operation: str = "operation"):
    """Assert that latency percentile is within acceptable range."""
    if not latencies:
        pytest.fail(f"No {operation} latencies recorded")
    
    sorted_latencies = sorted(latencies)
    index = int(len(sorted_latencies) * percentile / 100) - 1
    p_latency = sorted_latencies[max(0, index)]
    
    assert p_latency <= max_latency, f"{operation} P{percentile} latency {p_latency:.3f}s exceeds maximum {max_latency}s"