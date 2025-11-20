#!/bin/bash
set -e

echo "=== Caddy Setup Script for Digital Ocean ==="
echo ""

# Check if running as root
if [[ $EUID -ne 0 ]]; then
   echo "This script must be run as root (use sudo)"
   exit 1
fi

# Prompt for domain and email
echo "This script will:"
echo "1. Install Caddy"
echo "2. Configure it for your domain"
echo "3. Set up automatic HTTPS"
echo ""
read -p "Enter your domain (e.g., chat.yourdomain.com): " DOMAIN
read -p "Enter your email (for Let's Encrypt): " EMAIL

if [ -z "$DOMAIN" ] || [ -z "$EMAIL" ]; then
    echo "Error: Domain and email are required"
    exit 1
fi

echo ""
echo "=== Installing Caddy ==="

# Install dependencies
apt-get update
apt install -y debian-keyring debian-archive-keyring apt-transport-https curl

# Add Caddy repository
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list

# Install Caddy
apt update
apt install -y caddy

echo ""
echo "=== Configuring Caddy ==="

# Backup existing Caddyfile if it exists
if [ -f /etc/caddy/Caddyfile ]; then
    cp /etc/caddy/Caddyfile /etc/caddy/Caddyfile.backup
    echo "Backed up existing Caddyfile to /etc/caddy/Caddyfile.backup"
fi

# Create Caddyfile
cat > /etc/caddy/Caddyfile << CADDY_EOF
{
    email $EMAIL
}

$DOMAIN {
    reverse_proxy localhost:8080

    log {
        output file /var/log/caddy/chat-access.log
        format json
    }
}
CADDY_EOF

echo "Created Caddyfile at /etc/caddy/Caddyfile"

# Create log directory
mkdir -p /var/log/caddy
chown caddy:caddy /var/log/caddy

echo ""
echo "=== Configuring Firewall ==="

# Configure UFW
ufw allow 22/tcp   # SSH
ufw allow 80/tcp   # HTTP (for Let's Encrypt)
ufw allow 443/tcp  # HTTPS
ufw --force enable

echo ""
echo "=== Starting Caddy ==="

# Enable and start Caddy
systemctl enable caddy
systemctl restart caddy

echo ""
echo "=== Checking Caddy Status ==="
systemctl status caddy --no-pager

echo ""
echo "=== Setup Complete! ==="
echo ""
echo "Configuration:"
echo "  Domain: $DOMAIN"
echo "  Email: $EMAIL"
echo "  Reverse Proxy: localhost:8080"
echo ""
echo "Next steps:"
echo "1. Ensure your DNS is pointing to this server"
echo "2. Wait a few minutes for DNS to propagate"
echo "3. Start your chat server on localhost:8080"
echo "4. Caddy will automatically obtain HTTPS certificate"
echo ""
echo "Useful commands:"
echo "  sudo systemctl status caddy    # Check Caddy status"
echo "  sudo systemctl restart caddy   # Restart Caddy"
echo "  sudo journalctl -u caddy -f    # View Caddy logs"
echo "  sudo caddy list-certificates   # Check certificates"
echo ""
echo "To view access logs:"
echo "  sudo tail -f /var/log/caddy/chat-access.log"
echo ""
