#!/bin/bash

# HFT RTB Testing Script
# This script provides various testing scenarios for the HFT RTB engine

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_COMPOSE_FILE="$PROJECT_ROOT/docker-compose.test.yml"
MAIN_COMPOSE_FILE="$PROJECT_ROOT/docker-compose.yml"

# Logging function
log() {
    echo -e "${BLUE}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# Help function
show_help() {
    cat << EOF
HFT RTB Testing Script

Usage: $0 [COMMAND] [OPTIONS]

Commands:
    unit                Run unit tests
    integration         Run integration tests
    benchmark           Run performance benchmarks
    load-test           Run load testing with Docker
    e2e                 Run end-to-end tests
    docker-test         Run full Docker-based testing
    clean               Clean up test artifacts
    help                Show this help message

Options:
    --verbose           Enable verbose output
    --duration SECS     Set test duration (default: 60)
    --rate NUM          Set load test rate (default: 1000)
    --symbols LIST      Comma-separated symbol list (default: TEST1,TEST2,TEST3)

Examples:
    $0 unit                                 # Run unit tests
    $0 load-test --duration 120 --rate 5000 # Load test for 2 minutes at 5K TPS
    $0 e2e --verbose                        # End-to-end test with verbose output
    $0 docker-test                          # Full Docker environment test

EOF
}

# Parse command line arguments
COMMAND=""
VERBOSE=false
DURATION=60
RATE=1000
SYMBOLS="TEST1,TEST2,TEST3"

while [[ $# -gt 0 ]]; do
    case $1 in
        unit|integration|benchmark|load-test|e2e|docker-test|clean|help)
            COMMAND="$1"
            shift
            ;;
        --verbose)
            VERBOSE=true
            shift
            ;;
        --duration)
            DURATION="$2"
            shift 2
            ;;
        --rate)
            RATE="$2"
            shift 2
            ;;
        --symbols)
            SYMBOLS="$2"
            shift 2
            ;;
        *)
            error "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
done

# Set verbose mode
if [ "$VERBOSE" = true ]; then
    set -x
fi

# Unit tests
run_unit_tests() {
    log "Running unit tests..."
    cd "$PROJECT_ROOT"
    
    if cargo test --workspace --lib; then
        success "Unit tests passed"
    else
        error "Unit tests failed"
        return 1
    fi
}

# Integration tests
run_integration_tests() {
    log "Running integration tests..."
    cd "$PROJECT_ROOT"
    
    if cargo test --workspace --test '*'; then
        success "Integration tests passed"
    else
        error "Integration tests failed"
        return 1
    fi
}

# Benchmark tests
run_benchmarks() {
    log "Running performance benchmarks..."
    cd "$PROJECT_ROOT"
    
    log "Running decoder benchmarks..."
    cargo bench --package decoder
    
    log "Running engine core benchmarks..."
    cargo bench --package engine_core
    
    log "Running end-to-end latency benchmarks..."
    if [ -f "benches/e2e_latency.rs" ]; then
        cargo bench --bench e2e_latency
    else
        warning "End-to-end benchmark not found, skipping..."
    fi
    
    success "Benchmarks completed"
}

# Load testing
run_load_test() {
    log "Running load test (Rate: $RATE, Duration: ${DURATION}s, Symbols: $SYMBOLS)..."
    
    # Start test environment
    log "Starting test environment..."
    docker-compose -f "$TEST_COMPOSE_FILE" up -d
    
    # Wait for services to be healthy
    log "Waiting for services to be ready..."
    sleep 30
    
    # Check if engine is healthy
    if ! docker-compose -f "$TEST_COMPOSE_FILE" exec -T hft-engine-test curl -f http://localhost:9000/metrics > /dev/null 2>&1; then
        error "HFT Engine is not healthy"
        docker-compose -f "$TEST_COMPOSE_FILE" logs hft-engine-test
        return 1
    fi
    
    # Run load test
    log "Starting load generation..."
    docker-compose -f "$TEST_COMPOSE_FILE" run --rm \
        -e TARGET_HOST=hft-engine-test:7000 \
        -e RATE="$RATE" \
        -e DURATION="${DURATION}s" \
        -e SYMBOLS="$SYMBOLS" \
        loadgen-test
    
    # Collect metrics
    log "Collecting metrics..."
    docker-compose -f "$TEST_COMPOSE_FILE" exec -T hft-engine-test curl -s http://localhost:9000/metrics > "test_metrics_$(date +%Y%m%d_%H%M%S).txt"
    
    success "Load test completed"
}

# End-to-end testing
run_e2e_tests() {
    log "Running end-to-end tests..."
    
    # Build the project
    log "Building project..."
    cd "$PROJECT_ROOT"
    cargo build --release
    
    # Start minimal services
    log "Starting services..."
    docker-compose -f "$TEST_COMPOSE_FILE" up -d kafka-test zookeeper-test
    
    # Wait for Kafka
    log "Waiting for Kafka to be ready..."
    sleep 20
    
    # Start the engine
    log "Starting HFT engine..."
    RUST_LOG=info \
    GATEWAY_ADDR=127.0.0.1:17000 \
    METRICS_ADDR=127.0.0.1:19000 \
    ADMIN_ADDR=127.0.0.1:19100 \
    KAFKA_BROKERS=localhost:19092 \
    cargo run --release --package engine &
    ENGINE_PID=$!
    
    # Wait for engine to start
    sleep 10
    
    # Test TCP ingestion
    log "Testing TCP ingestion..."
    echo "Testing TCP connection..." | nc -w 1 127.0.0.1 17000 || warning "TCP test failed"
    
    # Test metrics endpoint
    log "Testing metrics endpoint..."
    if curl -f http://127.0.0.1:19000/metrics > /dev/null 2>&1; then
        success "Metrics endpoint is working"
    else
        error "Metrics endpoint failed"
    fi
    
    # Test admin gRPC (if grpcurl is available)
    if command -v grpcurl &> /dev/null; then
        log "Testing admin gRPC endpoint..."
        if grpcurl -plaintext 127.0.0.1:19100 list > /dev/null 2>&1; then
            success "Admin gRPC endpoint is working"
        else
            warning "Admin gRPC endpoint test failed"
        fi
    else
        warning "grpcurl not found, skipping gRPC test"
    fi
    
    # Cleanup
    log "Cleaning up..."
    kill $ENGINE_PID 2>/dev/null || true
    docker-compose -f "$TEST_COMPOSE_FILE" down
    
    success "End-to-end tests completed"
}

# Docker-based testing
run_docker_test() {
    log "Running full Docker-based testing..."
    
    # Clean up any existing containers
    docker-compose -f "$TEST_COMPOSE_FILE" down -v 2>/dev/null || true
    
    # Build images
    log "Building Docker images..."
    docker-compose -f "$TEST_COMPOSE_FILE" build
    
    # Start services
    log "Starting test environment..."
    docker-compose -f "$TEST_COMPOSE_FILE" up -d
    
    # Wait for all services to be healthy
    log "Waiting for services to be healthy..."
    sleep 60
    
    # Check service health
    log "Checking service health..."
    
    # Check Kafka
    if docker-compose -f "$TEST_COMPOSE_FILE" exec -T kafka-test kafka-broker-api-versions --bootstrap-server localhost:19092 > /dev/null 2>&1; then
        success "Kafka is healthy"
    else
        error "Kafka health check failed"
        docker-compose -f "$TEST_COMPOSE_FILE" logs kafka-test
        return 1
    fi
    
    # Check HFT Engine
    if docker-compose -f "$TEST_COMPOSE_FILE" exec -T hft-engine-test curl -f http://localhost:9000/metrics > /dev/null 2>&1; then
        success "HFT Engine is healthy"
    else
        error "HFT Engine health check failed"
        docker-compose -f "$TEST_COMPOSE_FILE" logs hft-engine-test
        return 1
    fi
    
    # Run load test
    log "Running integrated load test..."
    docker-compose -f "$TEST_COMPOSE_FILE" run --rm loadgen-test
    
    # Collect final metrics
    log "Collecting final metrics..."
    docker-compose -f "$TEST_COMPOSE_FILE" exec -T hft-engine-test curl -s http://localhost:9000/metrics > "docker_test_metrics_$(date +%Y%m%d_%H%M%S).txt"
    
    # Show logs
    if [ "$VERBOSE" = true ]; then
        log "Service logs:"
        docker-compose -f "$TEST_COMPOSE_FILE" logs
    fi
    
    success "Docker-based testing completed"
}

# Cleanup function
cleanup() {
    log "Cleaning up test artifacts..."
    
    # Stop Docker containers
    docker-compose -f "$TEST_COMPOSE_FILE" down -v 2>/dev/null || true
    docker-compose -f "$MAIN_COMPOSE_FILE" down -v 2>/dev/null || true
    
    # Clean Cargo artifacts
    cd "$PROJECT_ROOT"
    cargo clean
    
    # Remove test metrics files older than 7 days
    find . -name "*_metrics_*.txt" -mtime +7 -delete 2>/dev/null || true
    
    success "Cleanup completed"
}

# Main execution
main() {
    case "$COMMAND" in
        unit)
            run_unit_tests
            ;;
        integration)
            run_integration_tests
            ;;
        benchmark)
            run_benchmarks
            ;;
        load-test)
            run_load_test
            ;;
        e2e)
            run_e2e_tests
            ;;
        docker-test)
            run_docker_test
            ;;
        clean)
            cleanup
            ;;
        help|"")
            show_help
            ;;
        *)
            error "Unknown command: $COMMAND"
            show_help
            exit 1
            ;;
    esac
}

# Trap cleanup on exit
trap 'cleanup' EXIT

# Run main function
main