# Dockerfile for HFT RTB Load Generator
FROM rust:1.75-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy dependency manifests for better caching
COPY Cargo.toml Cargo.lock ./
COPY tools/loadgen/Cargo.toml ./tools/loadgen/
COPY crates/*/Cargo.toml ./crates/

# Create dummy source files
RUN find . -name "Cargo.toml" -exec dirname {} \; | \
    xargs -I {} sh -c 'mkdir -p {}/src && echo "fn main() {}" > {}/src/main.rs || echo "pub fn dummy() {}" > {}/src/lib.rs'

# Build dependencies
RUN cargo build --release --package loadgen

# Remove dummy files and copy real source
RUN find . -name "src" -type d -exec rm -rf {} + 2>/dev/null || true
COPY . .
RUN find . -name "*.rs" -exec touch {} +

# Build the load generator
ENV RUSTFLAGS="-C target-cpu=native -C opt-level=3"
RUN cargo build --release --package loadgen

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r loadgen && useradd -r -g loadgen loadgen

WORKDIR /app

# Copy binary
COPY --from=builder /app/target/release/loadgen /usr/local/bin/loadgen
RUN chmod +x /usr/local/bin/loadgen

USER loadgen

# Default environment
ENV TARGET_HOST=localhost:7000
ENV SYMBOLS=AAPL,GOOGL,MSFT
ENV RATE=1000
ENV DURATION=60s

ENTRYPOINT ["/usr/local/bin/loadgen"]