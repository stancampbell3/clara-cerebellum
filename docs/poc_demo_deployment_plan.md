# Clara PoC Deployment Plan (rev 2)

## Context

Clara is in demo phase and needs to be deployable on the existing AWS EC2 instance
(`ec2-54-176-157-222.us-west-1.compute.amazonaws.com`, AL2023, us-west-1) that already
hosts the Seashell website via nginx. We're containerizing the Clara stack for easy
on-demand start/stop and to manage native CLIPS/SWI-Prolog dependencies.

### Service Summary

Four main processes:
- **clara-api** (Rust) — main REST API; includes clara-transduction and clara-dagda;
  hosts the FastText model (`dagda-0.2.bin`) used by the `clara_fy` Prolog predicate
- **prolog-mcp-adapter** (Rust, HTTP mode) — MCP server for Prolog engine
- **clips-mcp-adapter** (Rust, HTTP mode) — MCP server for CLIPS engine
- **lildaemon** (Python) — FieryPit REST server

Plus **EdgeQuake** (separate Docker Compose stack, already Groq-configured).

### Public Access Policy

| Endpoint | Access | Reason |
|----------|--------|--------|
| Seashell corporate site `/` | Public | Existing |
| Frontdesk demo `/frontdesk/` | Public | Linked from corporate site |
| EdgeQuake `/edgequake/` | Nginx basic auth | Nice visual for invited viewers |
| clara-api, FieryPit, MCP adapters | Firewall only (no nginx proxy) | Internal only; shell in when needed |

---

## SSH/Instance Reference

- Host: `ec2-54-176-157-222.us-west-1.compute.amazonaws.com`
- User: `ec2-user`
- Key: `~/vastness/.ssh/SeashellAnalytics_220325.pem`
- Deploy pattern: `seashell/deploy/deploy.sh`

---

## Port Map

| Service               | Port  | Exposed publicly? |
|-----------------------|-------|-------------------|
| nginx                 | 80/443| Yes (security group) |
| clara-api             | 8080  | No — container network only |
| prolog-mcp-adapter    | 1968  | No — container network only |
| clips-mcp-adapter     | 1951  | No — container network only |
| clara-frontdesk       | 8088  | Via nginx (public, no auth) |
| lildaemon (FieryPit)  | 6666  | No — container network only |
| edgequake-api         | 8082  | Via nginx (basic auth) |
| edgequake-frontend    | 3000  | Via nginx (basic auth) |
| edgequake-postgres    | 5432  | No — container network only |

AWS Security Group: only ports 22, 80, 443 open inbound. All inter-service traffic
stays on the Docker internal network.

---

## Step 1: Upsize EC2 Instance

**Action**: Stop instance → resize to **t3.large** (2 vCPU, 8 GB) or **t3.xlarge**
(4 vCPU, 16 GB). Expand EBS volume to ≥40 GB (Docker images + PostgreSQL + 860 MB
FastText model files + build cache).

Install Docker (AL2023):
```bash
sudo yum install -y docker
sudo systemctl enable docker
sudo systemctl start docker
sudo usermod -aG docker ec2-user
sudo yum install -y docker-compose-plugin
```

---

## Step 2: Build Strategy

Both SWI-Prolog and CLIPS are compiled from C/CMake source during `cargo build`.
This means the builder Docker image needs a full build toolchain. Build is slow the
first time (~30–45 min); subsequent builds use Docker layer cache.

All four Rust crates share the same workspace, so we use **one builder image** for
all three Rust binaries. This avoids repeating the expensive SWI-Prolog + CLIPS
compile three times.

### Build context

The lildaemon container needs `lildaemon/` alongside `clara-cerebrum/`. We use
the parent `Development/` directory as the Docker build context, with Dockerfiles
located in `clara-cerebrum/docker/`.

```
Development/                      ← build context root
  clara-cerebrum/
    docker/
      Dockerfile                  ← builds all Rust binaries (builder + runtime targets)
      Dockerfile.lildaemon
      docker-compose.yml
    ...
  lildaemon/
    goat/
    config/
    pyproject.toml
    setup.cfg
```

---

## Step 3: Dockerfile — Rust Services (single builder)

**File**: `clara-cerebrum/docker/Dockerfile`

This replaces the 15-line root `Dockerfile`. All three Rust binaries are built in
one stage; each service gets its own slim runtime image using `COPY --from=builder`.

```dockerfile
# ── Stage 1: Builder ─────────────────────────────────────────────────────────
FROM rust:1.82-bookworm AS builder

RUN apt-get update && apt-get install -y \
    cmake ninja-build build-essential \
    libssl-dev pkg-config \
    libgmp-dev zlib1g-dev libpcre2-dev \
    python3 git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY clara-cerebrum/ .
RUN cargo build --release \
    -p clara-api \
    -p prolog-mcp-adapter \
    -p clips-mcp-adapter \
    -p clara-frontdesk-poc

# ── Stage 2: clara-api runtime ───────────────────────────────────────────────
FROM debian:bookworm-slim AS clara-api
RUN apt-get update && apt-get install -y \
    ca-certificates libssl3 libgmp10 zlib1g \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/clara-api /usr/local/bin/
COPY clara-cerebrum/config /etc/clara/
# models mounted at runtime via volume (860 MB, not baked into image)
EXPOSE 8080 9090
ENV RUST_LOG=info
CMD ["clara-api"]

# ── Stage 3: prolog-mcp runtime ──────────────────────────────────────────────
FROM debian:bookworm-slim AS prolog-mcp
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/prolog-mcp-adapter /usr/local/bin/
ENV REST_API_URL=http://clara-api:8080
ENV TRANSPORT=http
ENV HTTP_PORT=1968
EXPOSE 1968
CMD ["prolog-mcp-adapter"]

# ── Stage 4: clips-mcp runtime ───────────────────────────────────────────────
FROM debian:bookworm-slim AS clips-mcp
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/clips-mcp-adapter /usr/local/bin/
ENV REST_API_URL=http://clara-api:8080
ENV TRANSPORT=http
ENV HTTP_PORT=1951
EXPOSE 1951
CMD ["clips-mcp-adapter"]

# ── Stage 5: frontdesk runtime ───────────────────────────────────────────────
FROM debian:bookworm-slim AS frontdesk
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/clara-frontdesk /usr/local/bin/
COPY clara-cerebrum/clara-frontdesk-poc/static /app/static
COPY clara-cerebrum/clara-frontdesk-poc/roost  /app/roost
COPY clara-cerebrum/clara-frontdesk-poc/config/city_of_dis.toml /etc/frontdesk/city_of_dis.toml
EXPOSE 8088
ENV FRONTDESK_CONFIG=/etc/frontdesk/city_of_dis.toml
CMD ["clara-frontdesk"]
```

Note: The FastText model files (`dagda-0.2.bin`, `dagda-0.2.vec`) are **not baked
into the image** — they're volume-mounted at runtime. This keeps image size manageable
and lets us update models independently.

---

## Step 4: Dockerfile — lildaemon

**File**: `clara-cerebrum/docker/Dockerfile.lildaemon`

Build context root is `Development/` (one level up from clara-cerebrum).

```dockerfile
FROM python:3.11-slim

RUN apt-get update && apt-get install -y build-essential && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Install lildaemon package
COPY lildaemon/pyproject.toml lildaemon/setup.cfg /app/lildaemon/
COPY lildaemon/goat /app/lildaemon/goat
RUN pip install --no-cache-dir -e /app/lildaemon

# Copy runtime config and tools
COPY lildaemon/config /app/config
COPY lildaemon/goat/tools /app/goat/tools

EXPOSE 6666
ENV CEREBELLUM_URL=http://clara-api:8080

CMD ["uvicorn", "goat.app.main:app", "--host", "0.0.0.0", "--port", "6666"]
```

---

## Step 5: Config Fixes

### 5a. `clara-frontdesk-poc/config/city_of_dis.toml`

Update hardcoded local paths and service URLs:

```toml
[paths]
clara_api_url  = "http://clara-api:8080"
fiery_pit_url  = "http://lildaemon:6666"
clara_pl_path  = "/app/roost/front_desk_poc_reprise_clara.pl"
clara_clp_path = "/app/roost/front_desk_poc_reprise_clara.clp"
```

### 5b. `lildaemon/config/evaluators.yaml`

Update `cerebellum_url` from `http://pineal:8080` → `http://clara-api:8080`.
Affects: `clara_mind_splinter`, `prolog`, `clips`, `ember`, `kindling` evaluators.

Ollama URLs (`http://pineal:11434`) can be left as-is — those evaluators simply
won't be selectable in this deployment without an Ollama instance.

---

## Step 6: Docker Compose — Clara Stack

**File**: `clara-cerebrum/docker/docker-compose.yml`

Run from `Development/` directory (build context root):
`docker compose -f clara-cerebrum/docker/docker-compose.yml up -d`

```yaml
version: "3.9"

services:
  clara-api:
    build:
      context: ../..
      dockerfile: clara-cerebrum/docker/Dockerfile
      target: clara-api
    ports:
      - "127.0.0.1:8080:8080"
      - "127.0.0.1:9090:9090"
    environment:
      - RUST_LOG=info
      - JWT_SECRET=${JWT_SECRET}
    volumes:
      - ../../clara-cerebrum/models:/etc/clara/models:ro
    restart: unless-stopped
    networks:
      - clara-net

  prolog-mcp:
    build:
      context: ../..
      dockerfile: clara-cerebrum/docker/Dockerfile
      target: prolog-mcp
    ports:
      - "127.0.0.1:1968:1968"
    environment:
      - REST_API_URL=http://clara-api:8080
    depends_on:
      - clara-api
    restart: unless-stopped
    networks:
      - clara-net

  clips-mcp:
    build:
      context: ../..
      dockerfile: clara-cerebrum/docker/Dockerfile
      target: clips-mcp
    ports:
      - "127.0.0.1:1951:1951"
    environment:
      - REST_API_URL=http://clara-api:8080
    depends_on:
      - clara-api
    restart: unless-stopped
    networks:
      - clara-net

  clara-frontdesk:
    build:
      context: ../..
      dockerfile: clara-cerebrum/docker/Dockerfile
      target: frontdesk
    ports:
      - "127.0.0.1:8088:8088"
    environment:
      - RUST_LOG=info
    depends_on:
      - clara-api
      - lildaemon
    restart: unless-stopped
    networks:
      - clara-net

  lildaemon:
    build:
      context: ../..
      dockerfile: clara-cerebrum/docker/Dockerfile.lildaemon
    ports:
      - "127.0.0.1:6666:6666"
    environment:
      - CEREBELLUM_URL=http://clara-api:8080
      - GROQ_API_KEY=${GROQ_API_KEY}
    restart: unless-stopped
    networks:
      - clara-net

networks:
  clara-net:
    driver: bridge
```

All ports bound to `127.0.0.1` — not exposed on the public interface. Services
communicate via `clara-net` bridge; only nginx touches them.

**`.env` file** (on EC2 at `/opt/clara/.env`, not committed to git):
```
JWT_SECRET=<secret>
GROQ_API_KEY=<groq-key>
```

---

## Step 7: EdgeQuake

EdgeQuake already has its own Docker Compose with Groq support built in.

Deploy separately from the Clara stack:

```bash
cd /opt/clara/edgequake
docker compose -f docker/docker-compose.yml --env-file /opt/clara/edgequake.env up -d
```

**`/opt/clara/edgequake.env`** (on EC2, not committed):
```
EDGEQUAKE_PORT=8082
FRONTEND_PORT=3000
EDGEQUAKE_DEFAULT_LLM_PROVIDER=groq
GROQ_API_KEY=<groq-key>
```

Ports are also bound to `127.0.0.1` via the edgequake compose (or override in .env).

---

## Step 8: Nginx Configuration

Extend the existing seashell nginx config. Add a new include file:
**`seashell/deploy/nginx/clara.conf`**

```nginx
# ── Public: Frontdesk PoC demo ───────────────────────────────────────────────
location /frontdesk/ {
    proxy_pass         http://127.0.0.1:8088/;
    proxy_http_version 1.1;
    proxy_set_header   Upgrade $http_upgrade;
    proxy_set_header   Connection "upgrade";
    proxy_set_header   Host $host;
    proxy_set_header   X-Real-IP $remote_addr;
}

# ── Auth-protected: EdgeQuake ─────────────────────────────────────────────────
location /edgequake/ {
    auth_basic           "Clara Demo";
    auth_basic_user_file /etc/nginx/.htpasswd;
    proxy_pass           http://127.0.0.1:3000/;
    proxy_set_header     Host $host;
    proxy_set_header     X-Real-IP $remote_addr;
}

location /edgequake/api/ {
    auth_basic           "Clara Demo";
    auth_basic_user_file /etc/nginx/.htpasswd;
    proxy_pass           http://127.0.0.1:8082/;
}
```

Include in main nginx server block:
```nginx
include /etc/nginx/conf.d/clara.conf;
```

Create the htpasswd file on EC2:
```bash
sudo yum install -y httpd-tools
sudo htpasswd -c /etc/nginx/.htpasswd demo
```

All other internal services (clara-api, FieryPit, MCP adapters) have no nginx
proxy — only accessible from localhost or within the Docker network.

---

## Step 9: Deployment Script

Add a `scripts/deploy-clara.sh` to clara-cerebrum (alongside the seashell deploy
scripts, following the same SSH pattern):

```bash
#!/bin/bash
set -e

EC2_HOST="ec2-54-176-157-222.us-west-1.compute.amazonaws.com"
EC2_USER="ec2-user"
SSH_KEY="$HOME/vastness/.ssh/SeashellAnalytics_220325.pem"
REMOTE_DIR="/opt/clara"
PARENT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"  # Development/

echo "=== Clara PoC Deployment ==="

# Sync repos
rsync -avz --exclude='target/' --exclude='.git/' \
    -e "ssh -i $SSH_KEY" \
    "$PARENT_DIR/clara-cerebrum/" \
    "$EC2_USER@$EC2_HOST:$REMOTE_DIR/clara-cerebrum/"

rsync -avz --exclude='__pycache__/' --exclude='.git/' \
    -e "ssh -i $SSH_KEY" \
    "$PARENT_DIR/lildaemon/" \
    "$EC2_USER@$EC2_HOST:$REMOTE_DIR/lildaemon/"

rsync -avz --exclude='.git/' \
    -e "ssh -i $SSH_KEY" \
    "$PARENT_DIR/edgequake/" \
    "$EC2_USER@$EC2_HOST:$REMOTE_DIR/edgequake/"

# Sync models (large files — only if changed)
rsync -avz --checksum \
    -e "ssh -i $SSH_KEY" \
    "$PARENT_DIR/clara-cerebrum/models/" \
    "$EC2_USER@$EC2_HOST:$REMOTE_DIR/clara-cerebrum/models/"

# Build and restart Clara stack
ssh -i "$SSH_KEY" "$EC2_USER@$EC2_HOST" << 'ENDSSH'
    set -e
    cd /opt/clara
    docker compose -f clara-cerebrum/docker/docker-compose.yml \
        --env-file /opt/clara/.env \
        build --no-cache
    docker compose -f clara-cerebrum/docker/docker-compose.yml \
        --env-file /opt/clara/.env \
        up -d
    echo "Clara stack deployed."
ENDSSH
```

**Demo start/stop** (run on EC2 after SSH-ing in):

```bash
# start
cd /opt/clara && docker compose -f clara-cerebrum/docker/docker-compose.yml --env-file .env up -d
cd /opt/clara/edgequake && docker compose -f docker/docker-compose.yml --env-file /opt/clara/edgequake.env up -d

# stop
cd /opt/clara && docker compose -f clara-cerebrum/docker/docker-compose.yml down
cd /opt/clara/edgequake && docker compose -f docker/docker-compose.yml down
```

---

## Files to Create / Modify

| File | Action |
|------|--------|
| `clara-cerebrum/docker/Dockerfile` | Create (replaces root Dockerfile; multi-stage, all Rust targets) |
| `clara-cerebrum/docker/Dockerfile.lildaemon` | Create |
| `clara-cerebrum/docker/docker-compose.yml` | Create |
| `clara-cerebrum/clara-frontdesk-poc/config/city_of_dis.toml` | Update paths + service URLs |
| `lildaemon/config/evaluators.yaml` | Update cerebellum_url |
| `seashell/deploy/nginx/clara.conf` | Create (nginx proxy + auth) |
| `clara-cerebrum/scripts/deploy-clara.sh` | Create |

Old `clara-cerebrum/Dockerfile` (root level) can be deleted or kept for local dev use.

---

## Outstanding Build Risk

The `clara-prolog` crate compiles SWI-Prolog from source via CMake (`swipl-src/`).
Required build deps in the Docker builder:
- `cmake`, `ninja-build`, `build-essential`
- `libgmp-dev`, `zlib1g-dev`, `libpcre2-dev`, `libssl-dev`
- Possibly `libarchive-dev`, `libjpeg-dev` depending on SWI-Prolog feature flags

Check `clara-prolog/build.rs` to confirm exact CMake flags used, and adjust the
builder `apt-get install` line accordingly before the first EC2 build.

---

## Verification

```bash
# From local machine — these should NOT be reachable (firewall check):
curl http://ec2-host:8080/health   # should timeout
curl http://ec2-host:6666/health   # should timeout

# These should work (nginx proxy):
# Public — no auth:
curl https://seashell-host/frontdesk/

# Protected — prompt for credentials:
curl -u demo:PASSWORD https://seashell-host/edgequake/
curl -u demo:PASSWORD https://seashell-host/edgequake/api/health
```
