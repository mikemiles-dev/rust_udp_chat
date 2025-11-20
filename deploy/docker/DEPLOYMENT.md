# Deployment Guide

## Quick Start: Digital Ocean Deployment

### 1. Create Droplet

- **Image**: Docker on Ubuntu 22.04
- **Plan**: Basic ($6/month is sufficient for 100 users)
- **Region**: Choose closest to your users
- **Authentication**: SSH key recommended

### 2. Configure DNS

Point your domain to the droplet:
```
Type: A Record
Host: chat (or @)
Value: <your-droplet-ip>
TTL: 3600
```

Wait for DNS propagation (use `dig chat.yourdomain.com` to check).

### 3. SSH into Droplet

```bash
ssh root@<your-droplet-ip>
```

### 4. Clone Repository

```bash
git clone https://github.com/yourusername/rust_chat.git
cd rust_chat
```

### 5. Configure Caddy

Edit the `Caddyfile`:
```bash
nano Caddyfile
```

Replace:
- `your-email@example.com` → Your actual email (for Let's Encrypt)
- `chat.yourdomain.com` → Your actual domain

### 6. Deploy

```bash
docker-compose up -d
```

That's it! Caddy will automatically:
- Obtain Let's Encrypt certificates
- Configure HTTPS
- Set up automatic renewal

### 7. Verify Deployment

Check logs:
```bash
docker-compose logs -f
```

You should see:
```
caddy_proxy     | ... certificate obtained successfully
chat_server     | [OK] Chat Server started at 0.0.0.0:8080
```

Check certificates:
```bash
docker exec caddy_proxy caddy list-certificates
```

### 8. Test Connection

From your local machine:
```bash
cargo run --bin client
# Enter: chat.yourdomain.com:443
```

## Firewall Configuration

If using UFW (Ubuntu):
```bash
# Allow SSH
ufw allow 22/tcp

# Allow HTTP (for Let's Encrypt challenges)
ufw allow 80/tcp

# Allow HTTPS
ufw allow 443/tcp

# Enable firewall
ufw enable
```

## Monitoring

### View Logs

```bash
# All logs
docker-compose logs -f

# Just server
docker-compose logs -f chat_server

# Just Caddy
docker-compose logs -f caddy
```

### Resource Usage

```bash
# Check container stats
docker stats

# Check disk usage
docker system df
```

### Connected Users

Access the server console:
```bash
docker exec -it rust_chat_server /bin/bash
# (Server doesn't have interactive shell - check logs instead)
```

View active connections in logs:
```bash
docker-compose logs chat_server | grep "joined\|left"
```

## Updating

### Update Code

```bash
cd rust_chat
git pull
docker-compose up -d --build
docker image prune -f
```

### Update Dependencies

```bash
# Edit Cargo.toml to update versions
nano Cargo.toml

# Rebuild
docker-compose build --no-cache
docker-compose up -d
```

## Backup

### Certificate Backup

Certificates are stored in Docker volumes. Back them up:
```bash
# Create backup directory
mkdir -p ~/backups

# Backup certificates
docker run --rm -v rust_chat_caddy_data:/data -v ~/backups:/backup \
  alpine tar czf /backup/caddy-certs-$(date +%Y%m%d).tar.gz /data
```

### Restore Certificates

```bash
docker run --rm -v rust_chat_caddy_data:/data -v ~/backups:/backup \
  alpine sh -c "cd /data && tar xzf /backup/caddy-certs-YYYYMMDD.tar.gz --strip-components=1"
```

## Troubleshooting

### Certificate Issues

If Let's Encrypt fails:
```bash
# Check Caddy logs
docker-compose logs caddy | grep -i error

# Common issues:
# 1. DNS not propagated - wait longer
# 2. Port 80 blocked - check firewall
# 3. Rate limit hit - wait 1 hour
```

### Connection Issues

```bash
# Test if server is listening
curl -v https://chat.yourdomain.com

# Check internal connectivity
docker exec caddy_proxy wget -O- http://chat_server:8080
```

### Performance Issues

```bash
# Check resource usage
docker stats

# Increase max clients
# Edit docker-compose.yml:
CHAT_SERVER_MAX_CLIENTS=500

# Restart
docker-compose up -d
```

## Security Hardening

### 1. Enable Automatic Updates

```bash
apt install unattended-upgrades
dpkg-reconfigure --priority=low unattended-upgrades
```

### 2. Install Fail2ban

```bash
apt install fail2ban
systemctl enable fail2ban
```

### 3. Disable Root Login

```bash
nano /etc/ssh/sshd_config
# Set: PermitRootLogin no
systemctl restart sshd
```

### 4. Rate Limiting (Caddy)

Already configured in the application (10 msg/sec per user).

To add Caddy-level rate limiting, edit `Caddyfile`:
```
chat.yourdomain.com {
    rate_limit {
        zone dynamic {
            key {remote_host}
            events 100
            window 1m
        }
    }
    reverse_proxy chat_server:8080
}
```

## Cost Estimates

### Digital Ocean
- **Basic Droplet**: $6/month (1 GB RAM, 1 vCPU)
- **Domain**: ~$12/year
- **Bandwidth**: Included (1 TB)

**Total**: ~$7/month

### Scalability

- 1 GB RAM: ~100 concurrent users
- 2 GB RAM: ~250 concurrent users
- 4 GB RAM: ~500+ concurrent users

## Alternative Hosting

### AWS EC2
```bash
# t3.micro (free tier eligible)
# Same Docker setup works
```

### Google Cloud Run
```bash
# Not recommended - needs persistent WebSocket connections
```

### Self-hosted (Home Server)
```bash
# Use Tailscale or Cloudflare Tunnel for secure access
# Same Docker setup
```

## Production Checklist

- [ ] Domain configured and DNS propagated
- [ ] Caddyfile updated with domain and email
- [ ] Firewall configured (ports 80, 443)
- [ ] Docker containers running
- [ ] HTTPS certificate obtained
- [ ] Test connection successful
- [ ] Monitoring/logging in place
- [ ] Backup strategy defined
- [ ] Automatic updates enabled
- [ ] Security hardening applied

## Support

For issues, check:
1. Application logs: `docker-compose logs -f`
2. Caddy docs: https://caddyserver.com/docs/
3. Let's Encrypt status: https://letsencrypt.status.io/
