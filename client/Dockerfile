# Build stage - use Alpine for musl libc (smaller binary)
FROM rust:1.91-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev

WORKDIR /build

# Copy entire workspace (needed for Cargo workspace)
COPY Cargo.toml Cargo.lock ./
COPY shared ./shared
COPY client ./client
COPY server ./server

# Build the client in release mode
RUN cargo build --release --bin client

# Runtime stage - Alpine for minimal size
FROM alpine:latest

# Install ca-certificates for TLS
RUN apk --no-cache add ca-certificates

# Copy the binary from builder
COPY --from=builder /build/target/release/client /usr/local/bin/rust-chat-client

# Set environment variables (can be overridden at runtime)
ENV CHAT_SERVER=""
ENV CHAT_USERNAME=""

ENTRYPOINT ["/usr/local/bin/rust-chat-client"]
