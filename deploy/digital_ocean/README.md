# Digital Ocean Deployment with Native TLS

This guide shows how to deploy the Rust chat server on Digital Ocean with native TLS support.

## Architecture

```
Internet (TLS, port 8443)
    ↓
Rust Chat Server with native TLS (in tmux session)
```

**Note:** The server now has **native TLS support** built-in. No reverse proxy needed!

## Prerequisites

- Digital Ocean Droplet (Ubuntu 22.04 recommended)
- Domain name pointing to your droplet's IP
- SSH access to your droplet

## Quick Start

### 1. Create Droplet

1. Log into Digital Ocean
2. Create new Droplet
3. Choose **Ubuntu 22.04 LTS**
4. Select size (Basic $6/month is fine for 100 users)
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
# Install Certbot
sudo apt update
sudo apt install -y certbot

# Get certificates (replace with your domain and email)
sudo certbot certonly --standalone \
  -d chat.yourdomain.com \
  -m your-email@example.com \
  --agree-tos \
  --non-interactive

# Certificates will be at:
# /etc/letsencrypt/live/chat.yourdomain.com/fullchain.pem
# /etc/letsencrypt/live/chat.yourdomain.com/privkey.pem
```

**Note:** Port 80 must be open temporarily for certificate verification.

### 7. Configure Firewall

```bash
# Allow SSH, and chat server port
sudo ufw allow 22/tcp
sudo ufw allow 8443/tcp
sudo ufw enable
```

### 8. Start Chat Server

```bash
cd ~/rust_chat/deploy/digital_ocean
./start-server.sh
```

This will:
- Build the server in release mode
- Start it in a tmux session named `chat`
- Bind to 0.0.0.0:8443 with TLS

### 9. Attach to Server Console

```bash
tmux attach -t chat
```

Now you can use interactive commands:
```
/list              # Show connected users
/kick username     # Kick a user
/help              # Show commands
/quit              # Shutdown server
```

### 10. Detach from Session

**Press:** `Ctrl+B`, then `D`

The server keeps running in the background!

## Management

### Start Server

```bash
cd ~/rust_chat/deploy/digital_ocean
./start-server.sh
```

### Attach to Running Server

```bash
tmux attach -t chat
```

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
# Server logs (attach to tmux)
tmux attach -t chat

# Or view system journal if using systemd
sudo journalctl -u rust-chat -f
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
cd digital_ocean
./start-server.sh
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

### Can't connect via TLS

```bash
# Check server is running
tmux attach -t chat

# Check if certificates exist
ls -la /etc/letsencrypt/live/chat.yourdomain.com/

# Check firewall
sudo ufw status

# Test connection
openssl s_client -connect chat.yourdomain.com:8443

# Common issue: DNS not propagated yet
dig chat.yourdomain.com  # Should show your droplet IP
```

### Port already in use

```bash
# See what's using port 8443
sudo netstat -tlnp | grep 8443

# Kill the process
sudo kill <PID>
```

### Firewall blocking connections

```bash
# Check firewall status
sudo ufw status

# Should show:
# 22/tcp   ALLOW
# 8443/tcp ALLOW

# If not, allow the port
sudo ufw allow 8443/tcp
```

### Can't attach to tmux

```bash
# List sessions
tmux ls

# If no sessions, server isn't running
./start-server.sh

# If session exists but can't attach
tmux attach -t chat
```

## tmux Cheat Sheet

```bash
# Create new session
tmux new -s chat

# List sessions
tmux ls

# Attach to session
tmux attach -t chat

# Detach from session
Ctrl+B, then D

# Kill session
tmux kill-session -t chat

# Scroll up in tmux
Ctrl+B, then [
# Use arrow keys or Page Up/Down
# Press Q to exit scroll mode

# Split window horizontally
Ctrl+B, then "

# Split window vertically
Ctrl+B, then %

# Switch between panes
Ctrl+B, then arrow keys

# Rename session
Ctrl+B, then $
```

## Configuration

### Change Server Port

Edit `start-server.sh`:
```bash
SERVER_ADDR="0.0.0.0:9443"  # Change port here
```

Then update firewall:
```bash
sudo ufw allow 9443/tcp
sudo ufw delete allow 8443/tcp
```

### Change Max Clients

Edit `start-server.sh`:
```bash
MAX_CLIENTS="500"  # Change this
```

### Change Domain

Get new certificates for the new domain:
```bash
sudo certbot certonly --standalone -d newdomain.com
```

Update `start-server.sh`:
```bash
TLS_CERT_PATH="/etc/letsencrypt/live/newdomain.com/fullchain.pem"
TLS_KEY_PATH="/etc/letsencrypt/live/newdomain.com/privkey.pem"
```

## Security Best Practices

### 1. Use SSH Keys (not passwords)

```bash
# On your local machine, generate key if needed
ssh-keygen -t ed25519

# Copy to droplet
ssh-copy-id root@your-droplet-ip
```

### 2. Create Non-Root User

```bash
# On droplet
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

### 5. Monitor Failed Login Attempts

```bash
sudo apt install fail2ban
sudo systemctl enable fail2ban
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
sudo netstat -an | grep :8443 | grep ESTABLISHED | wc -l

# Or use ss
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
# Create simple monitoring script
cat > ~/monitor-chat.sh << 'MONITOR_EOF'
#!/bin/bash
CONNECTIONS=$(ss -tn | grep :8443 | wc -l)
if [ $CONNECTIONS -gt 50 ]; then
    echo "ALERT: $CONNECTIONS connections detected at $(date)" | tee -a ~/alerts.log
fi
MONITOR_EOF
chmod +x ~/monitor-chat.sh

# Run every 5 minutes
crontab -e
# Add: */5 * * * * ~/monitor-chat.sh
```

## Backup Strategy

### Backup Important Files

```bash
# Create backup directory
mkdir -p ~/backups

# Backup TLS certificates (if needed to migrate)
sudo tar -czf ~/backups/letsencrypt-certs-$(date +%Y%m%d).tar.gz /etc/letsencrypt

# Backup start script configuration
cp ~/rust_chat/deploy/digital_ocean/start-server.sh ~/backups/

# Backup your code
cd ~/rust_chat
git bundle create ~/backups/rust_chat-$(date +%Y%m%d).bundle --all
```

### Restore Process

```bash
# Restore certificates
sudo tar -xzf ~/backups/letsencrypt-certs-YYYYMMDD.tar.gz -C /

# Restart server
tmux kill-session -t chat
cd ~/rust_chat/deploy/digital_ocean
./start-server.sh
```

## Cost Optimization

### Right-Size Your Droplet

| Users | Droplet Size | Cost/Month |
|-------|--------------|------------|
| <50   | Basic (1GB)  | $6         |
| 50-150| Basic (2GB)  | $12        |
| 150-500| Basic (4GB) | $24        |
| 500+  | CPU-Opt (4GB)| $42       |

Monitor with:
```bash
htop  # Check if you're using all RAM
```

Downsize/upsize as needed (requires droplet restart).

## Connecting Clients

### From Client App

```bash
cargo run --bin client
# Enter server: tls://chat.yourdomain.com:8443
# Enter username: YourName
```

### Building Client for Distribution

```bash
# On your local machine
cargo build --release --bin client

# Binary is at: target/release/client
# Share this with users
```

## Complete Workflow

```bash
# === One-Time Setup ===
ssh root@your-droplet
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
git clone https://github.com/yourusername/rust_chat.git
cd rust_chat

# Get TLS certificates
sudo apt install certbot
sudo certbot certonly --standalone -d chat.yourdomain.com

# Configure firewall
sudo ufw allow 22/tcp
sudo ufw allow 8443/tcp
sudo ufw enable

# === Daily Usage ===
cd ~/rust_chat/deploy/digital_ocean
./start-server.sh            # Start server
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

## FAQ

**Q: Can I run multiple chat servers?**
A: Yes, use different ports and tmux sessions:
```bash
# Copy and edit start-server.sh:
cp start-server.sh start-server2.sh

# In start-server2.sh, change:
TMUX_SESSION="chat2"
SERVER_ADDR="0.0.0.0:9443"

# Get certificates for second domain and update paths
# Open firewall for new port
sudo ufw allow 9443/tcp
```

**Q: How do I see who's connected?**
A: Attach to tmux and type `/list`

**Q: Server crashed, how to auto-restart?**
A: Use systemd instead (see `deploy/NATIVE_DEPLOYMENT.md`) or add to crontab:
```bash
*/5 * * * * tmux has-session -t chat || cd ~/rust_chat/deploy/digital_ocean && ./start-server.sh
```

**Q: How do I renew certificates?**
A: Certificates auto-renew. Or manually: `sudo certbot renew`, then restart server.

**Q: Can I use this on AWS/GCP?**
A: Yes! Same scripts work on any Ubuntu server.

## Support

- **Application Issues**: Check `tmux attach -t chat` for error messages
- **TLS Issues**: Verify certificates exist: `ls -la /etc/letsencrypt/live/chat.yourdomain.com/`
- **Connection Issues**: Check firewall `sudo ufw status` and port `ss -tlnp | grep 8443`
- **Performance**: Monitor with `htop` and `/list` command

## Next Steps

- Set up monitoring alerts
- Configure automatic backups
- Add Cloudflare for DDoS protection (see `BAN_STRATEGY.md`)
- Implement IP banning for abuse
- Set up log rotation

---

**Pro Tip:** Use `tmux split-window` to watch server output and run commands in the same window! Press `Ctrl+B` then `"` to split horizontally.
