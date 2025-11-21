#!/bin/bash

# Chat Server Start Script for tmux with TLS
# This script starts the Rust chat server in a tmux session with TLS encryption

set -e

# Configuration
TMUX_SESSION="chat"
SERVER_ADDR="0.0.0.0:8443"
MAX_CLIENTS="100"
PROJECT_DIR="$HOME/rust_chat"
TLS_CERT_PATH="/etc/letsencrypt/live/chat.yourdomain.com/fullchain.pem"
TLS_KEY_PATH="/etc/letsencrypt/live/chat.yourdomain.com/privkey.pem"

echo "=== Rust Chat Server Starter ==="
echo ""

# Check if tmux is installed
if ! command -v tmux &> /dev/null; then
    echo "tmux is not installed. Installing..."
    sudo apt-get update
    sudo apt-get install -y tmux
fi

# Check if project directory exists
if [ ! -d "$PROJECT_DIR" ]; then
    echo "Error: Project directory not found at $PROJECT_DIR"
    echo "Please update PROJECT_DIR in this script or clone the repository"
    exit 1
fi

# Check if session already exists
if tmux has-session -t $TMUX_SESSION 2>/dev/null; then
    echo "Error: tmux session '$TMUX_SESSION' already exists"
    echo ""
    echo "Options:"
    echo "  1. Attach to existing session: tmux attach -t $TMUX_SESSION"
    echo "  2. Kill existing session: tmux kill-session -t $TMUX_SESSION"
    echo "  3. Use a different session name"
    exit 1
fi

# Change to project directory
cd "$PROJECT_DIR"

echo "Building server in release mode..."
cargo build --release --bin server

if [ $? -ne 0 ]; then
    echo "Error: Build failed"
    exit 1
fi

echo ""
echo "Starting chat server in tmux session '$TMUX_SESSION'..."
echo ""
echo "Configuration:"
echo "  Address: $SERVER_ADDR"
echo "  Max Clients: $MAX_CLIENTS"
echo "  Session: $TMUX_SESSION"

# Check if TLS certificates exist
if [ -f "$TLS_CERT_PATH" ] && [ -f "$TLS_KEY_PATH" ]; then
    echo "  TLS: ENABLED (certificates found)"
    echo "  Cert: $TLS_CERT_PATH"
    echo "  Key: $TLS_KEY_PATH"
    TLS_CONFIG="TLS_CERT_PATH=$TLS_CERT_PATH TLS_KEY_PATH=$TLS_KEY_PATH"
else
    echo "  TLS: DISABLED (certificates not found)"
    echo "  Note: Server will run without encryption"
    echo "  To enable TLS, update certificate paths in this script"
    TLS_CONFIG=""
fi
echo ""

# Create tmux session and run server
tmux new-session -d -s $TMUX_SESSION "CHAT_SERVER_ADDR=$SERVER_ADDR CHAT_SERVER_MAX_CLIENTS=$MAX_CLIENTS $TLS_CONFIG ./target/release/server"

echo "✓ Server started in tmux session '$TMUX_SESSION'"
echo ""
echo "Useful commands:"
echo "  tmux attach -t $TMUX_SESSION   # Attach to session (use server commands)"
echo "  tmux ls                         # List all sessions"
echo "  Ctrl+B, then D                  # Detach from session (keeps running)"
echo "  tmux kill-session -t $TMUX_SESSION  # Stop server and kill session"
echo ""
echo "Server commands (when attached):"
echo "  /help       # Show available commands"
echo "  /list       # List connected users"
echo "  /kick USER  # Kick a user"
echo "  /quit       # Shutdown server"
echo ""
echo "To attach now, run:"
echo "  tmux attach -t $TMUX_SESSION"
echo ""
echo "Client connection:"
if [ -f "$TLS_CERT_PATH" ]; then
    echo "  Use: tls://chat.yourdomain.com:8443"
else
    echo "  Use: chat.yourdomain.com:8443"
fi
echo ""
