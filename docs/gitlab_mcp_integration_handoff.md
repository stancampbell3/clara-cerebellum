# Context handoff: GitLab MCP integration (Clara stack)

> Written on Pineal 2026-10-27, handing off to a session on limbic (GitLab,
> Edgequake, and the rest of the Clara containers live there — less SSH
> back-and-forth doing this work from there directly).

## Where things stand

We've been working through the Pineal/Clara hermes stack this session:

1. **hermes-pineal** (standalone Hermes container on Pineal, `container_name: hermes`, image `nousresearch/hermes-agent:v0.21.3`) was rebuilt to mount `~/widebody/Development` read/write at `/opt/data/workspace/Development`. Deploy artifact: `~/widebody/Development/hermes-pineal/docker-compose.yml` (its own git repo, committed), secrets in sibling git-ignored `.env` (dashboard basic auth).
2. That rebuild broke two things, both now fixed and verified durable (survived a full `docker compose restart`):
   - **Ollama** (`qwen-clara-9b` via `http://host.docker.internal:11434/v1`) — needed `extra_hosts: ["host.docker.internal:host-gateway"]`, missing from the recreate.
   - **Edgequake MCP** (`mcp_servers.edgequake` → `http://limbic:8082/mcp` in `~/.hermes/config.yaml` on Pineal) — same class of bug (container lost DNS/reachability to `limbic`). Now confirmed live: `hermes mcp test edgequake` → ✓ connected, 3 tools (`edgequake_search`, `edgequake_fetch`, `edgequake_retrieve`).
   - Also hit and fixed a **separate gotcha**: snap-confined `docker compose` on Pineal expands `~` against its own sandboxed `$HOME` (`/home/stanc/snap/docker/<rev>/...`), not the real one — silently bind-mounts empty decoy directories. Fixed by using absolute paths in `docker-compose.yml`. Worth knowing if any Pineal-side compose file gets touched again.
3. Memory updated (Claude's own memory, on Pineal's Claude Code install): `edgequake_integration_status.md` (root cause + fix + still-open item: Edgequake's LLM-backed query path may 502 with `model is required` if Edgequake has no default model configured server-side — unconfirmed, check on first real query) and `hermes_pineal_secrets_todo.md` (accepted tradeoff: `.env` with dashboard creds sits inside the dir mounted into the agent's own workspace — fine for this single-user dev box, revisit if threat model changes).

## The new task: GitLab MCP

Goal: enable a GitLab MCP connection from **limbic's** self-hosted GitLab instance, wired into **both**:
- hermes-pineal (standalone, on Pineal, config at `~/.hermes/config.yaml` on Pineal)
- the **Clara-launched hermes-agent** — launched via a launcher on Pineal, described as a *different* instance from hermes-pineal. **Its config location is unresolved** — searched Pineal for a second `~/.hermes*` home, a Hermes profile, and launcher scripts referencing hermes in `clara-cerebellum`; found nothing. Need to find or ask where this one's `HERMES_HOME`/`config.yaml` actually lives before it can be wired up too.

### What's confirmed on limbic (via SSH, `docker ps`)

GitLab is real and running:
```
gitlab           gitlab/gitlab-ce:latest      0.0.0.0:8929->8929/tcp (http), 0.0.0.0:2224->22/tcp (ssh), 0.0.0.0:8930->443/tcp (https)
gitlab-runner     gitlab/gitlab-runner:latest
```
Verified live at `http://limbic:8929` (nginx, `X-Gitlab-Meta` header, redirects to `/users/sign_in`). **Community Edition** — no native MCP/Duo Agent Platform support (that's Enterprise/Ultimate tier), so there is **no existing GitLab MCP server** to just point at. One needs to be deployed.

This repo's own `origin` remote is actually this same GitLab instance: `http://limbic:8929/root/clara-cerebellum.git` (the `/mnt/moonpool` checkout) / `git@gitlab-limbic:root/clara-cerebellum.git` (the `~/widebody` checkout, SSH alias). Both are the same repo, same HEAD, just different checkouts/remote protocols.

Other containers on limbic for context (full `docker ps` at time of writing):
```
docker-cobbler-1          cobbler:latest                              0.0.0.0:5001->5001/tcp
docker-clara-frontdesk-1  clara-frontdesk:latest                      0.0.0.0:8088->8088/tcp
docker-lildaemon-1        lildaemon:latest                            0.0.0.0:6666->6666/tcp
docker-prolog-mcp-1       clara-prolog-mcp:latest                     0.0.0.0:1968->1968/tcp
docker-clips-mcp-1        clara-clips-mcp:latest                      0.0.0.0:1951->1951/tcp
docker-clara-api-1        clara-api:latest                            0.0.0.0:8080->8080/tcp, 9090/tcp
docker-redpanda-console-1 docker.redpanda.com/redpandadata/console    0.0.0.0:8083->8080/tcp   # Kafka UI — false lead, NOT an MCP server (its /mcp path is just SPA catch-all routing)
docker-kafka-1            apache/kafka:latest                         9092/tcp, 0.0.0.0:9094->9094/tcp
node-exporter             prom/node-exporter:latest
edgequake-api             edgequake-clara:g1                          8080/tcp, 0.0.0.0:8082->8082/tcp
edgequake-frontend        ghcr.io/raphaelmansuy/edgequake-frontend     3000/tcp, 0.0.0.0:3421->3421/tcp
edgequake-postgres        ghcr.io/raphaelmansuy/edgequake-postgres     5432/tcp
gitlab                    gitlab/gitlab-ce:latest                     0.0.0.0:8929/2224/8930
gitlab-runner             gitlab/gitlab-runner:latest
```

### The established pattern for adding an MCP server here

`prolog-mcp` and `clips-mcp` are the template — first-party, built from `clara-cerebellum/docker/Dockerfile` (multi-stage targets `prolog-mcp`/`clips-mcp`), defined in `clara-cerebellum/docker/docker-compose.yml` (also a `docker-compose.prod.yml` variant), on the `clara-net` Docker network, talking to `clara-api` over its internal hostname:

```yaml
prolog-mcp:
  build:
    context: ../..
    dockerfile: clara-cerebellum/docker/Dockerfile
    target: prolog-mcp
  image: clara-prolog-mcp:latest
  ports:
    - "0.0.0.0:1968:1968"
  environment:
    - REST_API_URL=http://clara-api:8080
  depends_on:
    clara-api:
      condition: service_healthy
  healthcheck:
    test: ["CMD", "bash", "-c", "echo > /dev/tcp/localhost/1968"]
    interval: 5s
    timeout: 3s
    retries: 20
    start_period: 10s
  restart: unless-stopped
  networks:
    - clara-net
```

`edgequake-api` is a separate, third-party-ish service (`edgequake-clara:g1`) exposing an HTTP MCP endpoint at `/mcp` on `:8082` — this is the shape the *client side* config expects (see below), regardless of how the server itself gets built.

### Decision still open (left for the limbic session to pick up)

Two choices, not yet made:
1. **Implementation**: deploy a known open-source GitLab MCP server (survey current, actually-maintained options — don't assume any specific package name is still accurate) as a new sidecar container next to `edgequake-api`/`prolog-mcp`, talking to GitLab's REST/GraphQL API via a **Personal Access Token** — vs. writing a minimal first-party wrapper in the `prolog-mcp`/`clips-mcp` style. Leaning toward the open-source sidecar for speed/maintenance, but not decided.
2. **GitLab Personal Access Token**: needs to be generated by the user (through the GitLab UI at `http://limbic:8929`) — not something to auto-generate. Scope needed depends on which MCP server implementation is chosen (typically `api` or `read_api` + repo-level scopes).

### Client-side config shape to replicate (from hermes-pineal's `~/.hermes/config.yaml` on Pineal)

This is what got edgequake wired into hermes-pineal — the new `gitlab` entry should look like this once the server exists, adjusted for whatever auth scheme the chosen MCP server actually wants:

```yaml
mcp_servers:
  edgequake:
    url: http://limbic:8082/mcp
    headers:
      X-Tenant-ID: "00000000-0000-0000-0000-000000000002"
      X-API-Key: "${EDGEQUAKE_API_KEY}"
      X-Workspace-ID: "e2fd2658-c252-4eed-94ad-40051fea2209"
    connect_timeout: 30
    timeout: 120
  gitlab:
    url: http://limbic:<PORT>/mcp   # TBD once deployed
    headers:
      # TBD — depends on chosen server's auth scheme (likely an Authorization/PAT header)
    connect_timeout: 30
    timeout: 120
```

`${VAR}` interpolation pulls from the owning profile's `.env` — same convention to follow for a `GITLAB_API_TOKEN` or similar.

## Immediate next steps for the limbic session

1. Survey current, actually-maintained GitLab MCP server options (don't trust any specific package name without checking it's current) — decide sidecar vs. custom wrapper.
2. Have the user generate a GitLab PAT via the `limbic:8929` UI with the scope the chosen server needs.
3. Add the new service to `clara-cerebellum/docker/docker-compose.yml` (and `.prod.yml` if relevant) following the `prolog-mcp`/`clips-mcp`/`edgequake-api` pattern — pick a free port, wire `GITLAB_URL`/token env var, add healthcheck, put it on `clara-net`.
4. Verify the new MCP server is reachable (`curl`/`hermes mcp test`-style check) before touching any client config.
5. Add `mcp_servers.gitlab` to hermes-pineal's `~/.hermes/config.yaml` **on Pineal** (not limbic) and verify with `docker exec hermes hermes mcp test gitlab`.
6. **Still need to locate the Clara-launched hermes-agent's config** before it can get the same entry — ask the user directly where/how that instance is launched (launcher script? separate profile? different host?) rather than guessing.

## Pineal-side loose ends (unrelated to this doc, FYI only)

A `git stash -u` was run in the `~/widebody/Development/clara-cerebellum` checkout on Pineal immediately before adding this doc, to keep the working tree clean for the commit/push. That stash (`"wip: pre-gitlab-mcp-doc stash ..."`) holds a batch of untracked scratch/test files (`test10.pl`, `test11_clara.*`, `test12*`, `demo/`, `docker/.env.example`, etc.) — not part of this work, just needed to be out of the way. Two older, unrelated stashes already existed on that checkout before this one; all three are still there, untouched.
