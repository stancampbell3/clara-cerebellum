# DNS & EdgeQuake Setup

## 1. Fix Route 53 `www` subdomain

In the AWS Console:

1. Go to **Route 53 → Hosted zones → seashellanalytics.com**
2. Note what the bare `seashellanalytics.com` record points to (A record or Alias)
3. Create a new record:
   - **Record name**: `www`
   - **Type**: `CNAME`
   - **Value**: `seashellanalytics.com`
   - **TTL**: 300
4. Save and wait for propagation (~5 minutes)

## 2. Update nginx `server_name`

Check the current `server_name` in `/etc/nginx/nginx.conf` (or wherever the seashell server block lives) and add `www.seashellanalytics.com`:

```nginx
server_name seashellanalytics.com www.seashellanalytics.com;
```

Then reload nginx:

```bash
sudo nginx -t && sudo systemctl reload nginx
```

## 3. Create EdgeQuake docker-compose override

Create `/opt/clara/edgequake/edgequake/docker/docker-compose.override.yml`:

```yaml
services:
  frontend:
    build:
      args:
        NEXT_PUBLIC_API_URL: http://seashellanalytics.com/edgequake/api
    ports:
      - "127.0.0.1:3000:3000"
  edgequake:
    ports:
      - "127.0.0.1:8082:8080"
  postgres:
    ports:
      - "127.0.0.1:5432:5432"
```

This does two things:
- Sets the correct public API URL baked into the Next.js bundle
- Locks all ports to localhost-only (keeps postgres etc. off the public internet)

## 4. Build and start EdgeQuake

```bash
cd /opt/clara/edgequake/edgequake
docker compose -f docker/docker-compose.yml --env-file /opt/clara/edgequake.env up -d --build
```

Docker Compose automatically merges `docker-compose.override.yml` — no extra flags needed.

## 5. Verify

```bash
# Check all containers healthy
docker compose -f docker/docker-compose.yml ps

# Test backend directly
curl http://localhost:8082/health

# Test frontend
curl -s http://localhost:3000 | head -5
```

Then test via nginx (will prompt for basic auth credentials):

```
http://seashellanalytics.com/edgequake/
```
