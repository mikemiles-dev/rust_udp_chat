# Quick Start - Digital Ocean Deployment

Get your Rust chat server running on Digital Ocean in minutes.

## One-Command Setup

After creating your droplet and pointing DNS to it:

```bash
ssh root@your-droplet-ip

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Clone and setup
git clone https://github.com/yourusername/rust_chat.git
cd rust_chat/deploy/digital_ocean

# Get TLS certificates
sudo ./setup-certificates.sh
# Enter your domain and email when prompted

# Update domain in start-server.sh
nano start-server.sh
# Change: TLS_CERT_PATH="/etc/letsencrypt/live/YOUR-DOMAIN/fullchain.pem"
# Change: TLS_KEY_PATH="/etc/letsencrypt/live/YOUR-DOMAIN/privkey.pem"

# Configure firewall
sudo ufw allow 22/tcp
sudo ufw allow 8443/tcp
sudo ufw enable

# Start server
./start-server.sh
```

Done! Your server is running with TLS encryption.

## Connect from Client

```bash
# On your local machine
cargo build --release --bin client
./target/release/client

# When prompted:
# Server: tls://your-domain.com:8443
# Username: YourName
```

## Management Commands

```bash
# View server
tmux attach -t chat

# Detach (keeps running)
Ctrl+B, then D

# Stop server
tmux kill-session -t chat

# Restart server
tmux kill-session -t chat && ./start-server.sh

# Update server
cd ~/rust_chat && git pull && cd deploy/digital_ocean && tmux kill-session -t chat && ./start-server.sh
```

## Files

- `setup-certificates.sh` - Gets Let's Encrypt certificates
- `start-server.sh` - Starts server in tmux with TLS
- `README.md` - Full documentation

## Troubleshooting

**Can't connect?**
```bash
# Check server is running
tmux attach -t chat

# Check firewall
sudo ufw status

# Test TLS
openssl s_client -connect your-domain.com:8443
```

**No TLS?**
```bash
# Verify certificates exist
sudo ls /etc/letsencrypt/live/your-domain.com/

# Check paths in start-server.sh match your domain
nano start-server.sh
```

For detailed documentation, see [README.md](README.md)
