# leannan_sidhe — divergent retrieval for the Id analyst (planning draft)

> **Status:** approved 2026-09-11, round 2 closed. Repo fork done. SPEC-084
> signed off 2026-09-11. **Tier 1 (Edgequake) and Tier 2 (`the_leannan.pl`)
> are both implemented, tested, and committed** — see the "done" notes
> under §Tier 1 and §2d below. Tier 3 (`id_analyst.pl`) not started; the
> live clara-api rebuild/restart + end-to-end `/deduce` verification are
> also still outstanding (deliberately deferred to alongside Tier 3).
> Round-1 and round-2 `[STAN]` feedback are both folded in below; see
> **Review round 1 — resolutions** and **Review round 2 — resolutions** at
> the bottom for the point-by-point trace.

## Context

The frontdesk POC is growing a Freudian theory-of-mind arc (Id / Ego / Superego)
across its assistant rulesets. `deliberative_analyst.pl` (Robert's Rules
deliberation) is the Superego prototype: procedural, converges to one decision.
`id_analyst.pl` is the opposite pole — surface many independently-generated
candidate responses, no judgment, "raw impulse."

Today `id_analyst.pl` fires one `caws_offer` per fixed persona to 4 model-diverse
seats, all working from the **same prompt**. Result: few, similar, boring
suggestions. What Id needs is *many* *varied* *creative* candidates, each grounded
in a genuinely different slice of the knowledge corpus, with a citation trail and
a rule-based (not LLM-loop) orchestration that stays explainable and auditable.

The chosen mechanism (from `clara-cerebellum/docs/Let's explore abusing
Mix_RRF.md`): repurpose Edgequake's Reciprocal Rank Fusion — normally a *consensus*
device across retrieval arms — as a *divergence* device. Each Id candidate is
seeded by a "spark": a retrieval anchored on a **graph-mechanical perturbation**
of the query's entities (neighbor-hop, sibling-by-label, bridge), fused with
deliberately skewed arm weights, a varied RRF `k`, and a new **fringe** fusion
mode that rewards chunks that are present-across-arms but ranked poorly in each.

Named **leannan_sidhe** (the Gaelic muse / fairy-lover who inspires artists) as an
end-to-end subsystem spanning both layers:
- **Edgequake**: minimal new divergent-fusion primitives on `POST /api/v1/query`.
- **clara-cerebellum**: a `the_leannan.pl` prolog-lib module (mirrors `the_cow.pl`)
  that walks the graph and drives those primitives.
- **lildaemon**: `id_analyst.pl` rewired to consume sparks; a configurable count.

Scope for this pass: **produce abundance only.** Selection / ranking /
cross-pollination ("glean the best candidates", genetic-algorithm soup) stays a
later Superego / Ego concern, consistent with `id_analyst.pl`'s current "no
judgment" framing.

### Decisions locked for this draft

| Question | Decision |
|---|---|
| How far into Edgequake's Rust source? | Add real EQ functionality, but only the slice Dis (Rust + Prolog) needs — not a general feature build. |
| Where does the name "leannan_sidhe" attach? | Both layers — `the_leannan.pl` in Prolog **and** a leannan-labelled divergent path in Edgequake. |
| Per-stream "lateral shift" query mutation? | Graph-mechanical — derive divergence from the knowledge graph itself, no LLM / think / tool loop. |
| Build the "glean best candidates" selection layer? | No — produce abundance only this pass. |
| Fringe fusion math | Keep the RRF shape, **reflected rank** (score from the bottom of each arm list). Band-pass is a fast-follow second mode, not v1. |
| `rrf_k` | Raw per-request value only. No jitter / randomization — determinism and explainability first. |
| Graph operators (v1) | `neighbor(Hops)`, `sibling`, `bridge` only. `relation_hop` and `contrast` deferred. |
| Spark count | Default 6, accept the Groq latency. `LEANNAN_LOCAL_ONLY` toggle for a remotes-free first cut. |
| Divergence persona | Decoupled from the graph operator: fixed rotating "impulse" voices in `system`; operator intent framed in the `prompt` body. |
| Edgequake SPEC process | Write a full new SPEC doc; likely an upstream PR. Sequencing question in §0. |

---

## 0. Edgequake SPEC doc — write it first

Edgequake is spec-driven: contract tests (`spec083_matrix_contracts.rs`,
`contract_rrf_fusion.rs`) literally read the spec/source for the fusion-mode name
set, and `@implements SPEC-xxx` annotations are the review currency. If this goes
upstream as a PR, the SPEC doc is the artifact reviewers want first.

**Recommendation: write the SPEC before any Rust.** It pins the fringe-fusion math
and the request-field contract (`rrf_k`, `fusion`, later `band_lo`/`band_hi`)
before code, and gives the team + upstream a single thing to sign off on.

- Repo layout confirmed: the git root is `~/moonpool/tools/edgequake` (the
  `edgequake/` subdirectory below it is just the Rust workspace, not a nested
  repo). Spec convention is **`specs/NNN-slug/`** — numbered, incrementing,
  each a directory of docs (`docs/adr/` cross-references it, e.g.
  `specs/073-relational-rag-layout/`). Highest existing number is
  `specs/083-improvements` (matches the `spec083_matrix_contracts.rs` test
  name in §1c). `specifications/` is a *different*, older/parallel numbering
  scheme (`0001_..._issueNNN`) — not the one `@implements SPEC-NNN` doc
  comments reference; ignore it for this work.
- New `specs/084-leannan-sidhe-divergent-fusion/` (or similar slug) —
  `SPEC-084: Per-request fusion control (rrf_k, fusion strategy, fringe mode)`.
  Rides conceptually alongside SPEC-022 (per-request Mix weight overrides) and
  SPEC-023 (retrieval fusion), but is its own doc.
- Contents: the two new `QueryRequest` fields; the `MixFusionMode::Fringe`
  definition and math; the reserved `bandpass` mode + `band_lo`/`band_hi` fields
  (documented, not implemented in this pass); health-endpoint label additions;
  backward-compat statement (all `Option`, env defaults unchanged).

Team sign-off on the SPEC gates Tier 1 implementation.

**Repo fork — done 2026-09-11.** Per round-2 feedback below: `origin` on the
local `~/moonpool/tools/edgequake` checkout now points at a new private
GitLab project, `root/edgequake` on limbic
(`http://limbic:8929/root/edgequake`, branch `edgequake-main`, pushed).
`upstream` still points at `github.com/raphaelmansuy/edgequake` for pulling
updates from the real project. Work happens on GitLab first (matches the
`origin`=GitLab / `github`=backup pattern used elsewhere in the stack); a
`stancampbell3/edgequake` GitHub fork as an explicit push-mirror backup is
deferred until Tier 1 is far enough along to be worth a backup copy or an
upstream PR — revisit then.

**SPEC-084 — drafted, signed off, and implemented (Tier 1) 2026-09-11.**
`specs/084-leannan-sidhe-divergent-fusion.md` in the fork (commit
`944f61e4`). Covers FR-001–005 (per-request `rrf_k`, per-request `fusion`
override, the new `Fringe` mode + its reflected-rank math, the exact
call-path threading through `query_mix_with_vector_storage` →
`fuse_mix_contexts`, and the reserved `bandpass` fields), plus API
contract, edge cases, backward-compat, and a test plan. One correction
surfaced while grounding the spec against current source: the
`QueryExecutionParams` struct and `build_engine_request` actually live in
`crates/edgequake-api/src/services/query_request_builder.rs`, not
`handlers/query/query_execute.rs` as originally drafted in §1a/§1b above —
`query_execute.rs` only *calls* `build_engine_request`. Tier 1
implementation should follow the SPEC doc's FR-004 threading section,
which has the corrected path. Sign-off on the SPEC gates starting Tier 1
Rust work.

---

## Tier 1 — Edgequake: divergent-fusion primitives

Repo: `~/moonpool/tools/edgequake/edgequake`. Add only what Prolog cannot compose
from existing endpoints. `mix_weights`, `ll_keywords`, `hl_keywords`,
`context_only`, `max_results`, and all `graph/entities|relationships|neighborhood`
endpoints **already exist** and carry most of the load — no change needed there.

### 1a. Per-request RRF `k`

- `crates/edgequake-query/src/types.rs` `QueryRequest`: add `rrf_k: Option<f32>`.
- `crates/edgequake-api/src/handlers/query_types.rs` `QueryRequest` DTO: add
  `rrf_k: Option<f32>` (with `@implements` doc comment).
- Thread through `QueryExecutionParams` + `build_engine_request`
  (`handlers/query/query_execute.rs`) exactly as `mix_weights` is threaded.
- `crates/edgequake-query/src/engine_impl/modes/mix.rs`:
  `query_mix_with_vector_storage` gains the value; `fuse_mix_contexts` replaces
  the literal `crate::fusion::RRF_K` with `rrf_k.unwrap_or(crate::fusion::RRF_K)`.
- Raw value only — no jitter field. Profiles in `the_leannan.pl` pick explicit
  constants (60 / 200 / 500).

### 1b. Per-request fusion strategy override + new `Fringe` mode

**Fringe = reflected-rank RRF.** Keep RRF's exact shape — the `1/(k+r)` curve and
the additive cross-arm reward — but measure rank from the *bottom* of each arm
list. A chunk at 0-indexed `rank` in an arm of length `N` contributes
`weight / (k + (N - 1 - rank) + 1)` = `weight / (k + N - rank)`. Chunks deep in
every arm's list (present, but ranked poorly everywhere — the "fringe consensus")
get the largest boost; a chunk that tops one list contributes almost nothing.
An optional `min_arms` (default 2) drops chunks that appear in only one arm, so
the result is genuinely *cross-arm* fringe, not just one arm's tail.

- `crates/edgequake-query/src/fusion.rs`:
  - add `MixFusionMode::Fringe`;
  - `mix_fusion_mode_label` → `"fringe"`;
  - `fn mix_fusion_mode_from_str(&str) -> Option<MixFusionMode>` (parse helper,
    reuse the alias table already in `mix_fusion_mode_from_env`);
  - `fn fringe_rrf_fusion(ranked_lists, weights, k, min_arms) -> Vec<(String, f32)>`
    — reflected-rank contribution as above, summed; drop ids seen in `< min_arms`
    lists; sort descending. Unit tests mirroring the existing `rrf_*` tests
    (symmetric lists → tied scores; a top-of-all-lists chunk ranks *last*).
- `types.rs` / DTO: add `fusion: Option<String>` to both `QueryRequest`s; thread
  like `rrf_k`. Accepted values: `rrf` (default), `fringe`, `round_robin`,
  `max_after_minmax`. `bandpass` reserved (see below), rejected with a clear
  error until implemented.
- `mix.rs` `fuse_mix_contexts`:
  `let mode = req_fusion.and_then(mix_fusion_mode_from_str).unwrap_or_else(mix_fusion_mode_from_env);`
  then a `MixFusionMode::Fringe` arm calling
  `fringe_rrf_fusion(&ranked_lists, &weights, rrf_k, 2)` → `chunks_from_rrf_ranking`.
- `crates/edgequake-api/src/handlers/health.rs` + `health_types.rs`: extend the
  `mix_fusion` label doc to include `fringe`.

**Band-pass — fast follow, documented now, not built.** A second mode that keeps
only chunks whose rank sits in `[band_lo, band_hi]` in ≥1 arm, then runs standard
RRF among the survivors. Request fields `band_lo` / `band_hi` (or `fusion_band:
[lo, hi]`). Team wants to A/B this against reflected-rank fringe once both exist;
reflected-rank ships first because it needs no new tuning params.

### 1c. Contract / spec upkeep

- `crates/edgequake-api/tests/spec083_matrix_contracts.rs` and
  `crates/edgequake-query/tests/contract_rrf_fusion.rs` assert the fusion mode
  name set — extend for `fringe` and the per-request override, referencing the
  new SPEC number.
- `context_only` responses already return `sources: Vec<SourceReference>` with
  `score`. Per-arm provenance on `SourceReference` is **out of scope** for v1 (the
  audit trail lives Prolog-side, see Tier 2); revisit if the tableau needs it.

**Tier 1 — done 2026-09-11.** Implemented against SPEC-084 exactly as
specced, committed to the GitLab fork (`06a0643a`, pushed). 17 files
changed across `edgequake-query` and `edgequake-api`:
- `fusion.rs`: `MixFusionMode::Fringe`, `fringe_rrf_fusion` (reflected-rank
  formula), `mix_fusion_mode_from_str` (strict per-request parser — the
  shared alias table refactored out of `mix_fusion_mode_from_env` so both
  share the alias list but differ on unrecognized-value handling: the env
  parser still silently falls back to `Rrf`, the per-request parser
  returns `None`/`Reserved` for the API layer to turn into a `400`).
- `mix.rs`: `query_mix_with_vector_storage` / `fuse_mix_contexts` now take
  explicit `rrf_k: Option<f32>, fusion: Option<&str>` params instead of
  reading `RRF_K` / the env fusion mode unconditionally — threaded through
  both call sites (`query_pipeline.rs`'s vector-storage and default-storage
  paths) and the `query_mix` convenience wrapper (`query_modes.rs`).
- API layer: `rrf_k`/`fusion` added to the HTTP `QueryRequest` DTO and the
  engine `QueryRequest`, threaded through `QueryExecutionParams` /
  `build_engine_request` (confirmed home:
  `services/query_request_builder.rs`, per the FR-004 correction);
  `validate_fusion_override` (`services/query_execution.rs`, next to
  `validate_llm_override_pair`) rejects an unrecognized or reserved
  (`bandpass`) `fusion` value with `ApiError::BadRequest` — wired into
  `execute_query` right after `validate_query`. `/api/v1/query/stream` and
  `/api/v1/query/context` deliberately **not** touched — matches SPEC-084's
  stated scope (`/api/v1/query` only) and existing precedent (neither
  already supports `hl_keywords`/`ll_keywords` overrides either).
- Contract tests extended: `spec083_matrix_contracts.rs`
  (`contract_fusion_mode_names`) and `contract_rrf_fusion.rs` both assert
  the `fringe` mode name, the reserved `bandpass` handling, and the
  defining inversion property (a chunk ranked #1 in every arm ranks
  **last** under fringe).

Verified: `cargo test -p edgequake-query --lib` 288/288 pass; `edgequake-api`
query-path tests (78 lib tests + `spec083_matrix_contracts` with
`--features postgres`) all pass except one pre-existing, unrelated failure
(`contract_tasks_pk_documented` — a docs-file-existence check untouched by
this change). `cargo check --workspace` clean.

---

## Tier 2 — clara-cerebellum: `the_leannan.pl` + tool args

### 2a. `edgequake` tool argument surface

`clara-cerebellum/clara-toolbox/src/tools/edgequake.rs`:
- `EdgequakeArgs`: add `rrf_k: Option<f32>`, `fusion: Option<String>`,
  `context_only: Option<bool>`, `hl_keywords: Option<Vec<String>>`,
  `ll_keywords: Option<Vec<String>>`.
- `EdgequakeClient::query(...)`: forward the new fields as JSON body keys
  (same pattern as the existing `llm_provider` / `llm_model` conditionals).
- Extend the `test_query_args_*` unit tests.
- Graph ops (`graph_search_entities`, `graph_get_entity`,
  `graph_entity_neighborhood`, `graph_search_relationships`) already exist —
  no change.

### 2b. New prolog-lib module `clara-prolog/prolog-lib/the_leannan.pl`

Structure copied from `the_cow.pl` verbatim (build `{tool, arguments}` dict →
`the_rabbit:clara_evaluate/2` → `atom_json_dict(_, _, [value_string_as(atom)])`
→ status check). `:- module(the_leannan, [...])`.

**Graph primitives** (thin wrappers over the `edgequake` tool graph ops):
- `leannan_entities(+Query, -Entities)` — `graph_search_entities(search=Query)`,
  extract entity names + labels.
- `leannan_neighborhood(+Entity, +Hops, -Neighbors)` — `graph_entity_neighborhood`
  (iterate for Hops > 1).
- `leannan_by_label(+Label, -Entities)` — `graph_search_entities(label=Label)`.
- `leannan_relationships(+Src, +Tgt, -Rels)` — `graph_search_relationships`.

**Divergence operators (v1 — three)** —
`leannan_perturb(+Seeds, +Operator, -Perturbed, -Prov)`, pure graph walking:

| Operator | Walk | Intent |
|---|---|---|
| `neighbor(Hops)` | N-hop neighborhood of the seed entities | "structural leap" — what the topic touches |
| `sibling` | entities sharing a seed's `label`, minus the seeds | "peers / alternatives" — other things of the same kind |
| `bridge` | entities on relationships linking two distinct seeds | "connective tissue" — what joins the query's own entities. Needs ≥2 seeds; degrades to `neighbor(1)` with one seed |

`Prov` records the operator, the seed entities, and the intermediate walk for the
audit trail. `relation_hop` (follow one named relation type) and `contrast`
(same-label, graph-distant) are **deferred** — `relation_hop` is largely
subsumed by `neighbor` for v1, and `contrast` needs a graph-distance primitive
Edgequake does not expose (expensive multi-hop + negative selection, fuzzy).
Flagged for round-2 feedback.

**Spark builder** — `leannan_spark(+Query, +Profile, -Spark, -Prov)` where
`Profile = spark(Operator, Weights, K, Fusion)`:
1. `leannan_entities(Query, Seeds)`
2. `leannan_perturb(Seeds, Operator, Perturbed, PProv)`
3. `edgequake` query: `operation: query`, `query: Query`, `context_only: true`,
   `ll_keywords: Perturbed`, `mode: mix`, `mix_weights: Weights`, `rrf_k: K`,
   `fusion: Fusion`
4. `Spark = spark_result(Chunks, Citations)`; `Prov` = full parameter record.

**Fan-out** — `leannan_sparks(+Query, +Count, -Sparks, -AllCitations)`:
`leannan_profiles/1` is a fixed, deterministic list of 6 profiles; take the first
`Count`, or cycle if `Count > 6`:

| # | Operator | Weights (local/global/naive) | k | Fusion |
|---|---|---|---|---|
| 1 | `neighbor(2)` | 3.0 / 1.0 / 0.2 | 60 | rrf |
| 2 | `neighbor(1)` | 1.0 / 1.0 / 1.0 | 500 | rrf |
| 3 | `sibling` | 1.0 / 3.0 / 0.5 | 60 | rrf |
| 4 | `sibling` | 1.0 / 1.0 / 1.0 | 200 | fringe |
| 5 | `bridge` | 2.0 / 2.0 / 0.3 | 60 | rrf |
| 6 | `bridge` | 1.0 / 1.0 / 0.5 | 500 | fringe |

Explicit recursion over the profile list (codebase style, not `maplist`).

**Audit facts** — `:- thread_local spark/3.` `:- thread_local spark_cites/2.`
Reuse `the_cow.pl`'s `citation/8` + `cite/2` (import from `the_cow`), plus a
`leannan_and_assert_citations/3` mirroring `ruminate_and_assert_citations/3`.

### 2c. Auto-load

`clara-prolog/src/backend/ffi/environment.rs` (~line 107): add `"the_leannan"` to
`["the_coire", "the_rabbit", "the_cow", "the_rat"]`. `build.rs` already copies the
whole `prolog-lib/` overlay — no packaging change.

### 2d. Tests

Prolog module load test + arg-parse tests alongside the `the_cow` equivalents.

**Tier 2 — done 2026-09-11.** Implemented in `clara-cerebellum` (uncommitted
locally as of this writing — not yet pushed to the GitLab mirror; do that
alongside committing). Several corrections surfaced against live source
while implementing, same pattern as Tier 1's FR-004 correction:

- **§2a (Rust).** `EdgequakeArgs` gained `context_only`, `hl_keywords`,
  `ll_keywords`, `mix_weights` (a new `MixWeightsArg` struct — **not** in
  the original §2a list, but required by §2b step 3's spark builder, which
  always specifies `mix_weights`; a real omission in the original draft),
  `rrf_k`, `fusion`, and `depth`. **Two real, pre-existing wire-format bugs
  found and fixed** while wiring this up, confirmed against Edgequake's
  actual `ListEntitiesQuery`/`ListRelationshipsQuery` structs: (1)
  `graph_search_entities`/`graph_search_relationships` were sending a
  `label=`/`limit=` query param that Edgequake's real endpoints don't
  recognize (the real filter keys are `entity_type`/`relationship_type`,
  and the real page-size key is `page_size`) — both silently no-op'd
  server-side (no `deny_unknown_fields`) since these tools were first
  written; now fixed. (2) `graph_entity_neighborhood` never forwarded a
  `depth` param at all even though the endpoint supports one natively
  (server-clamped `[1,3]`) — now threaded through. 10 unit tests, all
  passing (`cargo test -p clara-toolbox edgequake`).
- **§2b (Prolog, `the_leannan.pl`).** Implemented close to the draft, with
  three implementation-time corrections (documented in the file's own
  module doc comment, same spirit as the Tier 1 FR-004 correction):
  1. `leannan_neighborhood/3` passes `Hops` straight through as
     Edgequake's native `depth` param (see the Rust fix above) instead of
     the draft's "iterate for Hops > 1" — one call does what the draft
     assumed needed several.
  2. **`bridge` does not use a `leannan_relationships(+Src,+Tgt,-Rels)`
     primitive** — confirmed live that Edgequake's `/graph/relationships`
     list endpoint has no source/target filter at all (only pagination +
     `relationship_type` — see the Rust fix above and
     `clara-toolbox/src/tools/edgequake.rs`'s doc comment on `source`/
     `target` for the full finding). `bridge` instead intersects the
     1-hop neighborhoods of each pair of seeds in Prolog — entities
     appearing in both *are* "what joins the query's own entities",
     without depending on a filter Edgequake doesn't have.
     `leannan_relationships/2` (label/`relationship_type`-only — the one
     filter that's real) is still exported for future operators
     (`relation_hop`), just not used by `bridge`.
  3. `leannan_and_assert_citations/4` (not `/3`) — the extra `SparkId` arg
     is needed to key `spark_cites/2` (which spark used which citation),
     something the_cow.pl's non-spark-scoped `ruminate_and_assert_citations/3`
     has no reason to track. Also: the actual per-spark Edgequake RAG
     query reuses `the_cow:ruminate_opts/3` / `ruminate_and_assert_citations/3`
     directly (their `Opts.put/1` merge already forwards any QueryRequest
     field — `context_only`, `ll_keywords`, `mode`, `mix_weights`, `rrf_k`,
     `fusion`) rather than re-implementing that dispatch a second time;
     only the four graph-walk primitives (which the_cow.pl has no
     equivalent for) get their own dispatch helper, `leannan_dispatch/2`.
  `leannan_spark/5` (SparkId added vs. the draft's `/4`, needed for the
  `spark/3` memo key) and `leannan_sparks/4` implemented as drafted,
  including the assert-once memoization (mirrors
  `deliberative_analyst.pl`'s `committee_deadline_for/3` pattern) and the
  explicit recursion over the profile list.
- **§2c (auto-load).** Done — `"the_leannan"` added to
  `environment.rs`'s auto-load list.
- **§2d (tests).** 5 Prolog integration tests added to
  `clara-prolog/tests/prolog_integration_tests.rs`: library auto-load,
  `leannan_profiles/1` structure, and — via a `MockEdgequakeTool` `Tool`
  registered in `ToolboxManager` (mocking `the_rabbit:clara_evaluate/2`
  itself doesn't work: it's a genuine `PL_register_foreign` C predicate,
  not a plain interpreted one like the_rat.pl's mocked predicates, so
  `abolish`/`assertz` on it silently no-ops — documented on
  `MockEdgequakeTool`) — `leannan_entities/2` and `leannan_neighborhood/3`
  dict-field extraction, and `leannan_spark/5`'s memoization (proven via a
  shared call counter: a second call with the same SparkId does not
  re-invoke the mocked query). All pass; full `clara-prolog` (25
  integration + 9 unit) and `clara-toolbox` (58) suites pass; full
  `cargo build --workspace` clean.

**Not done this pass:** rebuilding/restarting the live `clara-api`
service to pick up the new binary (§Build & sequencing step 2's "Rebuild
the clara-api / FieryPit images" — a deploy-affecting action on the live
Clara stack, deferred to be done deliberately alongside or after Tier 3,
not mid-Tier-2). The design doc's Tier 2 live-verification step (a
scratch `/deduce` against a running clara-api) is accordingly still
outstanding.

---

## Tier 3 — lildaemon: `id_analyst.pl` on sparks

`lildaemon/goat/app/assistant/rulesets/id_analyst.pl`:

- **Keep**: `strip_think/2`, `extract_caws_response/2`, the empty-response guard,
  fire-all-then-await-all, `render_alternatives/2` markdown shape.
- **Seat pool**: the 4 model-diverse member seats (`member-gemma`, `member-qwen`,
  `groq-splinter`, `member-groq-gptoss`) by default. `LEANNAN_LOCAL_ONLY=1`
  restricts to the local seats (`member-gemma`, `member-qwen`) for a remotes-free
  first cut — a peer-FieryPit small-model seat (e.g. Pineal) is a later addition
  to that pool, not this pass.
- **Replace** the fixed `id_member/4` prompt-fan-out with:
  1. `leannan_sparks(Query, Count, Sparks, SparkCitations)` — memoized (see below).
  2. For each spark, `caws_offer` to a seat, round-robin over the pool. `Count`
     may exceed the pool size — distinct payload per spark ⇒ distinct caws
     idempotency key, offers just queue on the evaluators.
     - `system` = a **fixed rotating impulse voice** (`blunt`, `associative`,
       `provocative`, `contrarian`, `lateral`, `skeptical`), index `i mod 6` —
       **not** derived from the spark's operator.
     - `prompt` = impulse instruction + a one-line **operator framing** ("These
       passages sit 2 hops from your topic in the knowledge graph — react to the
       lateral connection, don't summarize") + the spark's retrieved evidence
       block + the original query.
     - `model`, `num_ctx` from the seat as now.
  3. `await_id_members/2` as now; degrade per-member as now.
- `id_step/2` → `id_step/3(+Query, +Count, -Alternatives)`.
- `Count`: `( getenv('LEANNAN_SPARK_COUNT', S), atom_number(S, N), N > 0 -> true
  ; N = 6 )` read inside `id_step/3`. (Per-session override is future work.)
- `assistant_turn/5`: **arity unchanged**; now binds real
  `Citations` / `CitationCount` from `SparkCitations` (union, dedup by id) instead
  of `[], 0`. Add a per-impulse citation footnote in `render_alternative/2`.

### Correctness: memoize the retrieval, don't re-run it per cycle

The classify deduction for `id` is multi-cycle (it awaits `caws` legs — pending
⇒ clause fails ⇒ retried next engine cycle). A bare `edgequake` call inside
`id_step` would re-execute on **every** retry cycle (the anti-pattern
`rituals_101.md` warns about for `ponder_text`). Two mitigations, both applied:
- `context_only: true` on every leannan query (no LLM generation ⇒ no
  nondeterminism, cheaper), **and**
- assert-once memo: `leannan_spark` asserts `spark(SparkId, Key, spark_result(...))`
  `thread_local` on first computation keyed by `(Query, Profile)`; retry cycles
  read the fact and fast-forward — exactly the `committee_deadline_for/3` pattern
  in `deliberative_analyst.pl`.

So `id_step/3` is effectively two phases: phase 1 computes + memoizes all sparks
(runs once); phase 2 fans out `caws_offer`/`caws_await` (cached by the caws
idempotency cache). Retry cycles skip phase 1 and fast-forward resolved legs.

### `runtime.py`

- No arity change; `assistant_turn/5` still the goal.
- `turn()`'s `id` branch already calls `_ensure_deliberation_seats()` before
  classify — unchanged (same seats). If `LEANNAN_LOCAL_ONLY` ever needs *fewer*
  seats joined, that is a `_ensure_deliberation_seats` refinement, not required
  for correctness (joining an unused seat is harmless).
- Confirm the classify deduction's Prolog engine reaches the `edgequake` tool from
  inside `/deduce` — it does: `progressive_research.pl`'s `answer_step/9` already
  calls `ruminate_opts/3` live from a registered ruleset the same way.
- `_RULESETS` already contains `id_analyst.pl` — no registration change.

### Tests

- Extend `lildaemon/tests/test_assistant_runtime_turn.py` id cases (mock the
  deduce result to include `Citations`; assert they propagate to the turn
  result and `touch_cited_documents` is called).
- `test_assistant_runtime_rulesets.py` already covers registration/syntax load.

---

## Docs & memory

- New `clara-cerebellum/docs/leannan_sidhe.md` — the cross-repo design record
  (this file becomes that once approved).
- Edgequake: the new SPEC doc (§0).
- `lildaemon/docs/assistant_demo.md` — rewrite the "Id ruleset" section's control
  flow; move the "Edgequake retrieval diversification" future-work bullet to
  implemented.
- `clara-cerebellum/docs/rituals_101.md` — add `the_leannan` to the auto-loaded
  libraries list.
- Memory: update `assistant_demo_progress.md`; new entry for leannan_sidhe.

---

## Build & sequencing

0. **Edgequake SPEC doc** — write it, team + upstream sign-off. Gates step 1.
1. **Edgequake** (Tier 1) — implement against the SPEC. Build + `cargo test`
   in `~/moonpool/tools/edgequake/edgequake`; update contract tests; open the
   upstream PR; redeploy the Edgequake service the stack points at.
2. **clara-cerebellum** (Tier 2) — `the_leannan.pl` + tool args + auto-load.
   `cargo build` clara-cerebellum; `cargo test` clara-toolbox + clara-prolog.
   Rebuild the clara-api / FieryPit images that bake the prolog-lib overlay.
3. **lildaemon** (Tier 3) — `id_analyst.pl` + tests. `python -m pytest`.

Each tier is independently testable; Tier 3 degrades gracefully if Tier 1's new
params are ignored by an un-upgraded Edgequake — they're all `Option`, and the
`QueryRequest` DTO (`handlers/query_types.rs`) does **not** set
`deny_unknown_fields`, so unknown JSON body keys are silently dropped (verified).
`the_leannan.pl` still produces graph-perturbed `ll_keywords` + `mix_weights`
divergence, just without `rrf_k` / `fringe`, until Tier 1 ships.

---

## Verification (end to end)

1. **Edgequake unit**: `cargo test -p edgequake-query fusion` — new
   `fringe_rrf_fusion` + `rrf_k` override tests pass (top-of-all-lists chunk ranks
   last; symmetric lists tie).
2. **Edgequake live**: `curl POST /api/v1/query` with
   `{"query":"...", "mode":"mix", "context_only":true, "rrf_k":500,
   "fusion":"fringe", "ll_keywords":["<off-topic entity>"],
   "mix_weights":{"local":3,"global":1,"naive":0.2}}` against a populated
   workspace — returns a visibly different `sources` set than the same query with
   defaults.
3. **the_leannan.pl**: a scratch `/deduce` (hand-authored `prolog_clauses` calling
   `use_module(library(the_leannan))`, `leannan_sparks("<demo query>", 6, S, C)`)
   against the running clara-api — `S` has 6 distinct chunk sets, `C` non-empty,
   `spark/3` facts asserted.
4. **id_analyst end to end**: `LEANNAN_SPARK_COUNT=6`, drive a real turn through
   `POST /assistant/sessions/{id}/send` with `ruleset_key=id` — reply renders 6
   markdown impulse blocks, each visibly seeded by different evidence, each with a
   citation footnote; `citation_count > 0` in the response. Repeat with
   `LEANNAN_LOCAL_ONLY=1` — still 6 blocks, local seats only.
5. **Browser**: existing Playwright e2e (`clara-frontdesk-poc/tests/e2e/`) with the
   `id` ruleset selected — screenshot the multi-impulse bubble.
6. **Regression**: full `python -m pytest` in lildaemon; `cargo test` in
   clara-cerebellum and edgequake.

---

## Review round 1 — resolutions

1. **Fringe fusion semantics.**
   `[STAN]` i like the alternative approach keeping RRF shape but let's not forget
   the band pass idea. it may work better. we'll test this first.
   **→ Resolved.** v1 = reflected-rank RRF (`fusion: "fringe"`, §1b). Band-pass
   documented in the SPEC and built as a fast-follow second mode for A/B testing.

2. **`rrf_k` range / randomization.**
   `[STAN]` since explainability is critical, even when generating creative
   expressions, let's go with the raw call.
   **→ Resolved.** Raw per-request `rrf_k` only, no jitter (§1a). Profiles use
   explicit constants.

3. **Graph operators.**
   `[STAN]` in light of our current plan, let's discuss this point further.
   **→ Proposed (needs round-2 sign-off).** v1 ships `neighbor(Hops)`, `sibling`,
   `bridge` — each 1–2 calls to existing graph endpoints, each fully traceable
   (§2b). `relation_hop` deferred (subsumed by `neighbor`); `contrast` deferred
   (needs a graph-distance primitive EQ lacks). Open for your fresh feedback.

4. **Spark count default.**
   `[STAN]` let's accept the latency. groq gives us remote demonstrability … if we
   need to leave remotes out of the first cut we can limit to only local models …
   push some work onto a peer FieryPit (like Pineal) … small, weird, open source
   models might even be perfect for some rituals.
   **→ Resolved.** Default count 6, accept latency. `LEANNAN_LOCAL_ONLY` toggle
   restricts the seat pool to local models (§Tier 3). Peer-FieryPit small-model
   seats and "weird small models per ritual" logged as tuning follow-ups.

5. **Where the divergence persona lives.**
   `[STAN]` This question needs further review. I'm not exactly sure what is being
   asked.
   **→ Clarified + proposed.** The question: when we move from 4 hardcoded voice
   personas to spark-driven generation, is each spark's `system` persona chosen
   from *which graph operator produced it*, or do we keep fixed voices and attach
   spark evidence regardless? **Proposal: decouple.** Fixed rotating impulse
   voices in `system`; the operator's intent goes in the `prompt` body as an
   evidence-framing line (§Tier 3). The evidence already carries the divergence;
   coupling persona to operator just makes the matrix rigid. Open for round-2.

6. **EQ spec process.**
   `[STAN]` … We may need to do a pull request upstream. … let's create a full new
   SPEC doc. Would writing that before starting implementation be useful?
   **→ Resolved: yes, write it first.** See §0 for the rationale and outline.
   SPEC sign-off gates Tier 1.

### Still open for round 2

- **§2b** — confirm the reduced 3-operator set, or pull `relation_hop` /
  `contrast` back in.
[STAN] affirmative.  let's not lose the relation_hop and contrast tasks for later.

- **§Tier 3** — confirm persona/operator decoupling.
[STAN] correct

- **§0** — confirm "SPEC before code" and which repo dir is canonical for it.
[STAN] i think we probably need to fork EQ at this point?  we have our own local gitlab and can operate off of that whilst pushing to github as backup.

- **§1b** — sanity-check the reflected-rank math and the `min_arms = 2` default.
[STAN] we'll go with your recommendations

## Review round 2 — resolutions

1. **Graph operators (§2b).** `[STAN]` affirmative — keep `relation_hop` and
   `contrast` as later work, don't drop them.
   **→ Resolved.** v1 ships the 3-operator set (`neighbor`, `sibling`,
   `bridge`) as drafted. `relation_hop` / `contrast` stay explicitly tracked
   as deferred follow-ups (§2b already documents why each is deferred), not
   cut from scope.

2. **Persona/operator decoupling (§Tier 3).** `[STAN]` correct.
   **→ Confirmed.** Fixed rotating impulse voices in `system`, operator
   intent in the `prompt` body, as drafted.

3. **SPEC-before-code + repo location (§0).** `[STAN]` fork EQ; operate off
   the local GitLab, push to GitHub as backup.
   **→ Resolved.** Repo root confirmed as `~/moonpool/tools/edgequake`;
   spec convention confirmed as `specs/NNN-slug/` (next number **084**).
   Fork stood up 2026-09-11 — see "Repo fork — done" note under §0 above.
   GitHub backup mirror deferred until Tier 1 is far enough along to be
   worth it.

4. **Reflected-rank math + `min_arms` (§1b).** `[STAN]` go with the
   recommendations.
   **→ Confirmed.** Reflected-rank RRF as specified in §1b, `min_arms`
   default 2, ships as drafted — no changes.

**Round 2 closed. Plan fully approved.** Next: write `specs/084-*` (§0),
then Tier 1 implementation.
