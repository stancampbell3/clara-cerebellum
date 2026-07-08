# Plan: `baloroptik` — Deduction Trace Visualization CLI

## Context

The clara reasoning cycle (Prolog ↔ CLIPS via Coire) now records per-phase Dagda tableau snapshots during `trace: true` deduction runs. The `clara-api` can produce colorized DOT graphs for each phase via its trace HTTP endpoints, and we can already generate static DOT graphs offline using `clara-cycle`'s `parse_prolog_rules` / `generate_dot` / `coloring_from_entries` pipeline.

`baloroptik` ("eye of Balor") is a CLI tool that consumes persisted deduction state — either from a local snapshot JSON file or from a live `clara-api` instance — and emits a sequenced set of colored DOT graphs (and optionally SVG) representing the reasoning trace. Named after Balor's deadly eye that sees all, fitting the project's infernal/medieval theme alongside `transduction`.

---

## New Crate: `clara-baloroptik`

New workspace member at `clara-baloroptik/`. Binary name: `baloroptik`.

Add to workspace `Cargo.toml` `members` list alongside `clara-transduction`.

### `clara-baloroptik/Cargo.toml`

```toml
[package]
name = "clara-baloroptik"
version = "0.1.0"
edition.workspace = true
license.workspace = true
description = "Deduction trace visualization CLI — the eye of Balor"
publish.workspace = true

[[bin]]
name = "baloroptik"
path = "src/main.rs"

[dependencies]
clara-cycle   = { path = "../clara-cycle" }
serde         = { version = "1", features = ["derive"] }
serde_json    = "1"
uuid          = { version = "1", features = ["serde"] }
reqwest       = { version = "0.11", features = ["blocking", "json"] }
clap          = { version = "4", features = ["derive"] }
```

---

## CLI Design

```
baloroptik <COMMAND>

COMMANDS:
  file    Generate a DOT graph from a local deduction snapshot JSON file (offline)
  trace   Fetch and render the full reasoning trace from a running clara-api (online)

OPTIONS (per subcommand):
  --out-dir <DIR>       Directory for output files  [default: .]
  --format <FORMAT>     dot | svg | both            [default: dot]
  --link-shared         Add shared-condition edges to DOT graph
```

### `baloroptik file <SNAPSHOT>`

Offline. Reads a persisted deduction snapshot JSON (the format stored by `clara-coire`'s snapshot store — same as `deduction_snapshots.json`). The snapshot contains the final Dagda tableau; produces **one** colorized DOT of the final reasoning state.

Data path:
1. Deserialize snapshot JSON; `prolog_clauses` is a **double-encoded** JSON string — deserialize with `serde_json::from_str::<Vec<String>>(&snap.prolog_clauses)`.
2. Join clauses with `\n` → `parse_prolog_rules` (from `clara-cycle`).
3. `tableau_entries` is also double-encoded → `serde_json::from_str::<Vec<PredicateEntry>>(&snap.tableau_entries)`.
4. `coloring_from_entries(&entries)` → `generate_dot(&rules, Some(&coloring), &opts)`.
5. Write `{stem}_final.dot` (stem = first 8 chars of deduction_id).

### `baloroptik trace <DEDUCTION_ID>`

Online. Calls a running `clara-api` to download one DOT per recorded trace phase.

Additional option:
- `--api <URL>`  API base URL  [default: `http://localhost:8080`]

Data path:
1. `GET {api}/deduce/{id}/trace` → ordered list of `{ change_id, cycle_num, phase, recorded_at_ms }`.
2. For each phase in order: `GET {api}/deduce/{id}/trace/{change_id}/dot` → raw DOT string (already colorized by the API — no local re-parsing needed).
3. Write `{stem}_{i:03}_{phase_slug}.dot` per phase; sequential index prevents collisions when the same cycle has multiple phases. `phase_slug` replaces `/` with `_`.

---

## Source Layout

```
clara-baloroptik/
  Cargo.toml
  src/
    main.rs       — clap CLI structs (Commands, Args), dispatch to subcommands
    file_mode.rs  — offline snapshot → single DOT
    trace_mode.rs — API fetch → sequence of DOT files
    render.rs     — write_dot(), write_svg(), print_summary()
```

### `render.rs` — SVG rendering

```rust
fn write_svg(dot_src: &str, path: &Path) -> Result<(), String>
```

Shells out to `dot -Tsvg` via `std::process::Command` with piped stdin/stdout. If the `dot` binary is not found in PATH, prints a warning to stderr and skips SVG (`.dot` is always written regardless).

---

## Stdout Summary

`file` mode:
```
Deduction: 7cf5e9cf-1052-4435-b8fa-0c7c7e6cd371
Status:    converged (3 cycles)
Goal:      omelette(bob, X).
Tableau:   5 entries  (T:3 F:1 U:0 ?:1)

Wrote: ./7cf5e9cf_final.dot
```

`trace` mode:
```
Deduction: 550e8400-e29b-41d4-a716-446655440000
Status:    converged (3 cycles)

  #    Cycle  Phase                   File
  ───  ─────  ──────────────────────  ────────────────────────────────────
   0      0   initial                 550e8400_000_initial.dot
   1      0   prolog_to_clips         550e8400_001_prolog_to_clips.dot
   2      0   clips_to_prolog         550e8400_002_clips_to_prolog.dot
   3      1   prolog_to_clips         550e8400_003_prolog_to_clips.dot
   4      2   final_converged         550e8400_004_final_converged.dot
```

---

## Key Reused APIs (from `clara-cycle`)

| Function / Type | File |
|---|---|
| `parse_prolog_rules(src: &str) -> Vec<PrologRule>` | `clara-cycle/src/transduction.rs` |
| `generate_dot(rules, coloring, opts) -> String` | `clara-cycle/src/transduction.rs` |
| `coloring_from_entries(entries: &[PredicateEntry]) -> NodeColoring` | `clara-cycle/src/transduction.rs` |
| `DotOptions { link_shared_conditions: bool }` | `clara-cycle/src/transduction.rs` |
| `PredicateEntry` (re-exported from `clara-dagda`) | `clara-cycle/src/lib.rs` |

`reqwest` blocking: follows the pattern in `fiery-pit-client` (version 0.11, same workspace).
`clap` v4 with derive feature is new to the workspace (not used elsewhere today).

---

## Truth-Value Color Reference

| Fill color | Truth value | Meaning |
|---|---|---|
| `#28a745` green | `KnownTrue` | Proved |
| `#dc3545` red | `KnownFalse` | Disproved |
| `#ffc107` amber | `KnownUnresolved` | Mixed / conflicting entries for same functor |
| `#adb5bd` gray | `Unknown` | Not yet evaluated |
| Structural default | (absent from tableau) | No entry recorded |

---

## Workspace Change

`Cargo.toml` — add `"clara-baloroptik"` to `members`:
```toml
members = [..., "clara-transduction", "clara-baloroptik"]
```

---

## Verification

```bash
# Build
cargo build -p clara-baloroptik

# Offline mode — from existing snapshot file in repo root
baloroptik file deduction_snapshots.json --out-dir /tmp/eye

# Verify DOT is valid Graphviz
dot -Tsvg /tmp/eye/*_final.dot > /dev/null && echo "DOT OK"

# Online mode — requires running clara-api with a traced deduction
baloroptik trace <UUID> --api http://localhost:8080 --out-dir /tmp/eye --format both

# Inspect output sequence
ls /tmp/eye/
# 550e8400_000_initial.dot  550e8400_001_prolog_to_clips.dot  ...
```

---

## Status (v1 + v2 complete)

**v1** — implemented 2026-04-10:
- `clara-baloroptik` crate added to workspace
- `baloroptik file` — offline snapshot → single colorized DOT
- `baloroptik trace` — online API → sequenced DOT per phase
- `--format dot|svg|both`, `--out-dir`, `--link-shared` flags

**v2** — implemented 2026-04-10:
- `baloroptik list` — `GET /deduce` endpoint + list subcommand
- `baloroptik watch` — live-poll, streams DOT files as phases arrive
- `baloroptik replay` — offline replay from snapshot + tableau_changes export
- `--format html` across all multi-phase subcommands — self-contained viz.js step-through viewer
- `GET /deduce/{id}/trace/export` API endpoint for offline export
- `CoireStore::list_snapshots` added to `clara-coire`

---

## v2 Roadmap

### 1. `baloroptik list` — enumerate persisted deductions

**What:** A `list` subcommand that prints a table of all deduction runs stored
in the persistence layer so the user can pick a UUID to pass to `trace`.

**Blocker:** `clara-api` has no `GET /deduce` endpoint. The current handlers
only expose individual deductions by UUID (`GET /deduce/{id}`). A list endpoint
must be added first.

**Suggested API endpoint:**
```
GET /deduce
```
Returns a JSON array of deduction summary objects, paged or limited:
```json
[
  {
    "deduction_id": "550e8400-...",
    "status":       "converged",
    "cycles_run":   3,
    "initial_goal": "mortal(X)",
    "created_at_ms": 1744286400000
  },
  ...
]
```

**Where to add it:**
- `clara-coire/src/store.rs` — add `list_snapshots() -> Result<Vec<DeductionSnapshot>>` (DuckDB `SELECT` on the snapshots table, ordered by `created_at_ms DESC`)
- `clara-api/src/handlers/deduce_handler.rs` — new `list_deductions` handler wired to `GET /deduce`
- `clara-baloroptik/src/main.rs` — add `Commands::List` variant
- New `clara-baloroptik/src/list_mode.rs` — calls `GET /deduce`, pretty-prints table

**CLI shape:**
```
baloroptik list [--api URL] [--limit N]
```

---

### 2. HTML/viz.js step-through viewer

**What:** A `--format html` output mode (or a dedicated `view` subcommand) that
emits a **single self-contained HTML file** embedding all DOT sources for a
trace. The browser renders each graph with [@viz-js/viz](https://github.com/nicowillis/viz.js)
(bundled inline or via CDN) and lets the user step forward/backward through
phases with keyboard arrows or buttons.

**Why it's natural here:** `baloroptik trace` already has the ordered DOT
strings in memory before writing them to disk. Generating HTML is just another
output pass over the same data.

**Proposed output:** `<stem>_trace.html` — a single file, no external assets
required when using a CDN-hosted viz.js build.

**Key design decisions for the session:**
- Inline viz.js via `<script src="https://cdn.jsdelivr.net/npm/@viz-js/viz@3/...">` vs. bundling the WASM locally
- Phase navigation: keyboard arrows + prev/next buttons; show cycle number and phase name as a caption
- Truth-value legend embedded in the page
- Optionally: scrubber/timeline showing all phases across cycles in a strip

**Implementation sketch:**
```rust
// In render.rs
pub fn write_html(phases: &[(u32, String, String)], path: &Path)
// phases: Vec<(cycle_num, phase_name, dot_src)>
// Writes a self-contained HTML page with all DOTs embedded as JS strings
```

---

### 3. `--watch` — live-polling a running deduction

**What:** Poll a running deduction mid-cycle, streaming new DOT graphs to disk
(and optionally opening/refreshing the HTML viewer) as each phase is recorded.

**How it would work:**
1. User runs `baloroptik watch <DEDUCTION_ID> --api ... --out-dir ...`
2. Tool polls `GET /deduce/{id}` every N ms until status is terminal
3. Simultaneously polls `GET /deduce/{id}/trace` to detect newly recorded phases
4. For each new phase seen: immediately fetch its DOT and write to disk
5. On terminal status: write a final summary and exit

**Key challenge:** The trace list endpoint returns all phases recorded so far.
The tool needs to track which `change_id`s it has already downloaded and only
fetch new ones. A simple `HashSet<Uuid>` of seen change_ids suffices.

**Poll interval:** Configurable via `--poll-ms <MS>` (default 500). A short
interval is fine since the API is local; the trace phases are coarse-grained
(one per relay step) so there will rarely be more than a few per second.

**CLI shape:**
```
baloroptik watch <DEDUCTION_ID> [--api URL] [--out-dir DIR] [--format FORMAT] [--poll-ms N]
```

---

### 4. Offline trace playback from a `tableau_changes` dump

**What:** A `baloroptik replay` subcommand that works entirely offline from a
JSON export of the `tableau_changes` table, without a running clara-api. This
is useful for post-mortem analysis of completed runs when the server is no
longer up.

**Required data:** Two files (or a combined export):
- A deduction snapshot JSON (same as `baloroptik file` already accepts)
- A `tableau_changes` dump: `Vec<TableauChange>` as JSON

**`TableauChange` structure** (from `clara-coire/src/store.rs`):
```rust
pub struct TableauChange {
    pub change_id:      Uuid,
    pub deduction_id:   Uuid,
    pub cycle_num:      u32,
    pub phase:          String,   // "initial", "prolog_to_clips", "clips_to_prolog", "final_*"
    pub event_origin:   Option<String>,
    pub event_type:     Option<String>,
    pub event_data:     Option<String>,
    pub entries_json:   String,   // JSON-encoded Vec<PredicateEntry>
    pub recorded_at_ms: i64,
}
```

**What baloroptik would do:**
1. Read snapshot JSON → get `prolog_clauses` → `parse_prolog_rules` (same as `file` mode)
2. Read `tableau_changes` dump → deserialize `Vec<TableauChange>`, filter to matching `deduction_id`
3. For each change in `cycle_num` + `recorded_at_ms` order: decode `entries_json` → `coloring_from_entries` → `generate_dot` locally
4. Write sequenced DOT files (same naming as `trace` mode)

**Export mechanism needed:** Either a new clara-api endpoint
(`GET /deduce/{id}/trace/export` → full `Vec<TableauChange>` as JSON) or a
direct DuckDB export CLI on the server side. The export format should be a
plain JSON array of `TableauChange` objects for simplicity.

**CLI shape:**
```
baloroptik replay <SNAPSHOT_JSON> <CHANGES_JSON> [--out-dir DIR] [--format FORMAT]
```
