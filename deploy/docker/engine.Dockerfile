# Multi-stage build for optimized HFT RTB Engine
FROM rust:1.75-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy dependency manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./
COPY crates/*/Cargo.toml ./crates/
COPY services/*/Cargo.toml ./services/
COPY tools/*/Cargo.toml ./tools/

# Create dummy source files to build dependencies
RUN find . -name "Cargo.toml" -exec dirname {} \; | \
    xargs -I {} sh -c 'mkdir -p {}/src && echo "fn main() {}" > {}/src/main.rs || echo "pub fn dummy() {}" > {}/src/lib.rs'

# Build dependencies (this layer will be cached)
RUN cargo build --release --package engine

# Remove dummy source files
RUN find . -name "src" -type d -exec rm -rf {} + 2>/dev/null || true

# Copy actual source code
COPY . .

# Touch source files to ensure rebuild
RUN find . -name "*.rs" -exec touch {} +

# Build the actual application with optimizations for HFT
ENV RUSTFLAGS="-C target-cpu=native -C opt-level=3 -C lto=fat -C codegen-units=1"
RUN cargo build --release --package engine

# Runtime stage with minimal footprint
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# Create non-root user with minimal privileges
RUN groupadd -r engine && useradd -r -g engine -s /bin/false engine

# Create necessary directories
RUN mkdir -p /app/data /app/logs && \
    chown -R engine:engine /app

WORKDIR /app

# Copy binary from builder stage
COPY --from=builder /app/target/release/engine /usr/local/bin/engine
RUN chmod +x /usr/local/bin/engine

# Switch to non-root user
USER engine

# Environment configuration for production
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1
ENV GATEWAY_ADDR=0.0.0.0:7000
ENV METRICS_ADDR=0.0.0.0:9000
ENV ADMIN_ADDR=0.0.0.0:9100

# Expose ports
EXPOSE 7000 9000 9100

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:9000/metrics || exit 1

# Use exec form for proper signal handling
ENTRYPOINT ["/usr/local/bin/engine"]
