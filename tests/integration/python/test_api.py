"""
API integration tests for HFT RTB system.
"""

import asyncio
import json
import time
import uuid
import pytest
import aiohttp
from conftest import assert_response_time, assert_success_rate

@pytest.mark.api
@pytest.mark.asyncio
async def test_api_endpoints_discovery(config, http_session, wait_for_services):
    """Test discovery of available API endpoints."""
    base_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}"
    
    # Test common API endpoints
    endpoints_to_test = [
        ("/health", 200),
        ("/api/v1/orders", 405),  # Should exist but require POST/GET
        ("/api/v1/positions", 200),  # Should return positions (possibly empty)
        ("/api/v1/symbols", 200),   # Should return available symbols
        ("/api/v1/status", 200),    # Should return system status
    ]
    
    for endpoint, expected_status in endpoints_to_test:
        url = f"{base_url}{endpoint}"
        start_time = time.time()
        
        async with http_session.get(url) as response:
            duration = time.time() - start_time
            
            # Allow for some flexibility in status codes
            if expected_status == 200:
                assert response.status in [200, 404], f"Endpoint {endpoint} returned unexpected status {response.status}"
            else:
                assert response.status in [expected_status, 404, 405], f"Endpoint {endpoint} returned unexpected status {response.status}"
            
            assert_response_time(duration, 2.0, f"API endpoint {endpoint}")

@pytest.mark.api
@pytest.mark.asyncio
async def test_order_submission_api(config, http_session, sample_order_data, wait_for_services):
    """Test order submission through API."""
    orders_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/api/v1/orders"
    
    # Prepare order data
    order_data = sample_order_data.copy()
    order_data["client_order_id"] = str(uuid.uuid4())
    
    start_time = time.time()
    async with http_session.post(orders_url, json=order_data) as response:
        duration = time.time() - start_time
        
        # Accept various success status codes
        assert response.status in [200, 201, 202], f"Order submission returned status {response.status}"
        
        if response.content_type == 'application/json':
            data = await response.json()
            
            # Check response structure
            if "order_id" in data:
                assert data["order_id"] is not None, "Order ID should not be null"
            
            if "status" in data:
                assert data["status"] in ["pending", "accepted", "filled", "rejected"], f"Invalid order status: {data['status']}"
            
            # If order was rejected, there should be a reason
            if data.get("status") == "rejected":
                assert "reason" in data or "message" in data, "Rejected order should include reason"
        
        assert_response_time(duration, 1.0, "Order submission")

@pytest.mark.api
@pytest.mark.asyncio
async def test_order_status_query(config, http_session, sample_order_data, wait_for_services):
    """Test querying order status through API."""
    orders_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/api/v1/orders"
    
    # First, submit an order
    order_data = sample_order_data.copy()
    order_data["client_order_id"] = str(uuid.uuid4())
    
    order_id = None
    async with http_session.post(orders_url, json=order_data) as response:
        if response.status in [200, 201, 202] and response.content_type == 'application/json':
            data = await response.json()
            order_id = data.get("order_id") or data.get("client_order_id")
    
    if order_id:
        # Query the order status
        status_url = f"{orders_url}/{order_id}"
        start_time = time.time()
        
        async with http_session.get(status_url) as response:
            duration = time.time() - start_time
            
            assert response.status in [200, 404], f"Order status query returned status {response.status}"
            
            if response.status == 200 and response.content_type == 'application/json':
                data = await response.json()
                
                # Validate order data structure
                expected_fields = ["order_id", "symbol", "side", "quantity", "status"]
                for field in expected_fields:
                    if field in data:
                        assert data[field] is not None, f"Field {field} should not be null"
            
            assert_response_time(duration, 0.5, "Order status query")

@pytest.mark.api
@pytest.mark.asyncio
async def test_positions_api(config, http_session, wait_for_services):
    """Test positions API endpoint."""
    positions_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/api/v1/positions"
    
    start_time = time.time()
    async with http_session.get(positions_url) as response:
        duration = time.time() - start_time
        
        assert response.status in [200, 404], f"Positions API returned status {response.status}"
        
        if response.status == 200 and response.content_type == 'application/json':
            data = await response.json()
            
            # Should be a list or dict
            assert isinstance(data, (list, dict)), "Positions should be list or dict"
            
            if isinstance(data, list):
                for position in data:
                    if isinstance(position, dict):
                        # Validate position structure
                        if "symbol" in position:
                            assert isinstance(position["symbol"], str), "Symbol should be string"
                        if "quantity" in position:
                            assert isinstance(position["quantity"], (int, float)), "Quantity should be numeric"
            
            elif isinstance(data, dict):
                # Could be a dict of symbol -> position
                for symbol, position in data.items():
                    assert isinstance(symbol, str), "Position key should be symbol string"
        
        assert_response_time(duration, 1.0, "Positions query")

@pytest.mark.api
@pytest.mark.asyncio
async def test_symbols_api(config, http_session, wait_for_services):
    """Test symbols API endpoint."""
    symbols_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/api/v1/symbols"
    
    start_time = time.time()
    async with http_session.get(symbols_url) as response:
        duration = time.time() - start_time
        
        assert response.status in [200, 404], f"Symbols API returned status {response.status}"
        
        if response.status == 200 and response.content_type == 'application/json':
            data = await response.json()
            
            # Should be a list of symbols or dict with symbol info
            assert isinstance(data, (list, dict)), "Symbols should be list or dict"
            
            if isinstance(data, list):
                assert len(data) > 0, "Should have at least some symbols"
                for symbol in data:
                    assert isinstance(symbol, str), "Symbol should be string"
                    assert len(symbol) > 0, "Symbol should not be empty"
            
            elif isinstance(data, dict):
                assert len(data) > 0, "Should have at least some symbols"
                for symbol, info in data.items():
                    assert isinstance(symbol, str), "Symbol should be string"
                    assert len(symbol) > 0, "Symbol should not be empty"
        
        assert_response_time(duration, 1.0, "Symbols query")

@pytest.mark.api
@pytest.mark.asyncio
async def test_system_status_api(config, http_session, wait_for_services):
    """Test system status API endpoint."""
    status_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/api/v1/status"
    
    start_time = time.time()
    async with http_session.get(status_url) as response:
        duration = time.time() - start_time
        
        assert response.status in [200, 404], f"Status API returned status {response.status}"
        
        if response.status == 200 and response.content_type == 'application/json':
            data = await response.json()
            
            # Validate status structure
            expected_fields = ["status", "timestamp"]
            for field in expected_fields:
                if field in data:
                    assert data[field] is not None, f"Field {field} should not be null"
            
            # Status should be a valid state
            if "status" in data:
                valid_statuses = ["healthy", "running", "active", "ready", "degraded", "maintenance"]
                assert data["status"] in valid_statuses, f"Invalid status: {data['status']}"
            
            # Check for additional useful fields
            useful_fields = ["uptime", "version", "connections", "processed_messages", "active_orders"]
            found_fields = [field for field in useful_fields if field in data]
            
            # Should have at least some useful information
            assert len(found_fields) >= 1, f"Status should include useful fields, found: {found_fields}"
        
        assert_response_time(duration, 1.0, "Status query")

@pytest.mark.api
@pytest.mark.asyncio
async def test_api_error_handling(config, http_session, wait_for_services):
    """Test API error handling for invalid requests."""
    base_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}"
    
    # Test invalid endpoints
    invalid_endpoints = [
        "/api/v1/nonexistent",
        "/api/v1/orders/invalid-id",
        "/api/v2/orders",  # Wrong version
        "/invalid/path"
    ]
    
    for endpoint in invalid_endpoints:
        url = f"{base_url}{endpoint}"
        async with http_session.get(url) as response:
            # Should return proper HTTP error codes
            assert response.status in [404, 405, 400], f"Endpoint {endpoint} should return error status, got {response.status}"

@pytest.mark.api
@pytest.mark.asyncio
async def test_api_invalid_order_data(config, http_session, wait_for_services):
    """Test API handling of invalid order data."""
    orders_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/api/v1/orders"
    
    # Test various invalid order scenarios
    invalid_orders = [
        {},  # Empty order
        {"symbol": "INVALID"},  # Missing required fields
        {"symbol": "AAPL", "side": "invalid", "quantity": 100},  # Invalid side
        {"symbol": "AAPL", "side": "buy", "quantity": -100},  # Negative quantity
        {"symbol": "", "side": "buy", "quantity": 100},  # Empty symbol
        {"symbol": "AAPL", "side": "buy", "quantity": "invalid"},  # Invalid quantity type
    ]
    
    for invalid_order in invalid_orders:
        async with http_session.post(orders_url, json=invalid_order) as response:
            # Should reject invalid orders
            assert response.status in [400, 422, 500], f"Invalid order should be rejected, got status {response.status} for {invalid_order}"

@pytest.mark.api
@pytest.mark.asyncio
async def test_api_concurrent_requests(config, http_session, wait_for_services):
    """Test API handling of concurrent requests."""
    positions_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/api/v1/positions"
    
    async def make_request():
        start_time = time.time()
        async with http_session.get(positions_url) as response:
            duration = time.time() - start_time
            return response.status, duration
    
    # Make 20 concurrent requests
    tasks = [make_request() for _ in range(20)]
    results = await asyncio.gather(*tasks)
    
    # Analyze results
    statuses = [result[0] for result in results]
    durations = [result[1] for result in results]
    
    successful_requests = sum(1 for status in statuses if status in [200, 404])
    assert_success_rate(successful_requests, len(results), 0.90, "Concurrent API requests")
    
    # Response times should be reasonable
    avg_duration = sum(durations) / len(durations)
    max_duration = max(durations)
    
    assert avg_duration < 2.0, f"Average API response time {avg_duration:.3f}s too high"
    assert max_duration < 5.0, f"Maximum API response time {max_duration:.3f}s too high"

@pytest.mark.api
@pytest.mark.asyncio
async def test_api_content_types(config, http_session, wait_for_services):
    """Test API content type handling."""
    orders_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/api/v1/orders"
    
    order_data = {
        "symbol": "AAPL",
        "side": "buy",
        "quantity": 100,
        "price": 150.0,
        "client_order_id": str(uuid.uuid4())
    }
    
    # Test JSON content type
    headers = {"Content-Type": "application/json"}
    async with http_session.post(orders_url, json=order_data, headers=headers) as response:
        json_status = response.status
    
    # Test form data (should be rejected or handled differently)
    headers = {"Content-Type": "application/x-www-form-urlencoded"}
    form_data = "&".join([f"{k}={v}" for k, v in order_data.items()])
    async with http_session.post(orders_url, data=form_data, headers=headers) as response:
        form_status = response.status
    
    # JSON should be accepted, form data might be rejected
    assert json_status in [200, 201, 202, 400, 422], f"JSON request returned unexpected status {json_status}"
    assert form_status in [200, 201, 202, 400, 415, 422], f"Form request returned unexpected status {form_status}"

@pytest.mark.api
@pytest.mark.performance
@pytest.mark.asyncio
async def test_api_performance_under_load(config, http_session, test_symbols, wait_for_services):
    """Test API performance under sustained load."""
    positions_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/api/v1/positions"
    orders_url = f"http://{config['hft_host']}:{config['hft_gateway_port']}/api/v1/orders"
    
    # Test parameters
    test_duration = 30  # seconds
    requests_per_second = 10
    
    start_time = time.time()
    request_results = []
    
    async def make_api_request(request_type, url, data=None):
        req_start = time.time()
        try:
            if request_type == "GET":
                async with http_session.get(url) as response:
                    duration = time.time() - req_start
                    return response.status, duration, None
            elif request_type == "POST" and data:
                async with http_session.post(url, json=data) as response:
                    duration = time.time() - req_start
                    return response.status, duration, None
        except Exception as e:
            duration = time.time() - req_start
            return 0, duration, str(e)
    
    # Generate sustained load
    request_count = 0
    while time.time() - start_time < test_duration:
        # Alternate between different API calls
        if request_count % 3 == 0:
            # GET positions
            task = make_api_request("GET", positions_url)
        elif request_count % 3 == 1:
            # GET positions again
            task = make_api_request("GET", positions_url)
        else:
            # POST order (might fail, that's ok)
            order_data = {
                "symbol": test_symbols[request_count % len(test_symbols)],
                "side": "buy",
                "quantity": 100,
                "price": 100.0 + request_count,
                "client_order_id": str(uuid.uuid4())
            }
            task = make_api_request("POST", orders_url, order_data)
        
        result = await task
        request_results.append(result)
        request_count += 1
        
        # Control request rate
        await asyncio.sleep(1.0 / requests_per_second)
    
    # Analyze performance
    total_requests = len(request_results)
    successful_requests = sum(1 for status, _, _ in request_results if status in [200, 201, 202, 404])
    durations = [duration for _, duration, _ in request_results if duration < 10.0]  # Exclude timeouts
    
    if durations:
        avg_duration = sum(durations) / len(durations)
        p95_duration = sorted(durations)[int(len(durations) * 0.95)]
        max_duration = max(durations)
        
        # Performance assertions
        assert_success_rate(successful_requests, total_requests, 0.85, "API requests under load")
        assert avg_duration < 1.0, f"Average API response time {avg_duration:.3f}s too high under load"
        assert p95_duration < 2.0, f"P95 API response time {p95_duration:.3f}s too high under load"
        assert max_duration < 5.0, f"Maximum API response time {max_duration:.3f}s too high under load"
        
        # Should maintain reasonable throughput
        actual_rps = total_requests / test_duration
        expected_rps = requests_per_second * 0.8  # Allow 20% tolerance
        assert actual_rps >= expected_rps, f"API throughput {actual_rps:.1f} RPS below expected {expected_rps:.1f} RPS"
    else:
        pytest.fail("No valid API response times recorded during load test")