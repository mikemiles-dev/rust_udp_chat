# Build stage
FROM rust:1.83-slim as builder

WORKDIR /build

# Copy entire workspace (needed for Cargo workspace)
COPY Cargo.toml Cargo.lock ./
COPY shared ./shared
COPY client ./client
COPY server ./server

# Build the client in release mode
RUN cargo build --release --bin client

# Runtime stage
FROM debian:bookworm-slim

# Install ca-certificates for TLS
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /build/target/release/client /usr/local/bin/rust-chat-client

# Set environment variables (can be overridden at runtime)
ENV CHAT_SERVER=""
ENV CHAT_USERNAME=""

ENTRYPOINT ["/usr/local/bin/rust-chat-client"]
