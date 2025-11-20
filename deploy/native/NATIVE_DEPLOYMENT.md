# Native Deployment Guide (Ubuntu + Caddy)

This guide shows how to deploy the Rust chat server directly on Ubuntu without Docker.

## Quick Start

### 1. Clone Repository

```bash
# SSH into your Ubuntu server
ssh user@your-server-ip

# Clone the repo
git clone https://github.com/yourusername/rust_chat.git
cd rust_chat
```

### 2. Run Installation Script

```bash
# Make script executable
chmod +x deploy/install.sh

# Run as root
sudo ./deploy/install.sh
```

The script will:
- Install Rust compiler
- Install Caddy web server
- Create a service user (`rustchat`)
- Build the chat server binary
- Install systemd service
- Set up directory structure

### 3. Configure Caddy

Edit the Caddyfile:

```bash
sudo nano /etc/caddy/Caddyfile
```

Use this template (replace with your details):

```caddy
{
    email your-email@example.com
}

chat.yourdomain.com {
    reverse_proxy localhost:8080

    log {
        output file /var/log/caddy/chat-access.log
        format json
    }
}
```

### 4. Start Services

```bash
# Enable and start the chat server
sudo systemctl enable rust-chat
sudo systemctl start rust-chat

# Enable and restart Caddy
sudo systemctl enable caddy
sudo systemctl restart caddy
```

### 5. Verify Everything is Working

```bash
# Check chat server
sudo systemctl status rust-chat

# Check Caddy
sudo systemctl status caddy

# View logs
sudo journalctl -u rust-chat -f
sudo journalctl -u caddy -f
```

## Management

### Start/Stop/Restart

```bash
# Chat server
sudo systemctl start rust-chat
sudo systemctl stop rust-chat
sudo systemctl restart rust-chat

# Caddy
sudo systemctl start caddy
sudo systemctl stop caddy
sudo systemctl restart caddy
```

### View Logs

```bash
# Chat server logs (follow in real-time)
sudo journalctl -u rust-chat -f

# Last 100 lines
sudo journalctl -u rust-chat -n 100

# Caddy logs
sudo journalctl -u caddy -f

# Access logs
sudo tail -f /var/log/caddy/chat-access.log
```

### Check Status

```bash
# Chat server
sudo systemctl status rust-chat

# Caddy
sudo systemctl status caddy

# Check if port is listening
sudo netstat -tlnp | grep 8080
sudo netstat -tlnp | grep 443
```

## Updating

### Update Code

```bash
# Navigate to directory
cd ~/rust_chat

# Pull latest changes
git pull

# Rebuild
cargo build --release --bin server

# Copy new binary
sudo cp target/release/server /opt/rust_chat/

# Restart service
sudo systemctl restart rust-chat

# Check logs
sudo journalctl -u rust-chat -f
```

### Update Caddy Configuration

```bash
# Edit config
sudo nano /etc/caddy/Caddyfile

# Test config
sudo caddy validate --config /etc/caddy/Caddyfile

# Reload (no downtime)
sudo systemctl reload caddy
```

## Configuration

### Environment Variables

Edit the systemd service file:

```bash
sudo nano /etc/systemd/system/rust-chat.service
```

Modify environment variables:

```ini
Environment="CHAT_SERVER_ADDR=127.0.0.1:8080"
Environment="CHAT_SERVER_MAX_CLIENTS=500"
```

Then reload:

```bash
sudo systemctl daemon-reload
sudo systemctl restart rust-chat
```

### Change Port

1. Edit systemd service:
```bash
sudo nano /etc/systemd/system/rust-chat.service
```

Change `CHAT_SERVER_ADDR`:
```ini
Environment="CHAT_SERVER_ADDR=127.0.0.1:9000"
```

2. Update Caddyfile:
```bash
sudo nano /etc/caddy/Caddyfile
```

Change port:
```caddy
reverse_proxy localhost:9000
```

3. Restart:
```bash
sudo systemctl daemon-reload
sudo systemctl restart rust-chat
sudo systemctl reload caddy
```

## Troubleshooting

### Chat Server Won't Start

```bash
# Check logs for errors
sudo journalctl -u rust-chat -n 50

# Check if binary exists
ls -la /opt/rust_chat/server

# Try running manually
sudo -u rustchat /opt/rust_chat/server

# Check permissions
ls -la /opt/rust_chat/
```

### Port Already in Use

```bash
# See what's using port 8080
sudo netstat -tlnp | grep 8080

# Kill the process if needed
sudo kill <PID>
```

### Caddy Certificate Issues

```bash
# Check Caddy logs
sudo journalctl -u caddy -n 100

# Verify DNS
dig chat.yourdomain.com

# Test Let's Encrypt connectivity
curl -v https://acme-v02.api.letsencrypt.org/directory

# Check firewall
sudo ufw status
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
```

### Permission Denied

```bash
# Fix ownership
sudo chown -R rustchat:rustchat /opt/rust_chat

# Make binary executable
sudo chmod +x /opt/rust_chat/server
```

## Firewall Configuration

### UFW (Ubuntu Firewall)

```bash
# Allow SSH
sudo ufw allow 22/tcp

# Allow HTTP (for Let's Encrypt)
sudo ufw allow 80/tcp

# Allow HTTPS
sudo ufw allow 443/tcp

# Enable firewall
sudo ufw enable

# Check status
sudo ufw status
```

## Performance Tuning

### Systemd Limits

Edit the service file to increase limits:

```bash
sudo nano /etc/systemd/system/rust-chat.service
```

Add under `[Service]`:

```ini
LimitNOFILE=65536
LimitNPROC=4096
```

Reload:

```bash
sudo systemctl daemon-reload
sudo systemctl restart rust-chat
```

### System Limits

For more than 1000 concurrent connections:

```bash
# Edit limits
sudo nano /etc/security/limits.conf
```

Add:

```
rustchat soft nofile 65536
rustchat hard nofile 65536
```

Reboot or restart the service.

## Monitoring

### Resource Usage

```bash
# CPU and memory
ps aux | grep server

# Real-time monitoring
top -p $(pgrep server)

# Detailed info
sudo systemctl show rust-chat
```

### Connection Count

```bash
# Active connections
sudo netstat -an | grep :8080 | grep ESTABLISHED | wc -l

# All connections
sudo ss -tn | grep :8080
```

## Backup

### Binary and Config

```bash
# Create backup directory
mkdir -p ~/backups

# Backup binary
sudo cp /opt/rust_chat/server ~/backups/server.backup

# Backup service file
sudo cp /etc/systemd/system/rust-chat.service ~/backups/

# Backup Caddyfile
sudo cp /etc/caddy/Caddyfile ~/backups/

# Backup Caddy certificates
sudo tar -czf ~/backups/caddy-certs-$(date +%Y%m%d).tar.gz /var/lib/caddy
```

## Uninstall

```bash
# Stop and disable services
sudo systemctl stop rust-chat
sudo systemctl disable rust-chat
sudo systemctl stop caddy
sudo systemctl disable caddy

# Remove files
sudo rm /etc/systemd/system/rust-chat.service
sudo rm -rf /opt/rust_chat
sudo apt remove caddy

# Remove user
sudo userdel rustchat

# Reload systemd
sudo systemctl daemon-reload
```

## Advantages Over Docker

✅ **Better Performance** - No container overhead
✅ **Lower Memory Usage** - Direct execution
✅ **Simpler Debugging** - Direct access to logs
✅ **Easier Updates** - Just rebuild and restart
✅ **Native systemd Integration** - Better system management

## Comparison

| Feature | Docker | Native |
|---------|--------|--------|
| Setup Complexity | Easy | Medium |
| Performance | Good | Excellent |
| Memory Usage | Higher | Lower |
| Updates | Rebuild image | Rebuild binary |
| Debugging | Moderate | Easy |
| Isolation | High | Medium |
| systemd Integration | Via docker | Native |

Choose **Docker** for: Multi-server, complex deployments, containerization requirements
Choose **Native** for: Single server, maximum performance, simpler stack
