# Build stage
FROM rust:1.91 as builder

WORKDIR /usr/src/app

# Copy manifests
COPY Cargo.toml Cargo.lock ./
COPY chat_client/Cargo.toml ./chat_client/
COPY chat_server/Cargo.toml ./chat_server/
COPY chat_shared/Cargo.toml ./chat_shared/

# Create dummy source files to cache dependencies
RUN mkdir -p chat_client/src chat_server/src chat_shared/src && \
    echo "fn main() {}" > chat_server/src/main.rs && \
    echo "fn main() {}" > chat_client/src/main.rs && \
    echo "pub fn dummy() {}" > chat_shared/src/lib.rs

# Build dependencies (this layer will be cached)
RUN cargo build --release --bin chat_server

# Remove dummy files
RUN rm -rf chat_client/src chat_server/src chat_shared/src

# Copy actual source code
COPY . .

# Build the actual application
RUN cargo build --release --bin chat_server

# Runtime stage
FROM debian:bookworm-slim

# Install CA certificates for HTTPS
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 chatuser

WORKDIR /app

# Copy binary from builder
COPY --from=builder /usr/src/app/target/release/chat_server /app/

# Change ownership
RUN chown -R chatuser:chatuser /app

USER chatuser

# Expose internal port (Caddy will handle external TLS)
EXPOSE 8080

# Set default environment variables
ENV CHAT_SERVER_ADDR="0.0.0.0:8080"
ENV CHAT_SERVER_MAX_CLIENTS="100"

CMD ["./chat_server"]
