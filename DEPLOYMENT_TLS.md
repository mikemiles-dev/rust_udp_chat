# Rust Chat - TLS Deployment Guide

This guide explains how to deploy the Rust Chat server with native TLS encryption on Digital Ocean (or any server).

## Overview

The chat server now has **native TLS support** built-in using `tokio-rustls`. No reverse proxy (like Caddy) is needed.

## Prerequisites

1. A domain name pointing to your Digital Ocean droplet
2. SSH access to your droplet
3. Docker and Docker Compose installed

## Step 1: Get TLS Certificates

You need TLS certificates for your domain. The recommended way is using Certbot with Let's Encrypt:

### On Your Digital Ocean Droplet:

```bash
# Install Certbot
sudo apt update
sudo apt install -y certbot

# Get certificates (replace with your domain and email)
sudo certbot certonly --standalone \
  -d chat.yourdomain.com \
  -m your-email@example.com \
  --agree-tos \
  --non-interactive

# Certificates will be created at:
# /etc/letsencrypt/live/chat.yourdomain.com/fullchain.pem
# /etc/letsencrypt/live/chat.yourdomain.com/privkey.pem
```

**Important:** Make sure ports 80 and 443 are open temporarily for certificate verification, then close port 80 after getting certs.

## Step 2: Set Up the Project

```bash
# Clone your repository
git clone https://github.com/yourusername/rust_chat.git
cd rust_chat/deploy/docker

# Create certs directory
mkdir -p certs

# Copy certificates to project (as root since certbot certs are protected)
sudo cp /etc/letsencrypt/live/chat.yourdomain.com/fullchain.pem certs/
sudo cp /etc/letsencrypt/live/chat.yourdomain.com/privkey.pem certs/

# Make certificates readable by the container
sudo chmod 644 certs/fullchain.pem
sudo chmod 644 certs/privkey.pem
```

## Step 3: Configure Firewall

Open only the port you need:

```bash
# If using ufw
sudo ufw allow 8443/tcp
sudo ufw enable

# If using Digital Ocean firewall
# Go to Networking > Firewalls in your DO console
# Add inbound rule: TCP port 8443 from all sources
```

## Step 4: Deploy with Docker Compose

```bash
cd /path/to/rust_chat/deploy/docker

# Build and start the server
docker-compose up -d

# Check logs
docker-compose logs -f chat_server
```

You should see:
```
TLS enabled - loading certificates...
TLS certificates loaded successfully
Chat Server started at 0.0.0.0:8443
```

## Step 5: Connect from Client

Clients must use the `tls://` prefix when connecting:

```bash
# From your client machine
./client

# When prompted for server:
tls://chat.yourdomain.com:8443
```

**Or set environment variable:**
```bash
export CHAT_SERVER=tls://chat.yourdomain.com:8443
export CHAT_USERNAME=YourName
./client
```

## Certificate Renewal

Let's Encrypt certificates expire after 90 days. Set up auto-renewal:

```bash
# Create renewal script
sudo tee /usr/local/bin/renew-chat-certs.sh > /dev/null <<'EOF'
#!/bin/bash
set -e

# Renew certificates
certbot renew --quiet

# Copy new certs to project
cp /etc/letsencrypt/live/chat.yourdomain.com/fullchain.pem /path/to/rust_chat/deploy/docker/certs/
cp /etc/letsencrypt/live/chat.yourdomain.com/privkey.pem /path/to/rust_chat/deploy/docker/certs/

# Fix permissions
chmod 644 /path/to/rust_chat/deploy/docker/certs/*.pem

# Restart server
cd /path/to/rust_chat/deploy/docker
docker-compose restart chat_server
EOF

sudo chmod +x /usr/local/bin/renew-chat-certs.sh

# Set up cron job (runs weekly)
(crontab -l 2>/dev/null; echo "0 3 * * 1 /usr/local/bin/renew-chat-certs.sh") | crontab -
```

## Troubleshooting

### Server shows "TLS not configured"

- Check that `TLS_CERT_PATH` and `TLS_KEY_PATH` environment variables are set
- Verify certificate files exist and are readable
- Check docker-compose.yml volume mount

### Client can't connect

- Make sure you're using `tls://` prefix
- Verify port 8443 is open in firewall
- Check server logs: `docker-compose logs chat_server`
- Try without TLS first to isolate the issue

### Certificate errors on client

- Ensure your domain name matches the certificate
- Make sure you're using the domain name, not IP address
- Verify certificates are valid: `openssl x509 -in certs/fullchain.pem -text -noout`

### Running without TLS (for testing)

To run without TLS temporarily:

1. Edit `docker-compose.yml` and remove the `TLS_CERT_PATH` and `TLS_KEY_PATH` environment variables
2. Change port from 8443 to 8080
3. Restart: `docker-compose up -d`
4. Connect without `tls://` prefix: `your.domain.com:8080`

## Security Notes

1. **Always use TLS in production** - connections without TLS send everything in plaintext
2. Keep certificates up to date - expired certificates will cause connection failures
3. Protect your private key - never commit `privkey.pem` to git
4. Use strong firewall rules - only allow necessary ports

## Updating the Server

```bash
cd /path/to/rust_chat
git pull
cd deploy/docker
docker-compose build
docker-compose up -d
```

## Monitoring

Check server status:
```bash
docker-compose ps
docker-compose logs -f chat_server
```

Check active connections:
```bash
docker exec -it rust_chat_server /bin/bash
# Inside container, use server commands if TTY available
# Or check from outside using netstat:
ss -tn | grep :8443
```
