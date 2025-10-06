# HFT RTB Makefile
# Provides convenient commands for building, testing, and running the system

.PHONY: help build test bench clean docker run setup check fmt clippy

# Default target
help: ## Show this help message
	@echo "HFT RTB - Ultra-Low-Latency Trading Engine"
	@echo ""
	@echo "Available targets:"
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  %-15s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# Build targets
build: ## Build all packages in release mode
	cargo build --release --workspace

build-debug: ## Build all packages in debug mode
	cargo build --workspace

engine: ## Build only the engine binary
	cargo build --release --package engine

loadgen: ## Build only the load generator
	cargo build --release --package loadgen

# Test targets
test: ## Run all tests
	cargo test --workspace

test-unit: ## Run unit tests only
	cargo test --workspace --lib

test-integration: ## Run integration tests only
	cargo test --workspace --test '*'

test-docker: ## Run tests in Docker environment
	./scripts/test.sh docker-test

# Benchmark targets
bench: ## Run all benchmarks
	cargo bench --workspace

bench-decoder: ## Run decoder benchmarks
	cargo bench --package decoder

bench-engine: ## Run engine core benchmarks
	cargo bench --package engine_core

# Quality targets
fmt: ## Format code
	cargo fmt --all

fmt-check: ## Check code formatting
	cargo fmt --all -- --check

clippy: ## Run clippy lints
	cargo clippy --workspace -- -D warnings

clippy-fix: ## Fix clippy issues automatically
	cargo clippy --workspace --fix --allow-dirty --allow-staged

audit: ## Security audit
	cargo audit

outdated: ## Check for outdated dependencies
	cargo outdated

# Setup targets
setup: ## Run setup script
	./scripts/setup.sh all

setup-dev: ## Setup development environment only
	./scripts/setup.sh dev

check-deps: ## Check system dependencies
	./scripts/setup.sh check

# Docker targets
docker-build: ## Build Docker images
	docker build -f deploy/docker/engine.Dockerfile -t hft-rtb-engine .
	docker build -f deploy/docker/loadgen.Dockerfile -t hft-rtb-loadgen .

docker-up: ## Start Docker environment
	docker-compose up -d

docker-down: ## Stop Docker environment
	docker-compose down

docker-logs: ## Show Docker logs
	docker-compose logs -f

docker-clean: ## Clean Docker resources
	docker-compose down -v
	docker system prune -f

# Run targets
run: ## Run the engine locally
	RUST_LOG=info cargo run --release --package engine

run-debug: ## Run the engine with debug logging
	RUST_LOG=debug cargo run --package engine

run-loadgen: ## Run load generator against local engine
	cargo run --release --package loadgen -- \
		--target 127.0.0.1:7000 \
		--symbols AAPL,GOOGL,MSFT \
		--rate 1000 \
		--duration 60s

# Load testing targets
load-test: ## Run load test (requires running engine)
	./scripts/test.sh load-test

load-test-high: ## Run high-rate load test
	./scripts/test.sh load-test --rate 10000 --duration 120

load-test-symbols: ## Run load test with many symbols
	./scripts/test.sh load-test --symbols AAPL,GOOGL,MSFT,AMZN,TSLA,META,NFLX,NVDA --rate 5000

# Monitoring targets
metrics: ## Show current metrics
	curl -s http://localhost:9000/metrics

prometheus: ## Open Prometheus UI
	@echo "Opening Prometheus at http://localhost:9090"
	@command -v open >/dev/null 2>&1 && open http://localhost:9090 || echo "Please open http://localhost:9090 in your browser"

grafana: ## Open Grafana UI
	@echo "Opening Grafana at http://localhost:3000 (admin/admin)"
	@command -v open >/dev/null 2>&1 && open http://localhost:3000 || echo "Please open http://localhost:3000 in your browser"

# Utility targets
clean: ## Clean build artifacts
	cargo clean
	docker-compose down -v 2>/dev/null || true
	rm -f *.txt *.log

clean-all: clean ## Clean everything including Docker images
	docker system prune -af

watch: ## Watch for changes and rebuild
	cargo watch -x "build --release"

watch-test: ## Watch for changes and run tests
	cargo watch -x "test --workspace"

# Development targets
dev: build-debug test fmt clippy ## Full development cycle

ci: fmt-check clippy test bench ## CI pipeline

release: clean build test bench ## Prepare for release

# Documentation targets
docs: ## Generate documentation
	cargo doc --workspace --no-deps --open

docs-private: ## Generate documentation including private items
	cargo doc --workspace --no-deps --document-private-items --open

# Configuration
RUST_LOG ?= info
GATEWAY_ADDR ?= 0.0.0.0:7000
METRICS_ADDR ?= 0.0.0.0:9000
ADMIN_ADDR ?= 0.0.0.0:9100

# Export environment variables
export RUST_LOG GATEWAY_ADDR METRICS_ADDR ADMIN_ADDR