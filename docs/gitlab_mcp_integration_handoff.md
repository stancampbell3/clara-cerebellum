# GitLab MCP integration (Clara stack)

> Started on Pineal 2026-10-27, finished on limbic 2026-10-28. This replaces the original
> handoff, which assumed GitLab CE had no MCP route and left the Clara-Hermes config unresolved.

## Status: DONE for standalone hermes-pineal; Clara-managed Hermes deliberately excluded

- A `gitlab-mcp` sidecar runs on limbic (`clara-net`, published on `0.0.0.0:8767`, `127.0.0.1` in prod).
- hermes-pineal's `~/.hermes/config.yaml` has `mcp_servers.gitlab`; `docker exec hermes hermes mcp test gitlab`
  connects and lists 40 tools.
- Compose change committed in clara-cerebellum `a32c926` (not pushed at time of writing).

## Why a sidecar (findings)

- limbic runs GitLab CE 19.2.1 (`gitlab/gitlab-ce`, `http://limbic:8929`). It **does** mount a native MCP
  endpoint at `/api/v4/mcp` (unauthenticated calls get `401` with a `Bearer` challenge).
- It is not usable here: classic PATs get `403` on it regardless of scope or owner (`api`, `read_api`, root or
  not), and the fine-grained token form has no MCP permission on CE (Enterprise only).
- So the community server [zereight/gitlab-mcp](https://github.com/zereight/gitlab-mcp) runs as a sidecar,
  image `zereight050/gitlab-mcp` **pinned by digest** in the compose files. It is third-party code and its
  source was not reviewed; bump the digest deliberately.

## How it is configured

Service `gitlab-mcp` in `docker/docker-compose.yml` and `docker/docker-compose.prod.yml`:

- Streamable HTTP, endpoint `/mcp`, container port 3002, host port **8767**.
- `REMOTE_AUTHORIZATION=true`: the container holds **no GitLab token**; every client sends its own PAT as
  `Authorization: Bearer glpat-...`. Without a token the server returns `401`.
- `GITLAB_API_URL=http://limbic:8929/api/v4`, with `extra_hosts: limbic:host-gateway` so the container
  reaches GitLab on the host.
- `GITLAB_PERMISSION_MODE=modify`: issues, MRs, files, commits, projects; no delete/teardown tools.
- `MCP_ALLOWED_HOSTS=limbic,limbic:8767,localhost,localhost:8767,127.0.0.1:8767`. Required: the server's
  DNS-rebinding guard rejects any other `Host` header with `403 Host header is not allowed` (this is what
  broke the first cross-host test from pineal). A client reaching it by another name or IP needs adding here.
- Healthcheck is a node TCP connect on 3002 (the image is node-based, no bash `/dev/tcp`).

## GitLab-side setup (done by hand in the UI)

- User `hermespineal` (non-admin), member of the **Clara Development** group, which is added to the Clara
  projects (`seashell`, `ochecho`, `dagda`, `lildaemon`, `clara-cerebellum`). Hermes only sees those.
- Its classic PAT (`read_api`, `api`; expires 2027-01-27) is held in `hermes_gitlab_pat_token.txt` in the
  Development directory (env-style `NAME=glpat-...` line), and as `GITLAB_API_TOKEN` in pineal's
  `~/.hermes/.env`. Note the file sits on the shared NFS mount.
- ufw on limbic allows `8767/tcp` (added by hand).

## Client config (hermes-pineal, on Pineal)

```yaml
mcp_servers:
  gitlab:
    url: http://limbic:8767/mcp
    headers:
      Authorization: "Bearer ${GITLAB_API_TOKEN}"
    connect_timeout: 30
    timeout: 120
```

`${GITLAB_API_TOKEN}` resolves from `~/.hermes/.env`. Backups: `config.yaml.bak-gitlabmcp`,
`.env.bak-gitlabmcp` next to them. Verify with `docker exec hermes hermes mcp test gitlab`.

## Clara-managed Hermes: not wired, on purpose (decision 2026-10-28)

There is no separate Clara-Hermes home to edit. Seats are rendered per run from the golden template
`lildaemon/seat_launcher/template/config.yaml`, and the generated seat `.env` carries only `EGO_GATE_URL` and
`EGO_GATE_TOKEN` (`seats.py`). The template's `mcp_servers` is `ego_gate` alone, and `seats.py` refuses a seat
unless Hermes registered exactly the six `mcp__ego_gate__*` tools. Adding GitLab there would let seats make
`modify`-level changes outside the Superego gate and the action ledger, with a token in the seat's `.env`.

Decision: leave Clara-managed Hermes without GitLab for now. If it is wanted later, the real design is to make
GitLab a **gated action class** behind `request_action` (the PAT stays with the executor, not the seat). A
launcher-level extra-MCP allowlist is a possible shortcut but is an explicit exception to the gate.

## Loose ends

- Compose commit `a32c926` is not pushed; a pipeline here costs ~13 min on one runner (consider `-o ci.skip`).
- `hermes_gitlab_pat_token.txt` is mode `664` on the shared mount; `chmod 600` suggested.
- The earlier token `hermes_agent_legacy` (id 43) is a root-owned `api` token; revoke if nothing uses it.
- No real read through Hermes has been exercised yet, only connect + tool list.
- Pineal's scratch stash in `~/widebody/Development/clara-cerebellum` (three stashes, from before this work)
  is untouched and unrelated.
