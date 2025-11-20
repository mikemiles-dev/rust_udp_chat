#!/bin/bash
set -e

# Rust Chat Server Installation Script for Ubuntu
# This script installs the Rust chat server as a systemd service with Caddy reverse proxy

echo "=== Rust Chat Server Installation ==="
echo ""

# Check if running as root
if [[ $EUID -ne 0 ]]; then
   echo "This script must be run as root (use sudo)"
   exit 1
fi

# Variables
INSTALL_DIR="/opt/rust_chat"
SERVICE_FILE="/etc/systemd/system/rust-chat.service"
CADDY_CONFIG="/etc/caddy/Caddyfile"
USER="rustchat"

echo "Step 1: Installing dependencies..."
apt-get update
apt-get install -y curl build-essential

echo ""
echo "Step 2: Installing Rust (if not already installed)..."
if ! command -v cargo &> /dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
else
    echo "Rust already installed"
fi

echo ""
echo "Step 3: Installing Caddy..."
if ! command -v caddy &> /dev/null; then
    apt install -y debian-keyring debian-archive-keyring apt-transport-https curl
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list
    apt update
    apt install -y caddy
else
    echo "Caddy already installed"
fi

echo ""
echo "Step 4: Creating service user..."
if ! id "$USER" &>/dev/null; then
    useradd -r -s /bin/false $USER
    echo "Created user: $USER"
else
    echo "User $USER already exists"
fi

echo ""
echo "Step 5: Creating installation directory..."
mkdir -p $INSTALL_DIR
chown $USER:$USER $INSTALL_DIR

echo ""
echo "Step 6: Building the Rust chat server..."
if [ ! -d "$(pwd)/Cargo.toml" ]; then
    echo "Error: Must run this script from the rust_chat directory"
    exit 1
fi

cargo build --release --bin server

echo ""
echo "Step 7: Installing binary..."
cp target/release/server $INSTALL_DIR/
chown $USER:$USER $INSTALL_DIR/server
chmod +x $INSTALL_DIR/server

echo ""
echo "Step 8: Installing systemd service..."
cp deploy/native/rust-chat.service $SERVICE_FILE
systemctl daemon-reload

echo ""
echo "Step 9: Configuring Caddy..."
if [ -f "$CADDY_CONFIG" ]; then
    echo "Backing up existing Caddyfile to ${CADDY_CONFIG}.backup"
    cp $CADDY_CONFIG ${CADDY_CONFIG}.backup
fi

echo ""
echo "=== Configuration Required ==="
echo ""
echo "Before starting the services, you need to:"
echo ""
echo "1. Edit the Caddyfile:"
echo "   sudo nano /etc/caddy/Caddyfile"
echo ""
echo "   Replace:"
echo "   - your-email@example.com (with your email)"
echo "   - chat.yourdomain.com (with your domain)"
echo ""
echo "   You can use the template at: deploy/native/Caddyfile.native"
echo ""
echo "2. Ensure your domain DNS is pointing to this server"
echo ""
echo "3. Start the services:"
echo "   sudo systemctl enable rust-chat"
echo "   sudo systemctl start rust-chat"
echo "   sudo systemctl enable caddy"
echo "   sudo systemctl restart caddy"
echo ""
echo "4. Check status:"
echo "   sudo systemctl status rust-chat"
echo "   sudo systemctl status caddy"
echo ""
echo "5. View logs:"
echo "   sudo journalctl -u rust-chat -f"
echo "   sudo journalctl -u caddy -f"
echo ""
echo "=== Installation Complete ==="
