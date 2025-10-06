# HFT RTB Integration Test Suite

This directory contains comprehensive integration tests for the HFT RTB (High-Frequency Trading Real-Time Bidding) system. The test suite includes both Rust and Python implementations with advanced reporting and metrics collection.

## Overview

The integration test suite validates:
- **System Health**: Service availability and responsiveness
- **API Functionality**: REST endpoints, request/response validation
- **Message Processing**: Kafka integration, message ordering, throughput
- **Performance**: Latency, throughput, concurrent load handling
- **Reliability**: Error handling, recovery, sustained operation

## Test Structure

```
tests/integration/
├── README.md                    # This file
├── test_runner.py              # Main test orchestrator with reporting
├── run_tests.sh               # Shell script for quick execution
├── Cargo.toml                 # Rust test dependencies
├── src/                       # Rust integration tests
│   ├── main.rs               # Rust test runner
│   ├── health_tests.rs       # Health check tests
│   ├── api_tests.rs          # API integration tests
│   ├── kafka_tests.rs        # Kafka messaging tests
│   └── performance_tests.rs  # Performance benchmarks
├── python/                    # Python integration tests
│   ├── requirements.txt      # Python dependencies
│   ├── conftest.py          # Pytest configuration and fixtures
│   ├── test_health.py       # Health check tests
│   ├── test_api.py          # API integration tests
│   ├── test_kafka.py        # Kafka messaging tests
│   └── test_performance.py  # Performance tests
├── config/                    # Test configurations
│   └── test_config.yaml     # Environment-specific settings
├── data/                      # Test data files
│   ├── sample_market_data.json
│   └── sample_orders.json
└── reports/                   # Generated test reports
    ├── *.html               # HTML reports
    ├── *.json               # JSON reports
    └── *.xml                # JUnit XML reports
```

## Prerequisites

### System Requirements
- **Rust**: 1.70+ with Cargo
- **Python**: 3.8+ with pip
- **Services**: HFT RTB system, Kafka, Redis running locally or accessible

### Service Dependencies
The tests expect the following services to be running:

1. **HFT RTB Services**:
   - Gateway API: `http://localhost:8080`
   - Metrics API: `http://localhost:9090`
   - Admin API: `http://localhost:9091`

2. **Kafka**: `localhost:9092`
3. **Redis**: `localhost:6379`

## Quick Start

### 1. Using the Shell Script (Recommended)

```bash
# Run all tests
./run_tests.sh

# Run specific test types
./run_tests.sh health          # Health checks only
./run_tests.sh quick           # Fast tests only
./run_tests.sh performance     # Performance tests
./run_tests.sh api             # API tests only
./run_tests.sh kafka           # Kafka tests only

# Skip service checks (if services are known to be running)
./run_tests.sh --skip-services all

# Check services only
./run_tests.sh --check-only
```

### 2. Using the Python Test Runner

```bash
# Install Python dependencies first
cd python && python -m venv venv && source venv/bin/activate
pip install -r requirements.txt

# Run tests with the Python runner
./test_runner.py all                    # All tests
./test_runner.py quick performance      # Multiple test types
./test_runner.py --env staging api      # Specific environment
./test_runner.py --skip-services load   # Skip service checks
```

### 3. Running Individual Test Suites

#### Rust Tests
```bash
# Build and run Rust tests
cargo build --release
cargo run --release -- --test all --duration 30
```

#### Python Tests
```bash
cd python
source venv/bin/activate

# Run all tests
pytest -v

# Run specific test categories
pytest -v -m "health"          # Health tests only
pytest -v -m "api"             # API tests only
pytest -v -m "performance"     # Performance tests only
pytest -v -m "not slow"        # Exclude slow tests
```

## Test Categories

### Health Tests (`health`)
- Service availability checks
- Health endpoint validation
- Metrics endpoint verification
- Service startup time measurement
- Concurrent health check reliability

### API Tests (`api`)
- Endpoint discovery and validation
- Order submission and status queries
- Position and symbol data retrieval
- Error handling for invalid requests
- Content type handling (JSON, form data)
- Concurrent API request handling

### Kafka Tests (`kafka`)
- Producer/consumer connectivity
- Topic operations (create, list, delete)
- Message production and consumption
- Message ordering within partitions
- Consumer group functionality
- Throughput and latency measurement
- Error handling for invalid topics/brokers

### Performance Tests (`performance`)
- Baseline performance measurement
- Concurrent request scaling
- Message processing latency (end-to-end)
- Throughput scaling under load
- Memory usage monitoring
- Sustained performance over time

### Load Tests (`load`)
- High-volume message processing
- Sustained concurrent connections
- Resource utilization under stress
- System stability over extended periods

## Configuration

### Environment Variables
```bash
# HFT Service Configuration
export HFT_HOST=localhost
export HFT_GATEWAY_PORT=8080
export HFT_METRICS_PORT=9090
export HFT_ADMIN_PORT=9091

# Kafka Configuration
export KAFKA_BROKERS=localhost:9092

# Redis Configuration
export REDIS_URL=redis://localhost:6379

# Test Environment
export TEST_ENV=development  # default, development, staging, production
```

### Configuration File
Edit `config/test_config.yaml` to customize test parameters:

```yaml
default:
  hft:
    host: localhost
    gateway_port: 8080
    metrics_port: 9090
    admin_port: 9091
  kafka:
    brokers: localhost:9092
  redis:
    url: redis://localhost:6379
  test_params:
    default_timeout: 30
    performance:
      concurrent_users: [1, 5, 10, 20]
      test_duration: 60
    load:
      message_rate: 1000
      test_duration: 300
```

## Test Reports

The test suite generates comprehensive reports in multiple formats:

### HTML Reports
- **Location**: `reports/integration_test_report_YYYYMMDD_HHMMSS.html`
- **Content**: Interactive dashboard with test results, metrics, and visualizations
- **Features**: Test suite summaries, detailed results, system metrics

### JSON Reports
- **Location**: `reports/integration_test_report_YYYYMMDD_HHMMSS.json`
- **Content**: Machine-readable test results and metrics
- **Use Cases**: CI/CD integration, automated analysis

### JUnit XML Reports
- **Location**: `reports/pytest_YYYYMMDD_HHMMSS_*.xml`
- **Content**: Standard JUnit format for CI/CD systems
- **Integration**: Jenkins, GitLab CI, GitHub Actions

## Performance Benchmarks

### Expected Performance Targets

| Metric | Target | Test Category |
|--------|--------|---------------|
| Health Check Response | < 100ms | Health |
| API Response Time (P95) | < 500ms | API |
| Order Processing Latency | < 10ms | Performance |
| Message Throughput | > 10,000 msg/s | Kafka |
| Concurrent Users | 100+ | Load |
| Success Rate | > 99.9% | All |

### Performance Test Scenarios

1. **Baseline Performance**: Single-user response times
2. **Concurrent Load**: 1-100 concurrent users
3. **Message Processing**: End-to-end latency measurement
4. **Throughput Scaling**: Message rate scaling (100-10,000 msg/s)
5. **Sustained Load**: 5-minute continuous operation
6. **Memory Stability**: Memory usage monitoring under load

## Troubleshooting

### Common Issues

#### Service Connection Failures
```bash
# Check if services are running
curl http://localhost:8080/health
curl http://localhost:9090/metrics

# Check Kafka
kafka-topics.sh --bootstrap-server localhost:9092 --list

# Check Redis
redis-cli ping
```

#### Python Dependencies
```bash
# Reinstall dependencies
cd python
rm -rf venv
python -m venv venv
source venv/bin/activate
pip install -r requirements.txt
```

#### Rust Build Issues
```bash
# Clean and rebuild
cargo clean
cargo build --release
```

### Test Failures

#### Timeout Issues
- Increase timeout values in `config/test_config.yaml`
- Check system resource availability
- Verify network connectivity

#### Performance Issues
- Check system load and available resources
- Verify no other heavy processes are running
- Consider adjusting performance targets

#### Kafka Issues
- Verify Kafka topics exist and are accessible
- Check Kafka broker configuration
- Ensure sufficient disk space for Kafka logs

## CI/CD Integration

### GitHub Actions Example
```yaml
name: Integration Tests
on: [push, pull_request]

jobs:
  integration-tests:
    runs-on: ubuntu-latest
    services:
      kafka:
        image: confluentinc/cp-kafka:latest
        ports:
          - 9092:9092
      redis:
        image: redis:latest
        ports:
          - 6379:6379
    
    steps:
      - uses: actions/checkout@v2
      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Setup Python
        uses: actions/setup-python@v2
        with:
          python-version: 3.9
      - name: Start HFT Services
        run: |
          # Start your HFT services here
          ./start_services.sh
      - name: Run Integration Tests
        run: |
          cd tests/integration
          ./test_runner.py --skip-services quick
      - name: Upload Test Reports
        uses: actions/upload-artifact@v2
        with:
          name: test-reports
          path: tests/integration/reports/
```

### Jenkins Pipeline Example
```groovy
pipeline {
    agent any
    stages {
        stage('Setup') {
            steps {
                sh 'docker-compose up -d kafka redis'
                sh './start_hft_services.sh'
            }
        }
        stage('Integration Tests') {
            steps {
                dir('tests/integration') {
                    sh './test_runner.py all'
                }
            }
            post {
                always {
                    publishHTML([
                        allowMissing: false,
                        alwaysLinkToLastBuild: true,
                        keepAll: true,
                        reportDir: 'tests/integration/reports',
                        reportFiles: '*.html',
                        reportName: 'Integration Test Report'
                    ])
                    junit 'tests/integration/reports/*.xml'
                }
            }
        }
    }
}
```

## Contributing

### Adding New Tests

1. **Rust Tests**: Add to appropriate module in `src/`
2. **Python Tests**: Add to appropriate test file in `python/`
3. **Test Data**: Add sample data to `data/`
4. **Configuration**: Update `config/test_config.yaml` if needed

### Test Naming Conventions

- **Rust**: `test_feature_scenario`
- **Python**: `test_feature_scenario`
- **Markers**: Use pytest markers for categorization

### Performance Test Guidelines

1. Always include baseline measurements
2. Test with realistic data volumes
3. Measure both latency and throughput
4. Include error rate monitoring
5. Test sustained performance over time

## Support

For issues or questions:
1. Check the troubleshooting section above
2. Review test logs in the reports directory
3. Verify service configurations and connectivity
4. Check system resources and dependencies

## License

This test suite is part of the HFT RTB project and follows the same licensing terms.