#!/bin/bash

# HFT RTB Integration Test Runner
# This script provides a unified interface to run all integration tests

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default configuration
DEFAULT_HFT_HOST="localhost"
DEFAULT_HFT_GATEWAY_PORT="8080"
DEFAULT_HFT_METRICS_PORT="9090"
DEFAULT_HFT_ADMIN_PORT="9091"
DEFAULT_KAFKA_BROKERS="localhost:9092"
DEFAULT_REDIS_URL="redis://localhost:6379"

# Test configuration
HFT_HOST=${HFT_HOST:-$DEFAULT_HFT_HOST}
HFT_GATEWAY_PORT=${HFT_GATEWAY_PORT:-$DEFAULT_HFT_GATEWAY_PORT}
HFT_METRICS_PORT=${HFT_METRICS_PORT:-$DEFAULT_HFT_METRICS_PORT}
HFT_ADMIN_PORT=${HFT_ADMIN_PORT:-$DEFAULT_HFT_ADMIN_PORT}
KAFKA_BROKERS=${KAFKA_BROKERS:-$DEFAULT_KAFKA_BROKERS}
REDIS_URL=${REDIS_URL:-$DEFAULT_REDIS_URL}

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Test directories
RUST_TEST_DIR="$SCRIPT_DIR"
PYTHON_TEST_DIR="$SCRIPT_DIR/python"

# Output directory for reports
REPORTS_DIR="$SCRIPT_DIR/reports"
mkdir -p "$REPORTS_DIR"

# Timestamp for this test run
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
REPORT_PREFIX="integration_test_$TIMESTAMP"

# Function to print colored output
print_status() {
    local color=$1
    local message=$2
    echo -e "${color}${message}${NC}"
}

print_header() {
    echo
    print_status "$BLUE" "=================================================="
    print_status "$BLUE" "$1"
    print_status "$BLUE" "=================================================="
    echo
}

print_success() {
    print_status "$GREEN" "✓ $1"
}

print_error() {
    print_status "$RED" "✗ $1"
}

print_warning() {
    print_status "$YELLOW" "⚠ $1"
}

print_info() {
    print_status "$BLUE" "ℹ $1"
}

# Function to check if a service is running
check_service() {
    local service_name=$1
    local host=$2
    local port=$3
    local timeout=${4:-10}
    
    print_info "Checking $service_name at $host:$port..."
    
    if timeout $timeout bash -c "echo >/dev/tcp/$host/$port" 2>/dev/null; then
        print_success "$service_name is running"
        return 0
    else
        print_error "$service_name is not accessible at $host:$port"
        return 1
    fi
}

# Function to check all required services
check_services() {
    print_header "Checking Required Services"
    
    local all_services_ok=true
    
    # Check HFT services
    if ! check_service "HFT Gateway" "$HFT_HOST" "$HFT_GATEWAY_PORT"; then
        all_services_ok=false
    fi
    
    if ! check_service "HFT Metrics" "$HFT_HOST" "$HFT_METRICS_PORT"; then
        all_services_ok=false
    fi
    
    if ! check_service "HFT Admin" "$HFT_HOST" "$HFT_ADMIN_PORT"; then
        all_services_ok=false
    fi
    
    # Check Kafka
    local kafka_host=$(echo "$KAFKA_BROKERS" | cut -d: -f1)
    local kafka_port=$(echo "$KAFKA_BROKERS" | cut -d: -f2)
    if ! check_service "Kafka" "$kafka_host" "$kafka_port"; then
        all_services_ok=false
    fi
    
    # Check Redis
    local redis_host=$(echo "$REDIS_URL" | sed 's|redis://||' | cut -d: -f1)
    local redis_port=$(echo "$REDIS_URL" | sed 's|redis://||' | cut -d: -f2 | cut -d/ -f1)
    redis_port=${redis_port:-6379}
    if ! check_service "Redis" "$redis_host" "$redis_port"; then
        all_services_ok=false
    fi
    
    if [ "$all_services_ok" = true ]; then
        print_success "All required services are running"
        return 0
    else
        print_error "Some required services are not running"
        return 1
    fi
}

# Function to run Rust integration tests
run_rust_tests() {
    print_header "Running Rust Integration Tests"
    
    if [ ! -f "$RUST_TEST_DIR/Cargo.toml" ]; then
        print_warning "Rust integration tests not found, skipping..."
        return 0
    fi
    
    cd "$RUST_TEST_DIR"
    
    local rust_report="$REPORTS_DIR/${REPORT_PREFIX}_rust.json"
    local rust_log="$REPORTS_DIR/${REPORT_PREFIX}_rust.log"
    
    print_info "Building Rust integration tests..."
    if ! cargo build --release > "$rust_log" 2>&1; then
        print_error "Failed to build Rust integration tests"
        cat "$rust_log"
        return 1
    fi
    
    print_info "Running Rust integration tests..."
    local rust_cmd="cargo run --release -- \
        --host $HFT_HOST \
        --gateway-port $HFT_GATEWAY_PORT \
        --metrics-port $HFT_METRICS_PORT \
        --admin-port $HFT_ADMIN_PORT \
        --kafka-brokers $KAFKA_BROKERS \
        --redis-url $REDIS_URL \
        --test all \
        --duration 30"
    
    if $rust_cmd >> "$rust_log" 2>&1; then
        print_success "Rust integration tests passed"
        return 0
    else
        print_error "Rust integration tests failed"
        tail -20 "$rust_log"
        return 1
    fi
}

# Function to run Python integration tests
run_python_tests() {
    print_header "Running Python Integration Tests"
    
    if [ ! -f "$PYTHON_TEST_DIR/requirements.txt" ]; then
        print_warning "Python integration tests not found, skipping..."
        return 0
    fi
    
    cd "$PYTHON_TEST_DIR"
    
    # Check if virtual environment exists, create if not
    if [ ! -d "venv" ]; then
        print_info "Creating Python virtual environment..."
        python3 -m venv venv
    fi
    
    # Activate virtual environment
    source venv/bin/activate
    
    # Install dependencies
    print_info "Installing Python dependencies..."
    pip install -q -r requirements.txt
    
    # Set environment variables for tests
    export HFT_HOST="$HFT_HOST"
    export HFT_GATEWAY_PORT="$HFT_GATEWAY_PORT"
    export HFT_METRICS_PORT="$HFT_METRICS_PORT"
    export HFT_ADMIN_PORT="$HFT_ADMIN_PORT"
    export KAFKA_BROKERS="$KAFKA_BROKERS"
    export REDIS_URL="$REDIS_URL"
    
    local python_report="$REPORTS_DIR/${REPORT_PREFIX}_python.xml"
    local python_log="$REPORTS_DIR/${REPORT_PREFIX}_python.log"
    local python_html="$REPORTS_DIR/${REPORT_PREFIX}_python.html"
    
    print_info "Running Python integration tests..."
    
    # Run tests with different markers based on arguments
    local pytest_args=""
    case "${1:-all}" in
        "quick")
            pytest_args="-m 'not slow and not load'"
            ;;
        "performance")
            pytest_args="-m 'performance'"
            ;;
        "load")
            pytest_args="-m 'load'"
            ;;
        "health")
            pytest_args="-m 'health'"
            ;;
        "api")
            pytest_args="-m 'api'"
            ;;
        "kafka")
            pytest_args="-m 'kafka'"
            ;;
        *)
            pytest_args=""
            ;;
    esac
    
    local pytest_cmd="pytest -v $pytest_args \
        --junit-xml=$python_report \
        --html=$python_html \
        --self-contained-html \
        --tb=short \
        --durations=10"
    
    if $pytest_cmd > "$python_log" 2>&1; then
        print_success "Python integration tests passed"
        deactivate
        return 0
    else
        print_error "Python integration tests failed"
        tail -20 "$python_log"
        deactivate
        return 1
    fi
}

# Function to run health checks only
run_health_checks() {
    print_header "Running Health Checks"
    
    # Quick health check using curl
    local health_url="http://$HFT_HOST:$HFT_GATEWAY_PORT/health"
    local metrics_url="http://$HFT_HOST:$HFT_METRICS_PORT/metrics"
    
    print_info "Checking health endpoint..."
    if curl -s -f "$health_url" > /dev/null; then
        print_success "Health endpoint is responding"
    else
        print_error "Health endpoint is not responding"
        return 1
    fi
    
    print_info "Checking metrics endpoint..."
    if curl -s -f "$metrics_url" > /dev/null; then
        print_success "Metrics endpoint is responding"
    else
        print_error "Metrics endpoint is not responding"
        return 1
    fi
    
    # Run Python health tests if available
    if [ -f "$PYTHON_TEST_DIR/test_health.py" ]; then
        cd "$PYTHON_TEST_DIR"
        if [ -d "venv" ]; then
            source venv/bin/activate
            export HFT_HOST="$HFT_HOST"
            export HFT_GATEWAY_PORT="$HFT_GATEWAY_PORT"
            export HFT_METRICS_PORT="$HFT_METRICS_PORT"
            
            if pytest -v test_health.py -q; then
                print_success "Detailed health checks passed"
                deactivate
            else
                print_error "Detailed health checks failed"
                deactivate
                return 1
            fi
        fi
    fi
    
    return 0
}

# Function to generate summary report
generate_summary() {
    print_header "Test Summary Report"
    
    local summary_file="$REPORTS_DIR/${REPORT_PREFIX}_summary.txt"
    
    {
        echo "HFT RTB Integration Test Summary"
        echo "================================"
        echo "Timestamp: $(date)"
        echo "Configuration:"
        echo "  HFT Host: $HFT_HOST"
        echo "  Gateway Port: $HFT_GATEWAY_PORT"
        echo "  Metrics Port: $HFT_METRICS_PORT"
        echo "  Admin Port: $HFT_ADMIN_PORT"
        echo "  Kafka Brokers: $KAFKA_BROKERS"
        echo "  Redis URL: $REDIS_URL"
        echo ""
        
        # Check for test reports
        if [ -f "$REPORTS_DIR/${REPORT_PREFIX}_rust.log" ]; then
            echo "Rust Tests: Available"
        else
            echo "Rust Tests: Not run"
        fi
        
        if [ -f "$REPORTS_DIR/${REPORT_PREFIX}_python.xml" ]; then
            echo "Python Tests: Available"
            # Extract test results from XML if possible
            if command -v xmllint > /dev/null; then
                local python_xml="$REPORTS_DIR/${REPORT_PREFIX}_python.xml"
                local tests=$(xmllint --xpath "//testsuite/@tests" "$python_xml" 2>/dev/null | sed 's/tests="//;s/"//')
                local failures=$(xmllint --xpath "//testsuite/@failures" "$python_xml" 2>/dev/null | sed 's/failures="//;s/"//')
                local errors=$(xmllint --xpath "//testsuite/@errors" "$python_xml" 2>/dev/null | sed 's/errors="//;s/"//')
                
                if [ -n "$tests" ]; then
                    echo "  Total Tests: $tests"
                    echo "  Failures: ${failures:-0}"
                    echo "  Errors: ${errors:-0}"
                fi
            fi
        else
            echo "Python Tests: Not run"
        fi
        
        echo ""
        echo "Report files generated in: $REPORTS_DIR"
        ls -la "$REPORTS_DIR"/${REPORT_PREFIX}_* 2>/dev/null || echo "No report files found"
        
    } | tee "$summary_file"
    
    print_info "Summary report saved to: $summary_file"
}

# Function to show usage
show_usage() {
    cat << EOF
HFT RTB Integration Test Runner

Usage: $0 [OPTIONS] [TEST_TYPE]

TEST_TYPE:
    all         Run all integration tests (default)
    health      Run health checks only
    quick       Run quick tests (exclude slow/load tests)
    performance Run performance tests only
    load        Run load tests only
    api         Run API tests only
    kafka       Run Kafka tests only
    rust        Run Rust tests only
    python      Run Python tests only

OPTIONS:
    -h, --help              Show this help message
    -c, --check-services    Check services only, don't run tests
    -s, --skip-services     Skip service checks
    -v, --verbose           Verbose output
    --host HOST             HFT host (default: $DEFAULT_HFT_HOST)
    --gateway-port PORT     Gateway port (default: $DEFAULT_HFT_GATEWAY_PORT)
    --metrics-port PORT     Metrics port (default: $DEFAULT_HFT_METRICS_PORT)
    --admin-port PORT       Admin port (default: $DEFAULT_HFT_ADMIN_PORT)
    --kafka-brokers BROKERS Kafka brokers (default: $DEFAULT_KAFKA_BROKERS)
    --redis-url URL         Redis URL (default: $DEFAULT_REDIS_URL)

Environment Variables:
    HFT_HOST, HFT_GATEWAY_PORT, HFT_METRICS_PORT, HFT_ADMIN_PORT
    KAFKA_BROKERS, REDIS_URL

Examples:
    $0                      # Run all tests
    $0 health               # Run health checks only
    $0 quick                # Run quick tests
    $0 --host 192.168.1.100 # Run tests against remote host
    $0 -c                   # Check services only

EOF
}

# Parse command line arguments
SKIP_SERVICES=false
CHECK_SERVICES_ONLY=false
VERBOSE=false
TEST_TYPE="all"

while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_usage
            exit 0
            ;;
        -c|--check-services)
            CHECK_SERVICES_ONLY=true
            shift
            ;;
        -s|--skip-services)
            SKIP_SERVICES=true
            shift
            ;;
        -v|--verbose)
            VERBOSE=true
            shift
            ;;
        --host)
            HFT_HOST="$2"
            shift 2
            ;;
        --gateway-port)
            HFT_GATEWAY_PORT="$2"
            shift 2
            ;;
        --metrics-port)
            HFT_METRICS_PORT="$2"
            shift 2
            ;;
        --admin-port)
            HFT_ADMIN_PORT="$2"
            shift 2
            ;;
        --kafka-brokers)
            KAFKA_BROKERS="$2"
            shift 2
            ;;
        --redis-url)
            REDIS_URL="$2"
            shift 2
            ;;
        all|health|quick|performance|load|api|kafka|rust|python)
            TEST_TYPE="$1"
            shift
            ;;
        *)
            print_error "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
done

# Main execution
main() {
    print_header "HFT RTB Integration Test Runner"
    
    print_info "Test configuration:"
    print_info "  Host: $HFT_HOST"
    print_info "  Gateway Port: $HFT_GATEWAY_PORT"
    print_info "  Metrics Port: $HFT_METRICS_PORT"
    print_info "  Admin Port: $HFT_ADMIN_PORT"
    print_info "  Kafka Brokers: $KAFKA_BROKERS"
    print_info "  Redis URL: $REDIS_URL"
    print_info "  Test Type: $TEST_TYPE"
    print_info "  Reports Directory: $REPORTS_DIR"
    
    # Check services unless skipped
    if [ "$SKIP_SERVICES" = false ]; then
        if ! check_services; then
            print_error "Service checks failed. Use -s to skip service checks."
            exit 1
        fi
    fi
    
    # If only checking services, exit here
    if [ "$CHECK_SERVICES_ONLY" = true ]; then
        print_success "Service checks completed successfully"
        exit 0
    fi
    
    # Track overall test results
    local overall_success=true
    
    # Run tests based on type
    case "$TEST_TYPE" in
        "health")
            if ! run_health_checks; then
                overall_success=false
            fi
            ;;
        "rust")
            if ! run_rust_tests; then
                overall_success=false
            fi
            ;;
        "python")
            if ! run_python_tests; then
                overall_success=false
            fi
            ;;
        "quick"|"performance"|"load"|"api"|"kafka")
            if ! run_python_tests "$TEST_TYPE"; then
                overall_success=false
            fi
            ;;
        "all")
            if ! run_health_checks; then
                overall_success=false
            fi
            if ! run_rust_tests; then
                overall_success=false
            fi
            if ! run_python_tests; then
                overall_success=false
            fi
            ;;
    esac
    
    # Generate summary report
    generate_summary
    
    # Final result
    if [ "$overall_success" = true ]; then
        print_success "All integration tests completed successfully!"
        exit 0
    else
        print_error "Some integration tests failed!"
        exit 1
    fi
}

# Run main function
main "$@"