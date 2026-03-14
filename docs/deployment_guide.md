# Clara PoC — Deployment Guide

## Overview

The Clara demo stack runs on the same AWS EC2 instance that hosts the Seashell corporate
website. Services run as Docker containers behind an nginx reverse proxy. Only the
frontdesk demo and EdgeQuake are reachable from the public internet; all other services
(clara-api, FieryPit, MCP adapters) are internal to the Docker network.

**Stack summary:**

| Service | Port | Public? |
|---------|------|---------|
| clara-api | 8080 | No — internal |
| prolog-mcp-adapter | 1968 | No — internal |
| clips-mcp-adapter | 1951 | No — internal |
| clara-frontdesk (demo) | 8088 | Yes — `/frontdesk/` |
| lildaemon (FieryPit) | 6666 | No — internal |
| edgequake-api | 8082 | Auth only — `/edgequake/api/` |
| edgequake-frontend | 3000 | Auth only — `/edgequake/` |
| edgequake-postgres | 5432 | No — internal |

**EC2 instance:** `ec2-54-177-89-105.us-west-1.compute.amazonaws.com`
**SSH key:** `~/vastness/.ssh/SeashellAnalytics_220325.pem`
**Demo is not always running** — start it on demand, stop it when done.

---

## First-Time Setup

Perform these steps once before the first deployment.

### 1. Resize the EC2 Instance

The full stack (Clara + EdgeQuake + PostgreSQL) requires at least 8 GB RAM and 40 GB
disk. The current instance is likely undersized.

1. Open the AWS Console → EC2 → Instances
2. Stop the instance
3. Actions → Instance Settings → Change Instance Type → select **t3.large** (8 GB) or
   **t3.xlarge** (16 GB, recommended if budget allows)
4. While stopped: Actions → Instance Settings → Modify EBS Volume → resize to **≥ 40 GB**
5. Start the instance

After starting, SSH in and resize the filesystem to use the new disk space:

```bash
ssh -i ~/vastness/.ssh/SeashellAnalytics_220325.pem ec2-user@ec2-54-177-89-105.us-west-1.compute.amazonaws.com

# Verify the block device name (usually xvda or nvme0n1)
lsblk

# Grow the partition and filesystem (AL2023)
sudo growpart /dev/xvda 1
sudo xfs_growfs /
df -h /   # confirm new size
```

### 2. Install Docker

```bash
sudo yum install -y docker
sudo systemctl enable docker
sudo systemctl start docker
sudo usermod -aG docker ec2-user
sudo yum install -y docker-compose-plugin
```

Log out and back in so the `docker` group takes effect:

```bash
exit
ssh -i ~/vastness/.ssh/SeashellAnalytics_220325.pem ec2-user@ec2-54-177-89-105.us-west-1.compute.amazonaws.com
docker info   # should work without sudo
```

### 3. Create the Deployment Directory

```bash
sudo mkdir -p /opt/clara
sudo chown ec2-user:ec2-user /opt/clara
```

### 4. Create Environment Files

These files hold secrets and are **never committed to git**. Create them directly on
the EC2 instance.

**Clara stack** (`/opt/clara/clara-cerebrum/docker/.env`):

```bash
mkdir -p /opt/clara/clara-cerebrum/docker
cat > /opt/clara/clara-cerebrum/docker/.env << 'EOF'
JWT_SECRET=<generate a long random string>
GROQ_API_KEY=<your Groq API key>
EOF
```

Generate a JWT secret:
```bash
openssl rand -hex 32
```

**EdgeQuake** (`/opt/clara/edgequake.env`):

```bash
cat > /opt/clara/edgequake.env << 'EOF'
EDGEQUAKE_PORT=8082
FRONTEND_PORT=3000
EDGEQUAKE_DEFAULT_LLM_PROVIDER=groq
GROQ_API_KEY=<your Groq API key>
EOF
```

### 5. Configure nginx

Deploy the updated Seashell nginx config (which now includes the Clara proxy rules)
using the existing Seashell deploy script from your local machine:

```bash
cd ~/Development/seashell
./deploy/deploy.sh
```

Then on the EC2 instance, install the Clara config and set up basic auth for EdgeQuake:

```bash
# Copy the Clara nginx config
sudo cp /opt/clara/clara-cerebrum/deploy/nginx/clara.conf /etc/nginx/conf.d/clara.conf

# Set up EdgeQuake password (you'll be prompted to enter a password)
sudo yum install -y httpd-tools
sudo htpasswd -c /etc/nginx/.htpasswd demo

# Test nginx config and reload
sudo nginx -t
sudo systemctl reload nginx
```

---

## First Deployment

Run this from your **local machine** inside the `clara-cerebrum` directory.

### 1. Set Up the Env File Locally

Copy the example template and fill it in (this is only needed locally if you want to
test the compose file before deploying — the real secrets live on EC2):

```bash
cp docker/.env.example docker/.env
# edit docker/.env with your secrets
```

### 2. Run the Deploy Script

```bash
cd ~/Development/clara-cerebrum
./scripts/deploy-clara.sh
```

This will:
1. rsync `clara-cerebrum/`, `lildaemon/`, and `edgequake/` to `/opt/clara/` on EC2
2. rsync the `models/` directory (818 MB FastText model — slow first time, checksum-based after)
3. Place `.dockerignore` at the build context root
4. Run `docker compose build` on EC2 (first build takes **30–45 minutes** due to SWI-Prolog compiling from source)
5. Start all containers

Watch the build progress:

```bash
ssh -i ~/vastness/.ssh/SeashellAnalytics_220325.pem ec2-user@ec2-54-177-89-105.us-west-1.compute.amazonaws.com
cd /opt/clara
docker compose -f clara-cerebrum/docker/docker-compose.yml logs -f
```

### 3. Start EdgeQuake

EdgeQuake is managed separately from the Clara stack:

```bash
# On EC2:
cd /opt/clara/edgequake
docker compose -f docker/docker-compose.yml --env-file /opt/clara/edgequake.env up -d
```

### 4. Verify

From a browser:
- Frontdesk demo: `http://ec2-54-177-89-105.us-west-1.compute.amazonaws.com/frontdesk/`
- EdgeQuake: `http://ec2-54-177-89-105.us-west-1.compute.amazonaws.com/edgequake/` (enter password when prompted)

From the EC2 instance (internal health checks):
```bash
curl http://localhost:8080/health   # clara-api
curl http://localhost:6666/health   # FieryPit
curl http://localhost:8082/health   # EdgeQuake API
```

---

## Maintenance Deployments

### Full Update (code changed)

Run from your local machine:

```bash
cd ~/Development/clara-cerebrum
./scripts/deploy-clara.sh
```

The script syncs all three repos, rebuilds Docker images, and restarts containers.
Subsequent builds are much faster (~5–10 min) because Docker caches the SWI-Prolog
and CLIPS compilation layers — they only rebuild if `clara-prolog/` or `clara-clips/`
source changes.

To force a full rebuild from scratch:

```bash
./scripts/deploy-clara.sh --no-cache
```

To sync files without rebuilding (e.g. to update a config):

```bash
./scripts/deploy-clara.sh --sync-only
```

### Update a Single Service

SSH into EC2 and rebuild only the affected container:

```bash
ssh -i ~/vastness/.ssh/SeashellAnalytics_220325.pem ec2-user@ec2-54-177-89-105.us-west-1.compute.amazonaws.com
cd /opt/clara

# Rebuild and restart just one service (e.g. after a frontdesk config change)
docker compose -f clara-cerebrum/docker/docker-compose.yml \
    --env-file clara-cerebrum/docker/.env \
    up -d --build clara-frontdesk

# Or restart without rebuilding (picks up env var changes)
docker compose -f clara-cerebrum/docker/docker-compose.yml \
    --env-file clara-cerebrum/docker/.env \
    restart clara-frontdesk
```

### Update nginx Config

The nginx config is deployed as part of the Seashell site deploy. After changing
`seashell/deploy/nginx/seashell.conf` or `clara.conf`:

```bash
# From local machine
cd ~/Development/seashell
./deploy/deploy.sh

# Then on EC2 (or included in the seashell deploy script):
sudo nginx -t && sudo systemctl reload nginx
```

---

## Demo Start / Stop

The demo stack is not meant to run continuously. Start it before a demo, stop it after.

### Start

SSH into EC2:

```bash
ssh -i ~/vastness/.ssh/SeashellAnalytics_220325.pem ec2-user@ec2-54-177-89-105.us-west-1.compute.amazonaws.com

# Start Clara stack
cd /opt/clara
docker compose -f clara-cerebrum/docker/docker-compose.yml --env-file clara-cerebrum/docker/.env up -d

# Start EdgeQuake
docker compose -f edgequake/docker/docker-compose.yml --env-file /opt/clara/edgequake.env up -d

# Check status
docker ps
```

### Stop

```bash
# Stop Clara stack
cd /opt/clara
docker compose -f clara-cerebrum/docker/docker-compose.yml down

# Stop EdgeQuake
docker compose -f edgequake/docker/docker-compose.yml down
```

---

## Logs and Monitoring

```bash
# Tail all Clara service logs
docker compose -f /opt/clara/clara-cerebrum/docker/docker-compose.yml logs -f

# Tail a specific service
docker compose -f /opt/clara/clara-cerebrum/docker/docker-compose.yml logs -f clara-api
docker compose -f /opt/clara/clara-cerebrum/docker/docker-compose.yml logs -f lildaemon
docker compose -f /opt/clara/clara-cerebrum/docker/docker-compose.yml logs -f clara-frontdesk

# EdgeQuake logs
docker compose -f /opt/clara/edgequake/docker/docker-compose.yml logs -f

# nginx logs
sudo tail -f /var/log/nginx/access.log
sudo tail -f /var/log/nginx/error.log
```

---

## Troubleshooting

### Container won't start

```bash
docker compose -f /opt/clara/clara-cerebrum/docker/docker-compose.yml ps
docker compose -f /opt/clara/clara-cerebrum/docker/docker-compose.yml logs <service-name>
```

### SWI-Prolog initialization fails at startup

If clara-api logs show `PL_initialise returned 0` or `SWI_HOME_DIR` errors:

```bash
# Check that SWI_HOME_DIR is set inside the container
docker exec clara-api env | grep SWI
docker exec clara-api ls /opt/swipl/home

# Check libswipl.so is loadable
docker exec clara-api ldd /usr/local/bin/clara-api | grep swipl
```

### Frontdesk can't reach clara-api or FieryPit

The services communicate over the `clara-net` Docker bridge. Verify the network:

```bash
docker network inspect clara-cerebrum_clara-net
docker exec clara-frontdesk curl -s http://clara-api:8080/health
docker exec clara-frontdesk curl -s http://lildaemon:6666/health
```

### EdgeQuake database issues

```bash
# Check PostgreSQL is healthy
docker compose -f /opt/clara/edgequake/docker/docker-compose.yml ps
docker exec edgequake-postgres psql -U edgequake -c '\l'

# Reset database (destructive — wipes all data)
docker compose -f /opt/clara/edgequake/docker/docker-compose.yml down -v
docker compose -f /opt/clara/edgequake/docker/docker-compose.yml --env-file /opt/clara/edgequake.env up -d
```

### Disk space

The Docker build cache and PostgreSQL data volume can grow large:

```bash
df -h
docker system df

# Clean up stopped containers, dangling images, unused build cache
docker system prune -f

# More aggressive: remove all unused images (frees build cache too — next build will be slow)
docker system prune -af
```

### Rotating secrets

Update `.env` on EC2 directly, then restart the affected services:

```bash
nano /opt/clara/clara-cerebrum/docker/.env

docker compose -f /opt/clara/clara-cerebrum/docker/docker-compose.yml \
    --env-file /opt/clara/clara-cerebrum/docker/.env \
    up -d
```

---

## File Reference

```
Development/
├── clara-cerebrum/
│   ├── docker/
│   │   ├── Dockerfile           # Multi-stage Rust build (all 4 services)
│   │   ├── Dockerfile.lildaemon # Python FieryPit service
│   │   ├── docker-compose.yml   # Clara stack composition
│   │   ├── .dockerignore        # Exclude large dirs from build context
│   │   ├── .env.example         # Secret template (copy to .env)
│   │   └── .env                 # Live secrets — DO NOT COMMIT
│   └── scripts/
│       └── deploy-clara.sh      # Sync + build + deploy
├── lildaemon/
│   └── config/
│       └── evaluators.yaml      # FieryPit evaluator registry
└── seashell/
    └── deploy/
        └── nginx/
            ├── seashell.conf    # Main nginx server block
            └── clara.conf       # Clara proxy rules (included by seashell.conf)
```

Remote layout on EC2:

```
/opt/clara/
├── clara-cerebrum/      # synced from local Development/clara-cerebrum/
├── lildaemon/           # synced from local Development/lildaemon/
├── edgequake/           # synced from local Development/edgequake/
├── .dockerignore        # placed by deploy script
└── edgequake.env        # EdgeQuake secrets (created manually on EC2)
```
