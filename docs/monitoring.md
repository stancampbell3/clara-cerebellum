# Monitoring: Prometheus + Grafana for the Clara stack

*Status: Phases 1-6 built and live-verified 2026-10-01 — Prometheus/Grafana (Phases 1-5) and the clara-frontdesk-poc live topology view (Phase 6).*

## Why

Debugging Rituals from clara-frontdesk-poc gave almost no visibility into what was actually
happening: ritual lifecycle, deduction/research-task state, which tool got called where, and
which FieryPit evaluators were alive and doing what. This adds a standard Prometheus exposition
layer (counters/gauges/histograms behind `/metrics` on `clara-api` and every FieryPit) feeding
Grafana dashboards on pineal.

## What's where

- **`clara-metrics`** (Rust crate): typed counter/gauge/histogram functions (`clara_metrics::counters::*`,
  `gauges::*`, `histograms::*`) wrapping the `metrics` facade (renamed `metrics_core` in Cargo.toml
  to avoid colliding with this crate's own `metrics` module). `clara-ritual`, `clara-toolbox`, and
  `clara-api` depend on it and call it at existing chokepoints (`RitualRegistry::{create,join,
  terminate,reap_topics}`, `ToolboxManager::execute_tool`). `clara-api`'s `GET /metrics` renders the
  installed `PrometheusHandle` — same port (8080) as the rest of the API, not a second listener.
- **`goat/app/metrics.py`** (Python, lildaemon): the same pattern over `prometheus_client`, mounted
  at `GET /metrics` on every FieryPit (port 6666 inside the container). Wired into
  `EvaluationRegistry`, `HungDetector`, `RitualParticipant` (a new minimal `ParticipantState` —
  starting/idle/busy/error/stopped — the one genuinely new instrumentation point; everything else
  re-emits from existing logging/registries), `toolified_ollama._dispatch_tool`,
  `agent_evaluator._supervise` (Hermes' own tool events), `ego_gate/tools.py`, and the assistant's
  `research_queue.py`.
- **`FIERYPIT_INSTANCE`** env var on each FieryPit deployment (`limbic-main`, `limbic-ego`,
  `pineal-remote`, `pineal-ego`) labels which process emitted a given `fierypit_*` metric — set in
  each `docker-compose*.yml`.
- **Prometheus + Grafana + node_exporter**: on **pineal**, host-networked
  (`docker-compose.monitoring.yml`, run from pineal's own checkout at
  `~/widebody/Development/clara-cerebellum/docker` — NOT the shared NFS mount, which is limbic's
  live working tree). node_exporter also runs on **limbic** (`docker-compose.yml`'s `node-exporter`
  service) so Prometheus can scrape both hosts' CPU/mem/disk/network.

Host networking everywhere in the monitoring stack (and on the `lildaemon` remote-fierypit
service it mirrors) sidesteps a confirmed gotcha: Docker's own `DOCKER-USER` iptables chain can
silently drop bridge-to-host traffic ahead of ufw's rules.

## Deploying / updating

```bash
# Rust + Python instrumentation: rebuild and recreate the affected containers, from limbic
cd clara-cerebellum/docker
docker compose build clara-api lildaemon
docker compose up -d --no-deps clara-api lildaemon

# node_exporter on limbic (part of the main compose file)
docker compose up -d node-exporter

# Prometheus + Grafana + node_exporter on pineal (separate checkout — copy changed files there
# first; this directory is not git-pulled automatically, see "Known gaps" below)
ssh pineal
cd ~/widebody/Development/clara-cerebellum/docker
docker compose -f docker-compose.monitoring.yml up -d
```

Grafana: `http://pineal:3000`, default `admin`/`admin` — **change the password on first login**.
Dashboards auto-provision from `grafana-provisioning/dashboards/json/*.json` into a "Clara" folder;
edit the JSON and re-run `docker compose -f docker-compose.monitoring.yml up -d grafana` (or wait
for the 30s provisioning poll) to pick up changes. The Prometheus datasource is provisioned with a
fixed `uid: prometheus` — if you ever see Grafana fail to start with `Datasource provisioning
error: data source not found` after editing `datasources.yml`, it means the persisted
`grafana-data` volume still has an old datasource row with a different UID; safe to
`docker volume rm monitoring_grafana-data` and let it reprovision from scratch (there's no manually
-created content in it yet as of this writing).

## Firewall

New inbound rules, limbic (pineal's IP, confirm it hasn't changed): `6666`, `6667`, `9100` —
plus a Grafana-UI rule on pineal (`3000`) for any source besides pineal itself, if remote access to
Grafana is wanted. All ufw changes need interactive sudo — not automatable from this session's
access level. Verify with `curl http://pineal:9090/api/v1/targets` (or the Grafana UI's Prometheus
data source test) after changing anything.

## Dashboards

Four dashboards, "Clara" folder: **Ritual Overview**, **Deduction / Research-task State**,
**Tool Usage**, **Evaluator Liveness**. A fifth, generic host-health dashboard is deliberately not
hand-built here — import the community node_exporter dashboard (grafana.com ID 1860) once, per
Grafana instance, via the UI.

## Live topology view (Phase 6)

`clara-frontdesk-poc` has a toggleable live graph (header button, top right) of active Rituals and
their evaluators — a Cytoscape.js panel fed by a `"topology"` WS frame pushed every 5s, built from:

- Dis's `GET /ritual` — active Ritual nodes.
- lildaemon's `GET /ritual/participants` (new) — this FieryPit's own live `RitualParticipant`s,
  joined to their Ritual purely on `ritual_id` (the one identifier both APIs agree on; Dis's
  `participant_key` and lildaemon's `node_id` are different identity spaces, not cross-referenced).

Evaluator nodes reuse Cobbler's `lild_*.svg` icons (vendored into `static/vendor/icons/`) and its
`ICON_ALIASES`/fallback-on-404 technique, ported to vanilla JS (`static/vendor/topology.js` —
Cytoscape itself needs no build step either, vendored the same way as `marked`/`highlight`/
`purify`/`viz`). Complements, doesn't replace, the Evaluator Liveness dashboard: same
`ParticipantState` vocabulary (`starting`/`idle`/`busy`/`error`/`stopped`), structural (who/where,
right now) view instead of time-series.

Only this one FieryPit's evaluators are shown — a multi-FieryPit topology would need either a
config list of known FieryPit URLs or discovery via Dis's `GET /fierypits` registry, neither of
which this first cut does.

## Known gaps / deferred

- **Per-call ritual/caller attribution** on `clara-toolbox`'s `ToolRequest` (would let tool metrics
  be ritual-scoped, not just domain-wide) — `clara_toolbox::set_domain_id` is a single process-wide
  global today. Deferred; revisit if ritual-scoped tool breakdowns turn out to matter.
- **pineal's monitoring checkout is not git-tracked against this repo** — it's a separate clone at
  `~/widebody/Development/clara-cerebellum`, and the files there were deployed by direct copy
  during this rollout, not a `git pull`. Keep them in sync by hand (or script it) until that
  checkout is git-connected the same way limbic's is.
- **`seat_launcher` on pineal** (Hermes seat manager, Unix-socket-only) has no `/metrics` endpoint
  yet — would need a small new loopback-bound TCP listener to be scrapable.
- **The topology view only shows one FieryPit** (`fiery_pit_url` from frontdesk-poc's own config) —
  see above.
- Per-state coloring on the Evaluator Liveness dashboard's state-timeline panel is a first cut
  (rows distinguished by label, not yet by per-state color) — a Grafana field-override follow-up,
  not a data gap.
