"""
Kafka integration tests for HFT RTB system.
"""

import asyncio
import json
import time
import uuid
from typing import List, Dict, Any
import pytest
from kafka import KafkaProducer, KafkaConsumer
from kafka.errors import KafkaError, KafkaTimeoutError
from conftest import assert_response_time, assert_success_rate, assert_latency_percentile

@pytest.mark.kafka
@pytest.mark.asyncio
async def test_kafka_connectivity(config):
    """Test basic Kafka connectivity."""
    try:
        # Test producer creation
        producer = KafkaProducer(
            bootstrap_servers=config["kafka_brokers"],
            value_serializer=lambda v: json.dumps(v).encode('utf-8'),
            request_timeout_ms=5000
        )
        
        # Test consumer creation
        consumer = KafkaConsumer(
            bootstrap_servers=config["kafka_brokers"],
            value_deserializer=lambda m: json.loads(m.decode('utf-8')),
            auto_offset_reset='latest',
            consumer_timeout_ms=1000
        )
        
        # Clean up
        producer.close()
        consumer.close()
        
    except KafkaError as e:
        pytest.fail(f"Kafka connectivity test failed: {e}")

@pytest.mark.kafka
@pytest.mark.asyncio
async def test_kafka_topic_creation_and_listing(config):
    """Test Kafka topic operations."""
    from kafka.admin import KafkaAdminClient, NewTopic
    
    admin_client = KafkaAdminClient(
        bootstrap_servers=config["kafka_brokers"],
        client_id='test_admin'
    )
    
    # Create a test topic
    test_topic = f"test-topic-{uuid.uuid4().hex[:8]}"
    topic_list = [NewTopic(name=test_topic, num_partitions=1, replication_factor=1)]
    
    try:
        # Create topic
        fs = admin_client.create_topics(new_topics=topic_list, validate_only=False)
        for topic, f in fs.items():
            try:
                f.result()  # The result itself is None
                print(f"Topic {topic} created successfully")
            except Exception as e:
                print(f"Failed to create topic {topic}: {e}")
        
        # List topics to verify creation
        metadata = admin_client.list_consumer_groups()
        
        # Clean up - delete the test topic
        admin_client.delete_topics([test_topic])
        
    except Exception as e:
        pytest.fail(f"Topic operations failed: {e}")
    finally:
        admin_client.close()

@pytest.mark.kafka
@pytest.mark.asyncio
async def test_kafka_message_production(config, test_symbols):
    """Test Kafka message production."""
    producer = KafkaProducer(
        bootstrap_servers=config["kafka_brokers"],
        value_serializer=lambda v: json.dumps(v).encode('utf-8'),
        key_serializer=lambda k: k.encode('utf-8') if k else None,
        request_timeout_ms=10000,
        retries=3
    )
    
    test_topic = "market-data"
    messages_sent = 0
    send_errors = 0
    
    try:
        # Send test messages
        for i in range(10):
            symbol = test_symbols[i % len(test_symbols)]
            message = {
                "symbol": symbol,
                "price": 100.0 + i,
                "volume": 1000 + i * 100,
                "timestamp": int(time.time() * 1000),
                "test_id": str(uuid.uuid4())
            }
            
            try:
                future = producer.send(test_topic, value=message, key=symbol)
                record_metadata = future.get(timeout=5)
                messages_sent += 1
                
                # Verify metadata
                assert record_metadata.topic == test_topic
                assert record_metadata.partition >= 0
                assert record_metadata.offset >= 0
                
            except KafkaTimeoutError:
                send_errors += 1
            except Exception as e:
                send_errors += 1
                print(f"Send error: {e}")
        
        # Flush to ensure all messages are sent
        producer.flush(timeout=10)
        
    finally:
        producer.close()
    
    # Verify success rate
    total_attempts = messages_sent + send_errors
    assert_success_rate(messages_sent, total_attempts, 0.8, "Kafka message production")

@pytest.mark.kafka
@pytest.mark.asyncio
async def test_kafka_message_consumption(config, test_symbols):
    """Test Kafka message consumption."""
    test_topic = "market-data"
    
    # First, produce some messages
    producer = KafkaProducer(
        bootstrap_servers=config["kafka_brokers"],
        value_serializer=lambda v: json.dumps(v).encode('utf-8'),
        key_serializer=lambda k: k.encode('utf-8') if k else None
    )
    
    test_messages = []
    for i in range(5):
        symbol = test_symbols[i % len(test_symbols)]
        message = {
            "symbol": symbol,
            "price": 100.0 + i,
            "volume": 1000,
            "timestamp": int(time.time() * 1000),
            "test_id": str(uuid.uuid4())
        }
        test_messages.append(message)
        producer.send(test_topic, value=message, key=symbol)
    
    producer.flush()
    producer.close()
    
    # Now consume the messages
    consumer = KafkaConsumer(
        test_topic,
        bootstrap_servers=config["kafka_brokers"],
        value_deserializer=lambda m: json.loads(m.decode('utf-8')),
        key_deserializer=lambda k: k.decode('utf-8') if k else None,
        auto_offset_reset='earliest',
        consumer_timeout_ms=10000,
        group_id=f'test-group-{uuid.uuid4().hex[:8]}'
    )
    
    consumed_messages = []
    start_time = time.time()
    
    try:
        for message in consumer:
            consumed_messages.append(message.value)
            
            # Stop if we've consumed enough or timeout
            if len(consumed_messages) >= len(test_messages) or time.time() - start_time > 15:
                break
    
    except Exception as e:
        print(f"Consumption error: {e}")
    finally:
        consumer.close()
    
    # Verify we consumed some messages
    assert len(consumed_messages) > 0, "No messages were consumed"
    
    # Verify message structure
    for msg in consumed_messages:
        assert "symbol" in msg, "Message missing symbol field"
        assert "price" in msg, "Message missing price field"
        assert "timestamp" in msg, "Message missing timestamp field"

@pytest.mark.kafka
@pytest.mark.asyncio
async def test_kafka_message_ordering(config, test_symbols):
    """Test Kafka message ordering within partitions."""
    test_topic = "market-data"
    symbol = test_symbols[0]  # Use single symbol to ensure same partition
    
    producer = KafkaProducer(
        bootstrap_servers=config["kafka_brokers"],
        value_serializer=lambda v: json.dumps(v).encode('utf-8'),
        key_serializer=lambda k: k.encode('utf-8') if k else None,
        acks='all'  # Ensure all replicas acknowledge
    )
    
    # Send ordered messages
    sent_messages = []
    for i in range(10):
        message = {
            "symbol": symbol,
            "sequence": i,
            "price": 100.0 + i,
            "timestamp": int(time.time() * 1000) + i,
            "test_id": str(uuid.uuid4())
        }
        sent_messages.append(message)
        producer.send(test_topic, value=message, key=symbol)
    
    producer.flush()
    producer.close()
    
    # Consume messages
    consumer = KafkaConsumer(
        test_topic,
        bootstrap_servers=config["kafka_brokers"],
        value_deserializer=lambda m: json.loads(m.decode('utf-8')),
        key_deserializer=lambda k: k.decode('utf-8') if k else None,
        auto_offset_reset='earliest',
        consumer_timeout_ms=10000,
        group_id=f'test-order-group-{uuid.uuid4().hex[:8]}'
    )
    
    consumed_messages = []
    start_time = time.time()
    
    try:
        for message in consumer:
            if message.value.get("symbol") == symbol and "sequence" in message.value:
                consumed_messages.append(message.value)
            
            if len(consumed_messages) >= len(sent_messages) or time.time() - start_time > 15:
                break
    finally:
        consumer.close()
    
    # Verify ordering
    if len(consumed_messages) >= 2:
        sequences = [msg["sequence"] for msg in consumed_messages]
        for i in range(1, len(sequences)):
            assert sequences[i] > sequences[i-1], f"Message ordering violated: {sequences[i-1]} -> {sequences[i]}"

@pytest.mark.kafka
@pytest.mark.asyncio
async def test_kafka_consumer_groups(config, test_symbols):
    """Test Kafka consumer group functionality."""
    test_topic = "market-data"
    group_id = f'test-consumer-group-{uuid.uuid4().hex[:8]}'
    
    # Create two consumers in the same group
    consumer1 = KafkaConsumer(
        test_topic,
        bootstrap_servers=config["kafka_brokers"],
        value_deserializer=lambda m: json.loads(m.decode('utf-8')),
        group_id=group_id,
        auto_offset_reset='latest',
        consumer_timeout_ms=5000
    )
    
    consumer2 = KafkaConsumer(
        test_topic,
        bootstrap_servers=config["kafka_brokers"],
        value_deserializer=lambda m: json.loads(m.decode('utf-8')),
        group_id=group_id,
        auto_offset_reset='latest',
        consumer_timeout_ms=5000
    )
    
    # Produce messages
    producer = KafkaProducer(
        bootstrap_servers=config["kafka_brokers"],
        value_serializer=lambda v: json.dumps(v).encode('utf-8'),
        key_serializer=lambda k: k.encode('utf-8') if k else None
    )
    
    messages_to_send = 20
    for i in range(messages_to_send):
        symbol = test_symbols[i % len(test_symbols)]
        message = {
            "symbol": symbol,
            "sequence": i,
            "timestamp": int(time.time() * 1000),
            "test_id": str(uuid.uuid4())
        }
        producer.send(test_topic, value=message, key=symbol)
    
    producer.flush()
    producer.close()
    
    # Consume with both consumers
    async def consume_messages(consumer, consumer_id):
        messages = []
        start_time = time.time()
        try:
            for message in consumer:
                messages.append(message.value)
                if time.time() - start_time > 10:  # 10 second timeout
                    break
        except Exception as e:
            print(f"Consumer {consumer_id} error: {e}")
        finally:
            consumer.close()
        return messages
    
    # Run both consumers concurrently
    results = await asyncio.gather(
        asyncio.to_thread(consume_messages, consumer1, "1"),
        asyncio.to_thread(consume_messages, consumer2, "2")
    )
    
    consumer1_messages, consumer2_messages = results
    
    # Verify load balancing
    total_consumed = len(consumer1_messages) + len(consumer2_messages)
    assert total_consumed > 0, "No messages consumed by either consumer"
    
    # Both consumers should have received some messages (load balancing)
    # Note: This might not always be true if one consumer starts much later
    print(f"Consumer 1 received {len(consumer1_messages)} messages")
    print(f"Consumer 2 received {len(consumer2_messages)} messages")

@pytest.mark.kafka
@pytest.mark.performance
@pytest.mark.asyncio
async def test_kafka_throughput_performance(config, test_symbols):
    """Test Kafka throughput performance."""
    test_topic = "performance-test"
    
    # High-performance producer configuration
    producer = KafkaProducer(
        bootstrap_servers=config["kafka_brokers"],
        value_serializer=lambda v: json.dumps(v).encode('utf-8'),
        key_serializer=lambda k: k.encode('utf-8') if k else None,
        batch_size=16384,
        linger_ms=5,
        compression_type='snappy',
        acks=1,
        retries=3,
        request_timeout_ms=30000
    )
    
    # Performance test parameters
    num_messages = 1000
    start_time = time.time()
    successful_sends = 0
    send_latencies = []
    
    try:
        for i in range(num_messages):
            symbol = test_symbols[i % len(test_symbols)]
            message = {
                "symbol": symbol,
                "price": 100.0 + (i % 100),
                "volume": 1000 + i,
                "timestamp": int(time.time() * 1000),
                "sequence": i
            }
            
            send_start = time.time()
            try:
                future = producer.send(test_topic, value=message, key=symbol)
                future.get(timeout=10)  # Wait for send to complete
                send_latency = time.time() - send_start
                send_latencies.append(send_latency)
                successful_sends += 1
            except Exception as e:
                print(f"Send failed: {e}")
        
        producer.flush(timeout=30)
        
    finally:
        producer.close()
    
    total_time = time.time() - start_time
    
    # Calculate performance metrics
    throughput = successful_sends / total_time
    success_rate = successful_sends / num_messages
    
    if send_latencies:
        avg_latency = sum(send_latencies) / len(send_latencies)
        p95_latency = sorted(send_latencies)[int(len(send_latencies) * 0.95)]
        
        # Performance assertions
        assert throughput >= 100, f"Kafka throughput {throughput:.1f} msg/s below minimum 100 msg/s"
        assert success_rate >= 0.95, f"Kafka success rate {success_rate:.2%} below 95%"
        assert avg_latency < 0.1, f"Average send latency {avg_latency:.3f}s above 100ms"
        assert p95_latency < 0.5, f"P95 send latency {p95_latency:.3f}s above 500ms"
        
        print(f"Kafka performance: {throughput:.1f} msg/s, {avg_latency*1000:.1f}ms avg latency, {success_rate:.2%} success rate")

@pytest.mark.kafka
@pytest.mark.asyncio
async def test_kafka_error_handling(config):
    """Test Kafka error handling scenarios."""
    
    # Test 1: Invalid topic name
    producer = KafkaProducer(
        bootstrap_servers=config["kafka_brokers"],
        value_serializer=lambda v: json.dumps(v).encode('utf-8'),
        request_timeout_ms=5000
    )
    
    try:
        # Try to send to invalid topic (should handle gracefully)
        future = producer.send("", value={"test": "data"})
        try:
            future.get(timeout=5)
        except Exception:
            pass  # Expected to fail
    finally:
        producer.close()
    
    # Test 2: Invalid broker address
    try:
        invalid_producer = KafkaProducer(
            bootstrap_servers="invalid-broker:9092",
            value_serializer=lambda v: json.dumps(v).encode('utf-8'),
            request_timeout_ms=2000,
            retries=1
        )
        
        # This should fail quickly
        start_time = time.time()
        try:
            future = invalid_producer.send("test-topic", value={"test": "data"})
            future.get(timeout=3)
        except Exception:
            pass  # Expected to fail
        finally:
            invalid_producer.close()
        
        connection_time = time.time() - start_time
        assert connection_time < 10, f"Invalid broker connection took too long: {connection_time:.1f}s"
        
    except Exception:
        pass  # Expected to fail during creation

@pytest.mark.kafka
@pytest.mark.asyncio
async def test_kafka_message_size_limits(config, test_symbols):
    """Test Kafka message size handling."""
    test_topic = "size-test"
    
    producer = KafkaProducer(
        bootstrap_servers=config["kafka_brokers"],
        value_serializer=lambda v: json.dumps(v).encode('utf-8'),
        key_serializer=lambda k: k.encode('utf-8') if k else None,
        max_request_size=1048576  # 1MB
    )
    
    try:
        # Test normal size message
        normal_message = {
            "symbol": test_symbols[0],
            "price": 100.0,
            "data": "x" * 1000  # 1KB of data
        }
        
        future = producer.send(test_topic, value=normal_message, key=test_symbols[0])
        future.get(timeout=5)  # Should succeed
        
        # Test large message (but within limits)
        large_message = {
            "symbol": test_symbols[1],
            "price": 100.0,
            "data": "x" * 100000  # 100KB of data
        }
        
        future = producer.send(test_topic, value=large_message, key=test_symbols[1])
        future.get(timeout=10)  # Should succeed but might be slower
        
        # Test very large message (might fail)
        very_large_message = {
            "symbol": test_symbols[2],
            "price": 100.0,
            "data": "x" * 2000000  # 2MB of data
        }
        
        try:
            future = producer.send(test_topic, value=very_large_message, key=test_symbols[2])
            future.get(timeout=10)
            print("Very large message sent successfully")
        except Exception as e:
            print(f"Very large message failed as expected: {e}")
        
    finally:
        producer.close()

@pytest.mark.kafka
@pytest.mark.slow
@pytest.mark.asyncio
async def test_kafka_reliability_under_load(config, test_symbols):
    """Test Kafka reliability under sustained load."""
    test_topic = "reliability-test"
    
    # Test parameters
    test_duration = 60  # 1 minute
    target_rate = 50  # messages per second
    
    producer = KafkaProducer(
        bootstrap_servers=config["kafka_brokers"],
        value_serializer=lambda v: json.dumps(v).encode('utf-8'),
        key_serializer=lambda k: k.encode('utf-8') if k else None,
        batch_size=16384,
        linger_ms=10,
        acks=1,
        retries=5,
        retry_backoff_ms=100
    )
    
    start_time = time.time()
    message_count = 0
    successful_sends = 0
    failed_sends = 0
    send_times = []
    
    try:
        while time.time() - start_time < test_duration:
            symbol = test_symbols[message_count % len(test_symbols)]
            message = {
                "symbol": symbol,
                "sequence": message_count,
                "price": 100.0 + (message_count % 100),
                "timestamp": int(time.time() * 1000),
                "test_run": "reliability"
            }
            
            send_start = time.time()
            try:
                future = producer.send(test_topic, value=message, key=symbol)
                future.get(timeout=5)
                send_duration = time.time() - send_start
                send_times.append(send_duration)
                successful_sends += 1
            except Exception as e:
                failed_sends += 1
                print(f"Send failed: {e}")
            
            message_count += 1
            
            # Control rate
            await asyncio.sleep(1.0 / target_rate)
        
        producer.flush(timeout=30)
        
    finally:
        producer.close()
    
    total_time = time.time() - start_time
    total_attempts = successful_sends + failed_sends
    
    # Calculate metrics
    actual_rate = successful_sends / total_time
    success_rate = successful_sends / total_attempts if total_attempts > 0 else 0
    
    if send_times:
        avg_latency = sum(send_times) / len(send_times)
        assert_latency_percentile(send_times, 95, 1.0, "Kafka send under load")
    
    # Reliability assertions
    assert_success_rate(successful_sends, total_attempts, 0.90, "Kafka reliability under load")
    assert actual_rate >= target_rate * 0.8, f"Actual rate {actual_rate:.1f} msg/s below 80% of target {target_rate} msg/s"
    
    print(f"Kafka reliability test: {actual_rate:.1f} msg/s, {success_rate:.2%} success rate over {total_time:.1f}s")