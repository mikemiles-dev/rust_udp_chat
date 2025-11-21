#!/bin/bash

# TLS Certificate Setup Script
# This script obtains Let's Encrypt certificates for your chat server

set -e

echo "=== Rust Chat Server - Certificate Setup ==="
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo "Error: This script must be run as root (use sudo)"
    exit 1
fi

# Get domain name
read -p "Enter your domain name (e.g., chat.yourdomain.com): " DOMAIN

if [ -z "$DOMAIN" ]; then
    echo "Error: Domain name is required"
    exit 1
fi

# Get email
read -p "Enter your email for Let's Encrypt notifications: " EMAIL

if [ -z "$EMAIL" ]; then
    echo "Error: Email is required"
    exit 1
fi

echo ""
echo "Configuration:"
echo "  Domain: $DOMAIN"
echo "  Email: $EMAIL"
echo ""
read -p "Continue? (y/n): " CONFIRM

if [ "$CONFIRM" != "y" ]; then
    echo "Aborted."
    exit 0
fi

echo ""
echo "=== Installing Certbot ==="
apt update
apt install -y certbot

echo ""
echo "=== Obtaining Certificate ==="
echo "Note: Port 80 must be available for verification"
echo ""

# Check if port 80 is in use
if netstat -tuln | grep -q ':80 '; then
    echo "Warning: Port 80 is in use. Stopping services..."
    # Try to stop common services
    systemctl stop nginx 2>/dev/null || true
    systemctl stop apache2 2>/dev/null || true
    systemctl stop caddy 2>/dev/null || true
fi

# Get certificate
certbot certonly \
    --standalone \
    --non-interactive \
    --agree-tos \
    --email "$EMAIL" \
    -d "$DOMAIN"

if [ $? -ne 0 ]; then
    echo ""
    echo "Error: Failed to obtain certificate"
    echo "Make sure:"
    echo "  1. Domain DNS is pointing to this server"
    echo "  2. Port 80 is accessible from the internet"
    echo "  3. Firewall allows port 80"
    exit 1
fi

echo ""
echo "=== Certificate obtained successfully! ==="
echo ""
echo "Certificate files:"
echo "  Cert: /etc/letsencrypt/live/$DOMAIN/fullchain.pem"
echo "  Key:  /etc/letsencrypt/live/$DOMAIN/privkey.pem"
echo ""

# Set up auto-renewal
echo "=== Setting up automatic renewal ==="

# Create renewal hook script
cat > /usr/local/bin/restart-chat-server.sh << 'EOF'
#!/bin/bash
# Restart chat server after certificate renewal

# Check if running in tmux
if tmux has-session -t chat 2>/dev/null; then
    echo "Restarting chat server in tmux..."
    tmux kill-session -t chat
    cd /root/rust_chat/deploy/digital_ocean
    ./start-server.sh
fi

# Check if running in docker
if docker ps | grep -q rust_chat_server; then
    echo "Restarting Docker container..."
    cd /root/rust_chat/deploy/docker
    docker-compose restart chat_server
fi
EOF

chmod +x /usr/local/bin/restart-chat-server.sh

# Test renewal (dry run)
echo ""
echo "Testing automatic renewal (dry-run)..."
certbot renew --dry-run

echo ""
echo "=== Setup Complete! ==="
echo ""
echo "Certificate will auto-renew. To manually renew:"
echo "  sudo certbot renew"
echo ""
echo "Next steps:"
echo "  1. Update start-server.sh with your domain name"
echo "  2. Configure firewall: sudo ufw allow 8443/tcp"
echo "  3. Start your server: ./start-server.sh"
echo ""
