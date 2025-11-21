# Digital Ocean Deployment Guide

Deploy the Rust chat server on Digital Ocean with native TLS encryption.

## Overview

This deployment runs the chat server in a tmux session with TLS encryption. No reverse proxy needed - the server handles TLS directly.

**Architecture:**
```
Internet (TLS/TCP port 8443)
    ↓
Rust Chat Server (native TLS, in tmux)
```

## Prerequisites

- Digital Ocean Droplet (Ubuntu 22.04 recommended)
- Domain name pointing to your droplet's IP
- SSH access to your droplet

## Quick Start

### 1. Create Droplet

1. Log into Digital Ocean
2. Create new Droplet
3. Choose **Ubuntu 22.04 LTS**
4. Select size (Basic $6/month is fine for small deployments)
5. Add your SSH key
6. Create droplet

### 2. Configure DNS

Point your domain to the droplet:

```
Type: A Record
Host: chat (or @)
Value: <your-droplet-ip>
TTL: 3600
```

Wait 5-10 minutes for DNS propagation. Check with:
```bash
dig chat.yourdomain.com
```

### 3. SSH into Droplet

```bash
ssh root@<your-droplet-ip>
```

### 4. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### 5. Clone Repository

```bash
cd ~
git clone https://github.com/yourusername/rust_chat.git
cd rust_chat
```

### 6. Get TLS Certificates

```bash
cd deploy/digital_ocean
sudo ./setup-certificates.sh
```

This will:
- Install Certbot
- Obtain Let's Encrypt certificates for your domain
- Set up automatic renewal
- Configure renewal hooks

**Important:** Enter your actual domain and email when prompted!

### 7. Configure Server

Edit `start-server.sh` to set your domain:

```bash
nano start-server.sh
```

Change this line:
```bash
TLS_CERT_PATH="/etc/letsencrypt/live/chat.yourdomain.com/fullchain.pem"
TLS_KEY_PATH="/etc/letsencrypt/live/chat.yourdomain.com/privkey.pem"
```

Replace `chat.yourdomain.com` with your actual domain.

### 8. Configure Firewall

```bash
# Allow SSH
sudo ufw allow 22/tcp

# Allow chat server port
sudo ufw allow 8443/tcp

# Enable firewall
sudo ufw enable
```

### 9. Start Server

```bash
./start-server.sh
```

This will:
- Build the server in release mode
- Start it in a tmux session named `chat`
- Bind to 0.0.0.0:8443 with TLS encryption

### 10. Verify Server is Running

```bash
# Attach to tmux session
tmux attach -t chat
```

You should see:
```
TLS enabled - loading certificates...
TLS certificates loaded successfully
Chat Server started at 0.0.0.0:8443
```

Press `Ctrl+B`, then `D` to detach and leave it running.

## Usage

### Attach to Server Console

```bash
tmux attach -t chat
```

Use server commands:
```
/list              # Show connected users
/kick username     # Kick a user
/help              # Show commands
/quit              # Shutdown server
```

### Detach from Session

**Press:** `Ctrl+B`, then `D`

The server keeps running in the background!

### Stop Server

Option 1: From inside tmux session
```
/quit
```

Option 2: Kill the session
```bash
tmux kill-session -t chat
```

### Restart Server

```bash
# Kill existing session
tmux kill-session -t chat

# Start again
cd ~/rust_chat/deploy/digital_ocean
./start-server.sh
```

### View Logs

```bash
# Attach to tmux to see live logs
tmux attach -t chat

# Scroll up in tmux
# Press: Ctrl+B, then [
# Use arrow keys or Page Up/Down
# Press Q to exit scroll mode
```

## Updating the Server

```bash
# Navigate to project
cd ~/rust_chat

# Pull latest changes
git pull

# Kill running server
tmux kill-session -t chat

# Rebuild and restart
cd deploy/digital_ocean
./start-server.sh
```

## Connecting Clients

From your client machine:

```bash
# Build client
cargo build --release --bin client

# Run client
./target/release/client
```

When prompted:
- **Server:** `tls://chat.yourdomain.com:8443`
- **Username:** Your name

**Or set environment variables:**
```bash
export CHAT_SERVER=tls://chat.yourdomain.com:8443
export CHAT_USERNAME=YourName
./target/release/client
```

## Configuration

All configuration is in `start-server.sh`:

```bash
# Server binding address
SERVER_ADDR="0.0.0.0:8443"

# Maximum concurrent clients
MAX_CLIENTS="100"

# TLS certificate paths
TLS_CERT_PATH="/etc/letsencrypt/live/chat.yourdomain.com/fullchain.pem"
TLS_KEY_PATH="/etc/letsencrypt/live/chat.yourdomain.com/privkey.pem"
```

### Change Server Port

Edit `start-server.sh`:
```bash
SERVER_ADDR="0.0.0.0:9443"  # Change port
```

Update firewall:
```bash
sudo ufw allow 9443/tcp
sudo ufw delete allow 8443/tcp
```

### Change Max Clients

Edit `start-server.sh`:
```bash
MAX_CLIENTS="500"  # Increase limit
```

### Run Without TLS (Testing Only)

Edit `start-server.sh` and comment out the cert paths:
```bash
# TLS_CERT_PATH="/etc/letsencrypt/live/chat.yourdomain.com/fullchain.pem"
# TLS_KEY_PATH="/etc/letsencrypt/live/chat.yourdomain.com/privkey.pem"
```

Change port:
```bash
SERVER_ADDR="0.0.0.0:8080"
```

**Warning:** Without TLS, all traffic is unencrypted!

## Certificate Management

### Auto-Renewal

Certificates renew automatically via Certbot. Verify renewal is configured:

```bash
sudo certbot renew --dry-run
```

### Manual Renewal

```bash
# Renew certificates
sudo certbot renew

# Restart server to use new certificates
tmux kill-session -t chat
cd ~/rust_chat/deploy/digital_ocean
./start-server.sh
```

### Check Certificate Expiry

```bash
sudo certbot certificates
```

## Troubleshooting

### Server won't start

```bash
# Check if tmux session already exists
tmux ls

# If exists, kill it
tmux kill-session -t chat

# Try starting again
./start-server.sh
```

### "TLS not configured" message

Server can't find certificates. Check:

```bash
# Verify certificates exist
sudo ls -la /etc/letsencrypt/live/chat.yourdomain.com/

# Check paths in start-server.sh match your domain
nano start-server.sh
```

### Can't connect from client

```bash
# Check server is running
tmux attach -t chat

# Check firewall
sudo ufw status

# Test TLS connection
openssl s_client -connect chat.yourdomain.com:8443

# Check DNS
dig chat.yourdomain.com  # Should show your droplet IP
```

### Port already in use

```bash
# See what's using port 8443
sudo netstat -tlnp | grep 8443

# Kill the process
sudo kill <PID>
```

### Certificate error

```bash
# Check certificate is valid
sudo openssl x509 -in /etc/letsencrypt/live/chat.yourdomain.com/fullchain.pem -text -noout

# If expired, renew
sudo certbot renew --force-renewal

# Restart server
tmux kill-session -t chat
./start-server.sh
```

## Monitoring

### Check Active Users

Attach to tmux and use:
```
/list
```

### Check Connection Count

```bash
# See active connections to server
ss -tn | grep :8443 | wc -l
```

### Check Resource Usage

```bash
# Overall system
htop

# Just the chat server
ps aux | grep server

# Memory usage
free -h

# Disk usage
df -h
```

### Set Up Monitoring Alert

```bash
# Create monitoring script
cat > ~/monitor-chat.sh << 'EOF'
#!/bin/bash
CONNECTIONS=$(ss -tn | grep :8443 | wc -l)
if [ $CONNECTIONS -gt 50 ]; then
    echo "ALERT: $CONNECTIONS connections at $(date)" >> ~/alerts.log
fi
EOF

chmod +x ~/monitor-chat.sh

# Run every 5 minutes
crontab -e
# Add: */5 * * * * ~/monitor-chat.sh
```

## Security Best Practices

### 1. Use SSH Keys

```bash
# On your local machine
ssh-keygen -t ed25519

# Copy to droplet
ssh-copy-id root@your-droplet-ip
```

### 2. Create Non-Root User

```bash
adduser chatadmin
usermod -aG sudo chatadmin
su - chatadmin
# Use this user instead of root
```

### 3. Disable Root SSH

```bash
sudo nano /etc/ssh/sshd_config
# Set: PermitRootLogin no
sudo systemctl restart sshd
```

### 4. Enable Auto-Updates

```bash
sudo apt install unattended-upgrades
sudo dpkg-reconfigure --priority=low unattended-upgrades
```

### 5. Install Fail2Ban

```bash
sudo apt install fail2ban
sudo systemctl enable fail2ban
```

## Backup Strategy

### Backup Certificates

```bash
# Create backup directory
mkdir -p ~/backups

# Backup certificates
sudo tar -czf ~/backups/letsencrypt-$(date +%Y%m%d).tar.gz /etc/letsencrypt

# Backup server config
cp ~/rust_chat/deploy/digital_ocean/start-server.sh ~/backups/
```

### Restore Certificates

```bash
# Restore from backup
sudo tar -xzf ~/backups/letsencrypt-YYYYMMDD.tar.gz -C /

# Restart server
tmux kill-session -t chat
cd ~/rust_chat/deploy/digital_ocean
./start-server.sh
```

## Performance Tuning

### Recommended Droplet Sizes

| Users | Droplet Size | Cost/Month |
|-------|--------------|------------|
| <50   | Basic (1GB)  | $6         |
| 50-200| Basic (2GB)  | $12        |
| 200-500| Basic (4GB) | $24        |
| 500+  | CPU-Opt (4GB)| $42       |

Monitor with `htop` and adjust as needed.

## Complete Workflow

```bash
# === One-Time Setup ===
ssh root@your-droplet
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
git clone https://github.com/yourusername/rust_chat.git
cd rust_chat/deploy/digital_ocean
sudo ./setup-certificates.sh
nano start-server.sh  # Update domain
sudo ufw allow 22/tcp
sudo ufw allow 8443/tcp
sudo ufw enable
./start-server.sh

# === Daily Usage ===
tmux attach -t chat          # Use server commands
Ctrl+B, D                    # Detach
tmux kill-session -t chat    # Stop server

# === Updates ===
cd ~/rust_chat
git pull
tmux kill-session -t chat
cd deploy/digital_ocean
./start-server.sh
```

## tmux Quick Reference

```bash
# List sessions
tmux ls

# Attach to session
tmux attach -t chat

# Detach from session
Ctrl+B, then D

# Kill session
tmux kill-session -t chat

# Scroll up
Ctrl+B, then [
# Use arrow keys, press Q to exit

# Split window horizontally
Ctrl+B, then "

# Split window vertically
Ctrl+B, then %

# Switch between panes
Ctrl+B, then arrow keys
```

## FAQ

**Q: Can I run multiple chat servers?**

Yes, use different ports and tmux sessions:
```bash
# Copy script
cp start-server.sh start-server2.sh

# Edit for different port and session
nano start-server2.sh
# Change: TMUX_SESSION="chat2"
# Change: SERVER_ADDR="0.0.0.0:9443"

# Open firewall
sudo ufw allow 9443/tcp

# Start
./start-server2.sh
```

**Q: How do I see who's connected?**

Attach to tmux and type `/list`

**Q: Server crashed, how to auto-restart?**

Add to crontab:
```bash
crontab -e
# Add: */5 * * * * tmux has-session -t chat || cd ~/rust_chat/deploy/digital_ocean && ./start-server.sh
```

**Q: How do I renew certificates?**

They renew automatically. To force renewal:
```bash
sudo certbot renew
tmux kill-session -t chat
./start-server.sh
```

**Q: Can I use this on AWS/GCP?**

Yes! Same scripts work on any Ubuntu server.

## Support

- **Server Issues:** Check `tmux attach -t chat` for error messages
- **TLS Issues:** Verify certificates exist: `sudo ls /etc/letsencrypt/live/chat.yourdomain.com/`
- **Connection Issues:** Check firewall `sudo ufw status` and port `ss -tlnp | grep 8443`
- **Performance:** Monitor with `htop` and `/list` command

## Next Steps

- Set up monitoring alerts
- Configure automatic backups
- Implement rate limiting (already built-in)
- Set up log rotation

---

**Pro Tip:** Use tmux split-window to monitor server and run commands simultaneously. Press `Ctrl+B` then `"` to split!
