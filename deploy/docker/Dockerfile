# Build stage
FROM rust:1.91 as builder

WORKDIR /usr/src/app

# Copy manifests
COPY Cargo.toml Cargo.lock ./
COPY client/Cargo.toml ./client/
COPY server/Cargo.toml ./server/
COPY shared/Cargo.toml ./shared/

# Create dummy source files to cache dependencies
RUN mkdir -p client/src server/src shared/src && \
    echo "fn main() {}" > server/src/main.rs && \
    echo "fn main() {}" > client/src/main.rs && \
    echo "pub fn dummy() {}" > shared/src/lib.rs

# Build dependencies (this layer will be cached)
RUN cargo build --release --bin server

# Remove dummy files
RUN rm -rf client/src server/src shared/src

# Copy actual source code
COPY . .

# Build the actual application
RUN cargo build --release --bin server

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
COPY --from=builder /usr/src/app/target/release/server /app/

# Change ownership
RUN chown -R chatuser:chatuser /app

USER chatuser

# Expose internal port (Caddy will handle external TLS)
EXPOSE 8080

# Set default environment variables
ENV CHAT_SERVER_ADDR="0.0.0.0:8080"
ENV CHAT_SERVER_MAX_CLIENTS="100"

ENTRYPOINT ["/app/server"]
