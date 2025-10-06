# HFT RTB - Ultra-Low-Latency Trading Engine

A Rust-based ultra-low-latency processing stack capable of ingesting market/auction data, applying decision logic, and issuing orders/bids within single-digit millisecond budgets.

## 🚀 Overview

This high-frequency trading (HFT) real-time bidding (RTB) engine is designed for ultra-low-latency market data processing and order execution. The system can process market ticks, apply trading strategies, and execute orders with sub-millisecond latency requirements.

### Key Performance Targets
- **Latency Budget**: Single-digit milliseconds end-to-end
- **Throughput**: 100K+ ticks/second per symbol
- **Memory**: Zero-copy data processing where possible
- **CPU**: SIMD-optimized computations for price normalization

## 🏗️ Architecture

### Data Flow
```
Network/Kafka → Gateway → Decoder → Strategy Engine → Order Router → Exchange/API
```

### Core Components

#### 1. Gateway Layer (`crates/gateway`)
- **TCP Ingest**: Raw feed capture using tokio framed sockets and Bytes for zero-copy slicing
- **Kafka Consumer**: Optional rdkafka consumer for replay or burst smoothing with offset management
- **Ports**: 
  - `7000`: TCP gateway for market data ingestion
  - `9000`: Prometheus metrics endpoint
  - `9100`: gRPC admin interface

#### 2. Feed Normalization (`crates/decoder`)
- Zero-copy deserialization into `Cow<'a, Tick>` via serde + bincode
- SIMD-assisted field transforms for price normalization batches
- Cache-aligned buffer management for optimal memory access

#### 3. Strategy Engine (`crates/engine_core`)
- **State Cache**: Lock-free ring buffers per symbol using arc-swap/crossbeam atomics
- **Strategy Plugin**: `Strategy::on_event(&Tick) -> Action` trait with dynamic dispatch
- **Core Affinity**: Dedicated core-affine tasks with `tokio::task::spawn_blocking`
- **Back-pressure**: Bounded MPSC channels sized to L3 cache multiples

#### 4. Order Router (`crates/order_router`)
- Async pipelines to venue adapters (FIX/HTTP)
- Tower middleware for retries/timeouts
- `tokio::time::timeout ≤ 2ms` for latency-critical paths
- Optional kernel-bypass sockets via `tokio-uring` feature

#### 5. Control Plane (`crates/control_plane`)
- **Metrics**: Prometheus exporter with latency histograms
- **Logging**: Structured logging via tracing with per-component spans
- **Admin API**: gRPC server (tonic) for runtime config updates and warm restarts

## 🛠️ Setup & Installation

### Prerequisites
- Rust 1.75+ (MSRV)
- Docker & Docker Compose (for containerized deployment)
- Linux/macOS (Windows support via WSL2)

### Local Development

1. **Clone the repository**:
```bash
git clone <repository-url>
cd hft_rtb
```

2. **Build the project**:
```bash
# Build all crates
cargo build --release

# Build specific components
cargo build --release --package engine
cargo build --release --package loadgen
```

3. **Run tests**:
```bash
# Unit tests
cargo test

# Integration tests
cargo test --test integration

# Benchmarks
cargo bench
```

4. **Start the engine**:
```bash
# With default configuration
cargo run --release --package engine

# With custom configuration
RUST_LOG=debug \
GATEWAY_ADDR=0.0.0.0:7000 \
METRICS_ADDR=0.0.0.0:9000 \
ADMIN_ADDR=0.0.0.0:9100 \
cargo run --release --package engine
```

### Docker Deployment

1. **Build the Docker image**:
```bash
docker build -f deploy/docker/engine.Dockerfile -t hft-rtb-engine .
```

2. **Run with Docker Compose**:
```bash
docker-compose up -d
```

This will start:
- HFT Engine
- Kafka cluster (for data replay/buffering)
- Prometheus (metrics collection)
- Grafana (metrics visualization)

## 🧪 Testing

### Load Testing

Use the included load generator to test system performance:

```bash
# Generate synthetic market data
cargo run --release --package loadgen -- \
  --target 127.0.0.1:7000 \
  --symbols AAPL,GOOGL,MSFT \
  --rate 10000 \
  --duration 60s

# With Kafka replay
cargo run --release --package loadgen -- \
  --kafka-brokers localhost:9092 \
  --topic market-data \
  --symbols AAPL,GOOGL,MSFT \
  --rate 50000
```

### Performance Benchmarks

```bash
# Decoder benchmarks
cargo bench --package decoder

# Engine core benchmarks  
cargo bench --package engine_core

# End-to-end latency benchmarks
cargo bench --bench e2e_latency
```

### Integration Testing

```bash
# Start test environment
docker-compose -f docker-compose.test.yml up -d

# Run integration tests
cargo test --test integration -- --test-threads=1

# Cleanup
docker-compose -f docker-compose.test.yml down
```

## 📊 Monitoring & Observability

### Metrics (Prometheus)
Access metrics at `http://localhost:9000/metrics`:

- `ingest_latency_histogram`: Ingestion latency distribution
- `strategy_execution_time`: Strategy processing time
- `order_dispatch_latency`: Order routing latency
- `throughput_ticks_per_second`: System throughput

### Grafana Dashboard
Access dashboard at `http://localhost:3000` (admin/admin):

- Real-time latency percentiles (P50, P95, P99)
- Throughput metrics per symbol
- Error rates and system health
- Resource utilization (CPU, memory)

### Structured Logging
```bash
# Enable debug logging
export RUST_LOG=debug

# Component-specific logging
export RUST_LOG=gateway=debug,engine_core=info,order_router=warn
```

### Admin API (gRPC)
```bash
# Update strategy parameters
grpcurl -plaintext \
  -d '{"strategy": "momentum", "params": {"threshold": 0.7}}' \
  localhost:9100 \
  control_plane.Admin/UpdateStrategy

# Get system status
grpcurl -plaintext localhost:9100 control_plane.Admin/GetStatus
```

## ⚙️ Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `GATEWAY_ADDR` | `0.0.0.0:7000` | TCP gateway bind address |
| `METRICS_ADDR` | `0.0.0.0:9000` | Prometheus metrics endpoint |
| `ADMIN_ADDR` | `0.0.0.0:9100` | gRPC admin interface |
| `RUST_LOG` | `info` | Logging level configuration |
| `STRATEGY_PARAMS` | `{}` | JSON strategy parameters |

### Strategy Configuration

The engine supports pluggable trading strategies. Current implementations:

- **MomentumStrategy**: Trend-following based on price momentum
- **MeanReversionStrategy**: Counter-trend trading strategy
- **ArbitrageStrategy**: Cross-venue arbitrage detection

Example strategy configuration:
```json
{
  "momentum": {
    "threshold": 0.5,
    "lookback_window": 100,
    "position_size": 1000
  }
}
```

## 🔧 Development

### Project Structure
```
├── crates/
│   ├── common/          # Shared types and utilities
│   ├── decoder/         # Market data deserialization
│   ├── gateway/         # Data ingestion layer
│   ├── engine_core/     # Strategy execution engine
│   ├── order_router/    # Order management and routing
│   └── control_plane/   # Admin API and metrics
├── services/
│   └── engine/          # Main application binary
├── tools/
│   └── loadgen/         # Load testing utility
└── deploy/
    ├── docker/          # Container definitions
    └── systemd/         # Service definitions
```

### Adding New Strategies

1. Implement the `Strategy` trait:
```rust
use engine_core::Strategy;
use common::{Tick, Action};

pub struct MyStrategy;

impl Strategy for MyStrategy {
    fn on_event(&self, tick: &Tick) -> Option<Action> {
        // Your strategy logic here
        None
    }
}
```

2. Register in the engine manager:
```rust
// In services/engine/src/main.rs
let strategy: Arc<dyn Strategy> = Arc::new(MyStrategy::new());
```

### Performance Optimization Tips

1. **CPU Affinity**: Pin critical threads to dedicated cores
2. **Memory**: Use `#[repr(C)]` for cache-friendly data layouts
3. **SIMD**: Leverage `std::simd` for batch operations
4. **Zero-Copy**: Prefer `Cow<'a, T>` and `Bytes` for data handling
5. **Lock-Free**: Use atomic operations and lock-free data structures

## 📈 Benchmarks

Typical performance on modern hardware (Intel Xeon, 32GB RAM):

| Metric | Value |
|--------|-------|
| Ingestion Latency (P99) | < 100μs |
| Strategy Execution (P99) | < 50μs |
| Order Dispatch (P99) | < 200μs |
| End-to-End Latency (P99) | < 500μs |
| Throughput | 100K+ ticks/sec |

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Run tests: `cargo test`
4. Run benchmarks: `cargo bench`
5. Commit changes: `git commit -m 'Add amazing feature'`
6. Push to branch: `git push origin feature/amazing-feature`
7. Open a Pull Request

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🔗 References

- [Tokio Async Runtime](https://tokio.rs/)
- [Tower Middleware](https://github.com/tower-rs/tower)
- [Prometheus Metrics](https://prometheus.io/)
- [gRPC/Tonic](https://github.com/hyperium/tonic)
- [SIMD Programming](https://doc.rust-lang.org/std/simd/)