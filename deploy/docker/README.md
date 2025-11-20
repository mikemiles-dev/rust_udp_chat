# Docker Deployment

This directory contains all files needed for Docker deployment with automatic HTTPS via Caddy.

## Quick Start

1. **Edit Caddyfile**:
   ```bash
   nano Caddyfile
   ```
   Replace:
   - `your-email@example.com` with your email
   - `chat.yourdomain.com` with your domain

2. **Deploy**:
   ```bash
   docker-compose up -d
   ```

3. **View Logs**:
   ```bash
   docker-compose logs -f
   ```

## Files

- **Dockerfile** - Multi-stage build for Rust chat server
- **docker-compose.yml** - Orchestrates chat server + Caddy
- **Caddyfile** - Caddy reverse proxy configuration with auto-HTTPS
- **.dockerignore** - Optimizes Docker build context
- **DEPLOYMENT.md** - Complete deployment guide

## Requirements

- Docker
- Docker Compose
- Domain name pointing to your server

## Architecture

```
Internet (443/80) → Caddy (TLS) → Chat Server (8080)
                      ↓
                Let's Encrypt
```

See [DEPLOYMENT.md](DEPLOYMENT.md) for detailed instructions.
