# Build stage
FROM rust:1.75-slim as builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build the application in release mode
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install OpenSSL and CA certificates (required for HTTPS)
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 && \
    rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/discord-price-ticker /usr/local/bin/discord-price-ticker

# Copy sample config (user should mount their own)
COPY app_config.sample.json /app/app_config.sample.json

# Create a non-root user
RUN useradd -m -u 1000 appuser && \
    chown -R appuser:appuser /app

USER appuser

# Default to using app_config.json in /app directory
# Users can override with --config flag or mount config at /app/app_config.json
ENTRYPOINT ["discord-price-ticker"]
CMD ["--config", "/app/app_config.json"]
