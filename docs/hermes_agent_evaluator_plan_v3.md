# Hermes-as-Ego: consolidated plan (v3)

Status: **draft for team review. Docs only, no code written, Phase 0 not yet run.**

Supersedes, for review purposes: `hermes_agent_evaluator_plan.md` (09-18), `..._addendum_2026-09-20.md`
(addendum 1) and `..._addendum_2_2026-09-20.md` (addendum 2). Those stay in place as history.
Inputs, treated as input and not decisions: `clara_feedback_hermes_agent_integration_plan.md`,
`clara_feedback_hermes_agent_integration_190926.md`, `hermes_ego_ritual_brainstorm.md`,
`hermes_ego_ritual_summary.md`, `lildaemon/docs/clara_brainstorm_agent_enablement_hermes.md`.

> **Invariant:** Hard veto governs side-effecting dispatch. Advisory philosophy governs creative content.
> Different axes. Not in tension.

> **Invariant (binding, non-waivable):** Prolog goals are never constructed from text supplied by Hermes.
> Hermes output is parsed and bound as data only. (Recorded in the 2026-09-21 assembly review; see §9.)

> **Note:** The Freudian labels (Id, Ego, Superego) are mnemonics only. They impose no behavioral constraint
> beyond each seat's functional spec.

Status tags: **[decided]** the user made the call (items 9-14 originated with Clara and were confirmed
2026-09-21). **[proposed]** new in v3, needs review. **[open]** unresolved. **[rejected]** dropped, with
the reason.

## 0. Needs your decision first

1. ~~Params handling (§4).~~ Confirmed by the user 2026-09-21. Escalation rule for irreversible or unsure free-form
   actions decided the same day (ledger 28). The remaining FFI detail is a Phase 2 spike.
[STAN]  Not sure what the question is.  Please explain.
2. **Phase 0 runs on limbic** (§6). The earlier docs said it needs Pineal. It does not, except for a final
   cross-host confirmation.
[STAN]  Correct.  We envision running a new example analyst in our clara-frontdesk-poc demo which should integrate our Superego, the new HermesAgentEvaluator as an Ego, and leaving space for future Id work (currently, its suggestions are too wild to be useful).

3. ~~Whether the adopted items in §1 are confirmed.~~ Items 9-14 confirmed by the user on 2026-09-21.
[STAN] Confirmed.

## 1. Decision ledger

| # | Item | Status | Source |
|---|---|---|---|
| 1 | One Superego seat per Ego seat, joined and left together | decided | addendum 1 §2 |
| 2 | Denial (or `caws_await` timeout) escalates to the user; no silent retry, no loop to Id | decided | addendum 1 §3 |
| 3 | Id stays a stateless pipeline stage; no Id quality pass in this work | decided | addendum 1 §4 |
| 4 | Native Hermes toolsets disabled is a hard Phase 0 gate | decided | addendum 2 §2.1 |
| 5 | One Hermes container per Ego seat, hard-stopped by the evaluator, cgroup-bounded, unless Phase 0 finds a real cancel API | decided | addendum 2 §2.2 |
| 6 | `caws_offer`/`caws_await` is the gate; timeout-to-deny is the enforcement; no Kafka control plane | decided | plan §3 |
| 7 | `request_action(action, params, justification)` is the only side-effecting tool | decided; amended by #23 | plan §3 |
| 8 | Promote `caws_tristate/3` into `the_coire.pl` in the same change as `approve_action/4` | decided | addendum 1 §5 |
| 9 | "Structurally deterministic; content-stochastic." Tests assert structure, never content | decided (from Clara, confirmed 2026-09-21) | Clara #5 |
| 10 | Tiered Superego context, with Tier 2 sourced independently of the Ego | decided (from Clara, confirmed 2026-09-21) | Clara #4 |
| 11 | `ritual_id` + `seq` envelope from Phase 1; full audit topics stay Phase 4 | decided (from Clara, confirmed 2026-09-21) | Clara #7 |
| 12 | Firewall is structural: distinct seats and `instance_id`s, no shared Coire subscriptions, no shared Prolog state | decided (from Clara, confirmed 2026-09-21) | Clara #2, revised |
| 13 | User override is an authenticated frontdesk WS action, not a Prolog predicate reachable by the Ego; does not change Superego rules | decided (from Clara, confirmed 2026-09-21) | Clara #6, revised |
| 14 | Governor question answered by the user plus timeout-to-deny; no fourth agent | decided (from Clara, confirmed 2026-09-21) | Clara #6 |
| 15 | Params: allowlist + schema only, no free-text | superseded by §4 | addendum 2 §3 |
| 16 | Shared, serialized Superego (FIFO queue) | rejected | contradicts #1; a Prolog-`assert` queue also reintroduces clause accumulation from the rumination-ingest work |
| 17 | Denial retry, or loop to Id | rejected | contradicts #2 and #3 |
| 18 | Id contract `id_proposal/4` | rejected | contradicts #3; `id_analyst.pl` already emits `Alternatives`; side effects only exist once the Ego calls `request_action` |
| 19 | `failure_mode/3` table as enforcement | rejected | the `caws_await` timeout is the enforcement; a table may exist as documentation only |
| 20 | Prompt string-match as firewall evidence | rejected as evidence | kept as a cheap extra only |
| 21 | 4-week estimate | rejected | |
| 22 | Observer / Id Analyst seat | deferred | not on the Ego critical path |
| 23 | `request_action` is the only tool with **external** side effects (amends #7). Ritual-space read and write are contained and allowed directly as separate MCP tools, with size caps and logging. Anything leaving the ritual space goes through `request_action` | decided (2026-09-21) | §8 |
| 24 | Every ritual-space write records author seat (`instance_id`) and `seq`; the Superego treats Ego-authored documents as Ego-supplied, keeping Tier 2 context independent (extends #10) | decided (2026-09-21) | §8 |
| 25 | Ritual-space concurrency: append mode plus write-with-expected-version; a conflict returns an error. No last-writer-wins | decided (2026-09-21) | §8 |
| 26 | Ritual-space lifecycle keyed by `ritual_id`; reuse Dis ritual TTL reaping with the `persist:true` opt-in | decided (2026-09-21) | §8 |
| 27 | Toolified evaluators migrate over time to the same ritual-space API, replacing path-based file tools | decided (2026-09-21) | §8 |
| 28 | Free-form (semantic-review) route approves only reversible, contained actions; **irreversible or unsure escalates to the user**; timeout denies | decided (2026-09-21) | §4 |

## 2. Architecture

- **`AgentEvaluator`** (new, `lildaemon/goat/evaluators/agent_evaluator.py`) subclasses `Evaluator`
  (`FieryPit.py:109`) and speaks `Offering` (`:26`) in, `Tephra` (`:70`) out, wrapping `Hohi` (`:41`) or
  `Tabu` (`:57`). No Ritual-side changes. It owns the evaluator-side fan-out cap and per-instance
  `workspace_dir` (pattern at `toolified_ollama.py:763-791`).
- **`HermesAgentEvaluator`** talks HTTP to a Hermes container. Registered in `config/evaluators.yaml` like
  `clara_mind_splinter_groq`. Runs as its own FieryPit near the GPU, joined manually via
  `fiery_pit_peer_client.join_remote()`. Auto-placement stays out of scope.
- **One container per Ego seat.** The evaluator can stop it hard; cgroup limits bound it. The fan-out cap is then a
  real guarantee independent of Hermes internals (the 1,393-sub-agent incident is the motivation).
- **Gate.** `request_action` dispatch does `caws_offer/4` (`the_coire.pl:161`) to the paired Superego and
  `caws_await/2` (`:195`). Timeout means deny. The Superego side is a new thin single-shot `approve_action/4`, not a
  Robert's Rules session per action. File placement (new file vs clauses in `deliberative_analyst.pl`) **[open]**,
  Phase 2.
- **Denial.** Terminal "denied" outcome delivered through the pending-research peek/ack channel
  (`ws.rs:350-448`), like `deliberate` was added as a `kind`. User override re-issues the request through an
  authenticated WS action.
- **Where the Ego sits.** Fills the named gap in `id_ritual_of_rituals_planning.md` Part 4: takes Id's
  `Alternatives`, formalizes one, acts only through the gate. Delivery uses existing choke points
  (`impl Handler<TurnResult> for FrontDeskActor`, `ws.rs:451`).

## 3. Superego context and message envelope [decided, items 10-11]

- Tier 1 (always): action, params, justification, ritual state.
- Tier 2 (on request): richer context. The Ego is the gated party, so the Superego must not take Tier 2 from it.
  The user's original request and the Id's `Alternatives` come from the Ritual channel. Only action, params and
  justification come from the Ego.
- Tier 3 (rare): full transcript.
- Every message carries `ritual_id` and `seq` from Phase 1, so failed rituals can be replayed.

## 4. Params handling [decided 2026-09-21, replaces item 15]

Your objection: restricting params to allowlisted structured data would stop the Superego, or other oversight, from
weighing a free-form action with `clara_fy` or other predicates and tools. Agreed. Injection safety should be a
property of how data flows, not of what shape it has.

- The Hermes boundary always speaks JSON: `{action, params, justification, ...}`.
- Parsing happens outside Prolog goal construction. Values are bound as data, never spliced into goal text. This
  closes the injection path Clara raised.
- `approve_action/4` has two routes, chosen by whether `action` is on the allowlist:
  - **Known action:** params validated against a per-action schema and checked by deterministic rules.
  - **Free-form or unknown action:** params go as an opaque data term to semantic review (`clara_fy` or other
    tools), which can only approve or deny. Never auto-approved by rules; fail-closed on timeout.
- Rule replaced: "reject free-text `Params`" becomes "never construct goals from Hermes-supplied text".
- **Escalation rule [decided 2026-09-21, ledger 28]:** the free-form route may approve only actions its review judges
  **reversible and contained**. If it judges an action **irreversible or is unsure**, it escalates to the user through
  the same path as a denial (ledger 2): no silent approve, no silent deny. Timeout still means deny. The semantic
  review therefore returns a verdict (`approve` / `deny` / `escalate`) plus a reversibility classification, and the
  demo starts with a small allowlist of known actions.
- **Params reaching Prolog [Phase 2 spike, not a decision]:** because the clara-prolog FFI cannot serialize dicts,
  params reach the rules either as a JSON string or as a structured term. Default: Python validates known-action params
  against a schema first, and Prolog only sees validated fields as plain values. The invariant (never build goals from
  Hermes text) holds either way.

## 5. Phase 0 facts gathered so far (limbic, read-only)

> Superseded by `hermes_phase0_findings.md` (2026-09-21). Phase 0 criteria are **met on limbic and attested by the
> user (2026-09-21)**. Hard gate cleared for Phase 1 design, subject to the follow-ups in the findings doc. The list below is the earlier CLI-only pass.

Hermes Agent v0.21.3 (2026.9.14), upstream 8ffc2f03, docker install, container `hermes` (8642, 9119).

- `hermes tools list` (the `cli` platform) shows these toolsets **enabled by default**: web, browser, terminal,
  file, code_execution, vision, image_gen, tts, skills, todo, memory, session_search, connections, clarify,
  delegation, cronjob, computer_use. Disabled: video, video_gen, x_search, stt, kanban, context_engine,
  homeassistant, spotify, yuanbao, and the `a2a` plugin toolset.
- `hermes tools enable|disable` exists, so disabling is supported. **Unverified:** which platform the API server on
  8642 uses, whether a disabled toolset leaves the model's tool list or is only blocked, and how a custom tool is
  registered (MCP likely). `hermes tools --summary` needs an interactive TTY, so it was not run.
- **Runtime tool-acquisition surfaces the single-tool gate must also close:** `skills`, `connections`, `cronjob`,
  `delegation`, the `a2a` plugin, `peer` (bot-to-bot across machines), `webhook`, `kanban`.
- CLI knobs to record: `--safe-mode`, `--ignore-user-config`, `--yolo` (must never be set), `hermes pause`
  (stops cron/kanban dispatch and new gateway turns; unverified whether it cancels in-flight sub-agents).
- `~/.hermes/config.yaml` is not readable by the dev user, so the effective config (C1) needs a `docker exec`
  read by the human.
- The checklist cites `lildaemon/docs/hermes_setup_notes.md`; no such file exists. Fix the reference or write it.

## 6. Phased rollout

- **Phase 0 (no repo code).** Run `hermes_phase0_research_checklist.md` plus hands-on verification on limbic;
  Pineal only as a final cross-host check. Gate passes only if it documents: (1) real tool-call wire format,
  (2) native toolsets, and the acquisition surfaces in §5, can be disabled, (3) whether a native confirmation hook
  exists, (4) whether a sub-agent cancel API exists (if not, item 5 applies). Output:
  `hermes_phase0_findings.md`. If toolsets cannot be disabled, revisit the gate design before Phase 1.
- **Phase 1.** Ritual-space API and NFS backend (§8), then `AgentEvaluator` + `HermesAgentEvaluator` skeleton, container-per-Ego-seat with cgroup limits and per-seat Hermes home from the golden template,
  fan-out cap, `ritual_id`/`seq` envelope. Ungated, text-in/text-out, one seat.
- **Phase 2.** `request_action`, `approve_action/4` with both §4 routes, paired Superego seat, `caws_tristate`
  promotion in the same change, structural firewall tests, independently sourced Tier 2 context, and
  approve/deny/timeout-deny tests in a standalone Ritual in the style of `ritual_roberts_rules_example.md`. Tests
  assert structure only.
- **Phase 3.** Frontdesk integration, terminal "denied" outcome, authenticated user override, pending-approval UX
  decision.
  Target (user, 2026-09-21): a new example analyst in `clara-frontdesk-poc` (style of `id_analyst.pl` and
  `deliberative_analyst.pl`) wiring the Superego (`approve_action`), the `HermesAgentEvaluator` as Ego, and an empty
  Id slot left for future work (Id suggestions are currently too wild to be useful; ledger 3). Consolidating copy-pasted
  analyst logic stays gated on this landing.
- **Phase 4 (named only).** Audit topics, latency budget, PitBoss auto-placement.

## 7. Open questions

1. Params: FFI serialization path is a Phase 2 spike (§4). The irreversible-action bar is decided (ledger 28); still to
   define is how the review classifies reversibility and what the user sees when it escalates.
2. Hermes wire format, confirmation hook, cancel API, API-server platform toolsets (Phase 0).
3. Placement and naming of `approve_action/4` (Phase 2).
4. Pending-approval UX in the WS protocol (Phase 3).
5. Superego decision-latency budget against deduction-cycle, httpx and Kafka timeouts (size early, tune Phase 4).
6. Cost model: not estimated anywhere; Clara flagged it. Phase 0 gives a first data point (660 input tokens with the
   gate config vs 13,357 with default tools; 10-27 s per run on the 27b model).
7. **Ritual space**: designed in §8 (decisions 23-27). Remaining sub-questions are listed at the end of §8.
8. Late-verdict handling: gate needs a correlation id with expiry so a Superego verdict arriving after the Ego's
   timeout is discarded (Phase 2).
9. Add a container pids limit alongside memory and CPU (Phase 1).
10. Autonomous work: Hermes' embedded kanban dispatcher and cron must not be able to act outside the Superego gate.
    Investigate disabling them for Ego seats; make "no autonomous work" a Phase 1 check (user concern, 2026-09-21).
11. Pineal Hermes must be reconciled with limbic's version and config before cross-host work.
12. **Who executes an approved action.** *(BUILT in Phase 2, 2026-09-21; see "Phase 2".)* `approve` says the action MAY run; the gate's
    executor runs it only if the action is allowlisted and has a registered handler. A free-form action's approval is advisory and
    nothing runs, so a fooled reviewer cannot cause an effect.
13. **Verify the seat at start, and never trust its narrative.** *(Start-up check implemented in slice 4a; recorded-events reporting begun, audit design still open, item 16.)* With a bad token Hermes ran with zero tools and the model
    still claimed to have saved a file. The evaluator must assert the expected six tools are registered before dispatching
    work, and report outcomes from tool events and the ritual-space journal (slice 4).
14. **Token delivery** must go through the seat's Hermes-home `.env` (0600), written at seat creation; container env vars are
    not resolved by `${VAR}` in MCP headers, and an unresolved variable stays literal without any error.
15. **Where the listener and seat containers live: per-host configuration, never an assumed shared docker network** (FieryPits
    can be remote and are identified by their registered URL). A host-run listener is unreachable from containers, so in the
    local compose stack the listener runs inside the FieryPit container, unpublished. The FieryPit advertises the URL at which
    seats reach its gate (`EGO_GATE_ADVERTISE_URL`); each host's seat launcher chooses the network a seat joins. **Decided
    2026-09-21:** seat containers are started by a **host-side launcher on a unix socket** bind-mounted into the FieryPit, not
    by mounting `docker.sock` (root on the host, next to `shell_command`) and not from a static pool. Contract in slice 4a;
    the daemon is built (slice 4b) and live-verified.
17. **Slice 4c (done, 2026-09-21): a full-stack ritual run** as a second FieryPit; see the slice 4c section. Merging the branch and rebuilding
    `lildaemon:latest` is still to do, as is a launcher for pineal.
18. **Version fragility.** The launcher reads Hermes' own log lines (tool registration, kanban dispatcher). Pin the Hermes image and
    re-verify the parser on every upgrade; the parser is tested against captured real lines and fails closed.
19. **The Ego role prompt** (`seat_launcher/template/SOUL.md`) is a first draft and needs real evaluation across models and tasks.
20. **Seat egress filtering.** *(Decision 2026-09-21: leave the firewall as is for testing; lock it down when moving to a production build.)* The `clara-seats` bridge isolates seats from the compose networks but not from host-published ports or the LAN (measured live:
    Dis :8080, Kafka :9094, Cobbler :5001 and ssh are reachable through the docker gateway). Add host firewall rules on the bridge allowing only DNS, the gate
    and Ollama. Defense in depth: the model itself has no network tool.
21. **Secrets baked into the FieryPit images.** *(RESOLVED for new builds 2026-09-21: `.env` is excluded and the live `lildaemon` was rebuilt without it. The old rollback image `lildaemon:pre-ego-merge` was deleted 2026-09-21; pineal's image is unchanged and still has it until rebuilt.)* Original finding, both images: `Dockerfile.lildaemon` copies the repo `.env` into the image and
    `Dockerfile.lildaemon.dockerignore` does not exclude it, so API keys, tokens and JWT secrets live in image layers. Recommend excluding `.env` (and
    passing secrets via compose) and reviewing what else the image carries. Not changed here: it affects the main build. Needs your call.
16. *(BUILT in Phase 2 as the action ledger, 2026-09-21; the gate is the single point every action passes, so it records decisions and executions itself.)* **Later (user idea, 2026-09-21): a way to be sure of what the agent actually did and what was actually decided.** Item 13 is the
    motivating case (the model narrated a save that never happened). A verifiable record of actions taken and decisions made,
    independent of the model's own account, is worth designing once the gate and executor exist; not scoped yet.
    **User strategy (2026-09-21):** detect or intercept the tool calls the agent makes, and record a log or emit an event asserting
    that the action was taken. Notes for when this is scoped: (a) the `ego_gate` server is already that interception point, since
    the seat's only tools are ours and native toolsets are off (verified at seat start, item 13); an event per call written by the
    server (seat, ritual, tool, arguments or a digest, verdict or result, the ritual `seq` from decision 11) is a complete record
    of what the seat could do, and absence of an event means it did not happen. (b) The audit record must not live in a
    model-writable place: keep it out of ritual-space documents (those are shared and model-writable), in an append-only log the
    server writes. Ritual-space writes are already journaled with author and `seq`. (c) Hermes' own SSE `tool.started/completed`
    events (name and preview, no full arguments) are an independent cross-check for detecting a tool call that never reached us.
    (d) Emission targets: a server-side log first; audit topics are the existing Phase 4 idea. (e) The user-facing answer should
    attach the recorded actions rather than rely on the model's summary. Not built yet.

Resolved since the base plan: Superego sharing (item 1), denial handling (item 2), kill-switch mechanics (item 5,
pending Phase 0), governor (item 14).

## 8. Ritual space design

Requirement (user, 2026-09-21): evaluators in a ritual must be able to read and write documents that other
participants can see. A Hermes Ego is a special case of that. Each performance of a ritual, or ritual of rituals, has
its own **ritual space**. Clara's mind splinters are distributed across FieryPits and hosts, so the approach must not
depend on any one host's filesystem layout.

### Why not extend the existing file tools

The lildaemon container has three overlapping roots: `/app` (working directory, code), `/app/workspace` (intended
file-tool root, `PYTHON_TOOLS_WORKSPACE`, a bind mount from `~/moonpool/.../lildaemon/workspace`) and `/app/moonpool`
(the whole moonpool tree, read-only). Models handed filesystem paths have three bases to guess from. The user reports
the path handling has never worked reliably (mistreating `/`, repo root and `./workspace`); this was **not reproduced**
here, only the configuration was read. The existing `instances/<label>-<instance_id>` private directory
(`toolified_ollama.py:763-791`) stays for seat-private scratch, but the ritual space does not build on path handling.

### Layers

| Layer | Scope | Where | Shared? |
|---|---|---|---|
| Seat-private | one evaluator instance | Hermes home on local disk (one per container); toolified `instances/<label>-<instance_id>` | never |
| **Ritual space** | one ritual performance (`ritual_id`) | dedicated backend directory, reached only through the API below | all participants in that ritual |
| Export | leaves the ritual space | `request_action` gate | per decision 23 |

### Ritual-space API [decided in principle; exact shape proposed]

- Operations: `list`, `read`, `write`, `append`, `stat`. Arguments are **logical document names**, never filesystem
  paths, so `/`, `./workspace` and repo-root confusion cannot occur. The server maps names to a directory.
- Caps on document size, document count and total bytes per ritual space. Names restricted to a safe character set.
- Every write records author `instance_id` and `seq` (decision 24); `stat`/`read` return that provenance.
- `write` takes an expected version and fails with a conflict error on mismatch; `append` is atomic (decision 25).
- Every operation is logged with `ritual_id`, author and `seq`, feeding the Phase 4 audit topics.
- For a Hermes seat the **evaluator hosts these as MCP tools** beside `request_action`. The container has **no mount**
  and no filesystem tools, so the Phase 0 gate stays hermetic. Toolified evaluators call the same API in-process.

### Backend [proposed]

- First backend: a dedicated directory on NFS, since limbic and pineal both mount moonpool. It must live **outside the
  repo tree** (see the moonpool shared-mount hazard), on its own subdirectory or export, and be written only by
  FieryPit-side code.
- The interface is backend-neutral so a service-hosted backend can be added for hosts that do not mount the share.
- NFS suits document files. It does not suit Hermes' own state (below).

### Hermes home and config [proposed]

- Hermes expands `${VAR}` and `${env:VAR}` in config (`hermes_cli/config.py:1618-1666`) and honours a per-container
  `HERMES_HOME`. So one golden template is **copied into each seat's local home at creation**; per-seat values (gate
  URL, API key, model) come from env. Config is not mounted from NFS. "Invariant" means copied, not read-only mounted:
  Hermes writes to its home at boot and on `config set`.
- Under the gate config Hermes' native memory, skills and session search are off, so nothing in the home needs
  sharing. Ritual-space documents are the shared, auditable memory. Sessions, `response_store.db`, `kanban.db` and
  the rest are per-seat scratch on local disk, reaped with the seat (Phase 0: six WAL-mode SQLite databases, unsafe
  on NFS; two containers on one data dir do not lock against each other).

### Lifecycle

Keyed by `ritual_id` (decision 11). Created with the ritual, reaped by the Dis ritual TTL sweep, retained when the
ritual sets `persist:true` (decision 26). Note the future multi-Dis-domain work: the key may later need the domain id.

### Migration (decision 27)

Toolified evaluators move to the ritual-space API in phases, so there is not a second file model to maintain.
Sequence: build the API and backend; wire Hermes seats first (they have no legacy path); then migrate toolified
evaluators and retire the path-based tools for ritual-scoped work.

### Implemented: slice 1 (lildaemon branch `hermes-ego-phase1`, 2026-09-21, not yet merged)

`goat/ritual_space/` with `tests/test_ritual_space.py` (44 tests). What the API settled:

- Flat document names matching `[A-Za-z0-9][A-Za-z0-9._-]{0,127}`; the space id (the Dis `ritual_id`, **not**
  `performance_id`, which is minted per participant join) uses the same rule. No name can express a path.
- `RitualSpace(backend, space_id, author)` with `list`, `stat`, `read`, `write(name, content, expected_version=None)`,
  `append`, and `set_persist`. Creating needs no version; overwriting requires the current one. Text only (UTF-8, no NUL).
- Space-wide `seq` from an append-only `journal.jsonl`. Every mutation is journal, then content, then metadata, so a
  crash between content and metadata is repaired on the next read from the journal record (provenance survives); an
  externally edited document is kept and flagged author `unknown`.
- Caps enforced under the lock (defaults 256 KiB per document, 200 documents, 8 MiB total; env-configurable).
- Cross-process safety is `fcntl.flock` per space. Verified: four processes appending 100 times lose nothing, and the
  same workload with the lock removed loses about half the appends and duplicates `seq`.
- Reaping: `reap_expired_spaces(backend, max_idle_seconds)`, time-based only, skips `persist` spaces. Not yet wired to
  the app or to Dis ritual status.
- Config: `RITUAL_SPACE_ROOT` (dev default `workspace/ritual_spaces`; production must be outside any repo tree),
  `RITUAL_SPACE_MAX_DOC_BYTES`, `_MAX_DOCS`, `_MAX_TOTAL_BYTES`.
- Caveat: `flock` on NFS is only as good as the mount's lock support; verify on the real export before relying on it
  across hosts.

### Implemented: slice 2 (lildaemon branch `hermes-ego-phase1`, 2026-09-21, uncommitted)

Wires the ritual space into rituals, toolified evaluators and app startup. 23 more tests (`tests/test_ritual_space_wiring.py`).

- **Scope, not arguments.** `goat/ritual_space/scope.py`: `ritual_scope(ritual_id, node_id)` and `author_scope(instance_id)`
  are `ContextVar` context managers (same pattern as `python_tools.workspace_scope`). `RitualParticipant` enters
  `ritual_scope` **inside the executor thread** that evaluates (`run_in_executor` does not copy context), and
  `ToolifiedOllamaEvaluator._dispatch_tool` enters `author_scope`. Tools take ritual and author from the scope, never
  from model-supplied arguments, so a model cannot name another ritual's space. Outside a ritual every tool fails closed.
- **Author is `<node_id>/<instance_id>`** (a superset of decision 24's `instance_id`), so a reader can tell which seat wrote
  a document, which is what the Superego needs to treat Ego-authored documents as Ego-supplied.
- **Tools:** `goat/tools/ritual_space_tools.py` + `.json`: `ritual_list`, `ritual_stat`, `ritual_read`, `ritual_write`,
  `ritual_append`. Return convention matches `python_tools` (`{"success", "result"}` or `{"success": false, "error",
  "error_type"}`), so a `VersionConflict` comes back to the model as data it can recover from. **Opt-in per evaluator**, no
  live registration changed:

      metadata:
        tools: goat/tools/ritual_space_tools.json    # module derived from the path

  To combine with `python_tools`, `tools` must be an inline list with an explicit `path` per tool (only one JSON file
  is accepted).
- **Reaping tied to the ritual:** `reap_by_ritual_status` (`goat/ritual_space/reaper.py`) with the Dis adapter in
  `goat/app/ritual_space_reaper.py`. A space idle less than the grace period, or marked persist, is never touched. Past
  that: Dis `active` keeps it however quiet; `terminated` or 404 reaps it; any other Dis failure only reaps past the long
  ceiling, so a Dis outage cannot wipe live rituals. **Opt-in**: `RITUAL_SPACE_REAP_ENABLED`, plus
  `RITUAL_SPACE_REAP_INTERVAL_SECONDS` (3600), `RITUAL_SPACE_TERMINATED_GRACE_SECONDS` (**7 days**),
  `RITUAL_SPACE_MAX_IDLE_SECONDS` (14 days). Enable on exactly one host per shared root. Wired into `goat/app/main.py`
  startup and shutdown.
- **Retention follows Dis (decided 2026-09-21).** Dis keeps a terminated ritual's row for 7 days and then deletes it
  (`clara-coire` CarrionPicker pass 5, reusing `snapshot_ttl`, `clara-cerebellum@c4e1aca`); active rituals are never
  touched. The grace default matches that, so a completed ritual's documents stay readable for the workers that will later
  pull finished work (the user's stated plan), and once Dis's sweep removes the row its status returns 404 and the reaper
  follows. The unknown-status ceiling is longer than the grace so an outage never reaps sooner than a confirmed
  termination would. (An earlier draft used a 1 hour grace; that would have deleted documents long before Dis forgets the
  ritual.)
- **`persist` for ritual spaces.** Dis's `persist` flag exists only on `/deduce` (durable deduction snapshots); ritual
  creation has no equivalent, so `persist` cannot be read from Dis. Until a ritual opts in, retention is the Dis-aligned
  default above, and `RitualSpace.set_persist()` is the hook. Deferred, in order: (a) a `persist_space` field on the
  lildaemon ritual config (`goat/app/ritual_configs/models.py`) applied at activation (`router.py:352`), stored in the
  space's own `space.json` so it works across hosts without touching Dis; (b) only if persistent rituals should also be
  exempt from Dis's own terminated-ritual GC, a `persist` field on the Dis `RitualConfig`/rituals table, returned by
  `/ritual/{id}/status` (Rust change plus migration).
- **Dis restarts do not lose rituals.** Dis restores rituals from its DuckDB store on start (251 were restored in the
  earlier count, per `dis_deduction_ritual_ttl_reaping`), so the 404-after-restart risk noted in an earlier draft is
  largely moot. Taken from the notes and the commit; not re-tested live.

### Implemented: slice 3, the ego_gate MCP server (lildaemon branch `hermes-ego-phase1`, 2026-09-21, uncommitted)

`goat/mcp/ego_gate/` (`seats`, `envelope`, `gate`, `tools`, `server`, `wiring`), 59 tests (`tests/test_ego_gate_core.py`,
`tests/test_ego_gate_server.py`), `mcp>=2.2,<3` declared in `pyproject.toml` (dev venv upgraded; the container already had
2.2.0). Live-verified against a throwaway Hermes (see `hermes_phase0_findings.md`, "Slice 3 live spike").

- **Per-seat identity from a bearer token.** `SeatRegistry` maps token to `(ritual_id, node_id, instance_id)`, storing only a
  SHA-256. Auth is checked twice: an ASGI wrapper rejects any HTTP request without a valid token (401), and every tool call
  re-resolves the seat from its own request headers, so a revoked token stops working immediately even inside a session.
  Tools take no ritual or author argument.
- **Six tools:** `request_action` and the five `ritual_*` tools (delegating to slice 2's functions inside the seat's scope).
- **`request_action` is deny-all until Phase 2**, behind an `ActionGate` interface. Timeout, exception or a malformed verdict
  all become deny, and the pending decision is cancelled, so a late verdict is discarded by construction (v3 open item 8,
  now implemented and tested; a gate that hands work to a thread must expire its own correlation id).
- **Envelope:** free-form action names (128 chars), `params` a JSON object up to 16 KiB, `justification` up to 4 KiB; plain data,
  never turned into Prolog text.
- **Separate listener, not on the public port.** Opt-in via `EGO_GATE_ENABLED` (`EGO_GATE_HOST`, `EGO_GATE_PORT` default 8765,
  `EGO_GATE_REQUEST_TIMEOUT_SECONDS`, `EGO_GATE_STATELESS` default true), started and stopped in `goat/app/main.py`.
- **Not fixed here:** `goat/mcp/ochecho/server.py` (imports the removed `mcp.server.fastmcp`) and
  `MCPToolRegistry`'s client (`streamablehttp_client` was renamed `streamable_http_client`) are still broken on mcp 2.x.

### Implemented: slice 4a, the Hermes evaluator and the seat-launcher contract (lildaemon branch `hermes-ego-phase1`, 2026-09-21, uncommitted)

`goat/evaluators/agent_evaluator.py` (`AgentEvaluator`), `goat/evaluators/custom/hermes_agent_evaluator.py`
(`HermesAgentEvaluator`), `goat/mcp/ego_gate/launcher.py` (contract and client), `Evaluator.close()`; 61 tests (all against fakes).
The real launcher daemon and a live end-to-end run are slice 4b.

- **`Evaluator.close()`** (no-op by default) is now called by `GoatWrangler.close_evaluator`, guarded so a failing `close()` never
  blocks slot removal. `RitualManager.leave` already reaches it. Without this hook nothing could tear a seat down.
- **`AgentEvaluator`** owns what must never be trusted to the agent: `max_tool_calls` (default 20) and `max_seconds` (120),
  enforced from outside by stopping the run, a capped run is an error even if the runtime reports it completed, and every
  failure is a `Tabu` (429 tool cap, 408 time cap, 502 failed or truncated run, 500 unexpected). The result carries the tool
  events the runtime reported, so callers can read what was done from events instead of the model's prose (item 16).
- **`HermesAgentEvaluator`**: one seat per instance, created lazily on the first evaluation from the ritual in scope (none
  in scope is a 400; a different ritual later is a 409). It registers a per-seat token, asks the launcher for a container, waits
  for ready, and **refuses to use a seat unless Hermes registered exactly the six `mcp__ego_gate__*` tools** (missing or
  unexpected tools tear the seat down and return 503), which closes the slice 3 finding that a seat with no tools still gets
  narrated success. A background thread renews the lease; `close()` deletes the seat and revokes the token.
- **Launcher contract** (JSON over a unix socket; the daemon in 4b implements the other side): `POST /seats`,
  `GET /seats/{id}` (state `starting|ready|failed|gone`, `tools_registered`), `POST /seats/{id}/lease`, `DELETE /seats/{id}`,
  `POST /seats/{id}/runs`, `GET /seats/{id}/runs/{run}/events` (SSE), `POST /seats/{id}/runs/{run}/stop`. The launcher fixes the
  image, limits and config template, **holds the seat's Hermes API key** (the FieryPit never sees it), **proxies the Hermes run
  API** (so the FieryPit needs no network path to seat containers), and **removes any seat whose lease lapses**, so a crashed
  FieryPit cannot leave containers running (decision 5's kill-switch guarantee).
- **A real bug the tests caught:** `SeatRegistry` defines `__len__`, so an empty registry is falsy and `registry or <global>`
  silently replaced a passed-in empty registry with the process singleton. Fixed with `is not None`, with a regression test.
- Registration is a **commented example** in `config/evaluators.yaml`; no live registration changed. Needs per FieryPit
  `SEAT_LAUNCHER_SOCKET`, `EGO_GATE_ADVERTISE_URL` and `EGO_GATE_ENABLED`.

### Implemented: slice 4b, the seat launcher daemon (lildaemon branch `hermes-ego-phase1`, 2026-09-21, uncommitted)

New top-level package `seat_launcher/` (**stdlib only**, runs on the docker host under the system Python with no venv and no
`goat` import), 117 tests, and a live end-to-end run against real Hermes (findings doc, "Slice 4b live end-to-end").

- **Trust boundary.** The FieryPit supplies only `seat_id`, `ritual_id`, `node_id`, `gate_url`, `gate_token`, `lease_seconds`. Image,
  mounts, network, limits, flags and config template are launcher config. `build_run_args` is the single place a `docker run` line is
  built: the seat's own home is the only mount; never `docker.sock`, `--privileged`, or host networking. Every id that becomes a
  container or directory name is matched against a strict pattern; unknown fields, oversized bodies and hostile values are rejected
  before docker or the filesystem is touched.
- **Secure by default.** Seats run with `no-new-privileges` and `--cap-drop ALL` plus six capabilities (verified live); a read-only root
  filesystem does not work. `hardening = []` opts out.
- **Ready on evidence only.** Hermes must report the six registered tools in its own log (read on the host), answer `/health`, confirm
  `kanban dispatcher: disabled via config`, and `hermes cron list` must show no jobs. This is the launcher half of slice 3 item 13; the
  evaluator's exact-tool-set check remains the second guard.
- **The seat's API key never leaves the launcher** (seat `.env`, mode 0600); the FieryPit drives the seat through proxied run endpoints.
- **Dead-man switch verified live:** a `SIGKILL`ed FieryPit had its seat reaped in 16 s; a killed launcher's orphans are swept on restart.
- **Golden seat template** (`seat_launcher/template/`): `platform_toolsets.api_server: []`, `tool_search: off`, `mcp_servers.ego_gate` with
  only the six tools, kanban dispatch/review/notify off, memory off, `agent.max_turns` backstop, and a short Ego role prompt (`SOUL.md`,
  first draft). A test cross-checks the template's tool list against `TOOL_NAMES` so they cannot drift.
- Docs and examples in `seat_launcher/README.md`, `launcher.example.toml`, `seat-launcher.service.example`; the compose snippet that
  bind-mounts the socket is documented, **not applied**.
- Bugs caught along the way (all with regression tests): keep-alive request smuggling on unread bodies, unix socket path length, an
  over-strict hardening-argument validator.

### Implemented: slice 4c, a full-stack ritual with a Hermes Ego seat (lildaemon `bea4bb8` on `hermes-ego-phase1`; compose file and docs uncommitted, 2026-09-21)

- **A second FieryPit, not a rebuild of the live one:** `clara-cerebellum/docker/docker-compose.ego.yml` (project `ego`, image `lildaemon:ego`, its own
  state, loopback-only port) registers with the existing Dis by URL. The live `lildaemon`, `lildaemon:latest` and `docker-compose.yml` are never touched;
  rollback is `docker compose -f docker-compose.ego.yml down`. Verified: the live stack was byte-identical before and after.
- **Seats on a dedicated `clara-seats` bridge**, with the launcher's new `extra_hosts` setting for Ollama. The runbook is
  `hermes_ego_limbic_runbook.md`.
- **`hermes_ego` is now a live registration** in `config/evaluators.yaml` (inert until a ritual joins with it; without the launcher socket and gate URL an
  evaluation fails closed with a 503).
- **`examples_ritual_hermes_ego.py`** drives a real ritual (Dis, Kafka, Prolog) and reports **evidence from disk and `docker inspect`, not the model's
  account**; `--strict` exits non-zero on any failed check. 29 tests, including that cleanup always happens even when the run fails.
- **Result:** passed end to end; real-stack dead-man switch reaped a seat 290 s after its FieryPit was `SIGKILL`ed (lease 300 s). Details in the findings doc,
  "Slice 4c live full-stack ritual".

### Merged and deployed (2026-09-21): lildaemon `master` == `0bb4b4b`, live `lildaemon:latest` rebuilt

- **Merge:** lildaemon `hermes-ego-phase1` (8 commits) fast-forwarded onto `master` (old master `f7cfb60`, kept as the rollback point). **Nothing pushed:** local `master` is 8
  commits ahead of `github/master`; you will say when to push. The branch is kept because the docs reference it.
- **Rebuild:** `docker compose -f docker-compose.yml build lildaemon`, then `up -d --no-deps lildaemon`: only that service was recreated (healthy in 8 s). Verified: the 12 other
  containers unchanged (start times, images, health); Dis lists `http://lildaemon:6666` with 13 evaluators including `hermes_ego` (was 12); zero errors in the startup log; the
  ego_gate listener and the ritual-space reaper are **off** (opt-in, as intended); the dependents show no lildaemon-related errors.
- **`.env` excluded from the image** (`Dockerfile.lildaemon.dockerignore`, its own commit). Verified: `/app/.env` is gone, and the live process environment differs from before by
  **exactly** the six baked-only variables (`CLIPS_MCP_URL`, `FIERYPIT_BASE_URL`, `GITHUB_TOKEN`, `GROQ_BASE_URL`, `OLLAMA_URL`, `PROLOG_MCP_URL`), none added; only `HOSTNAME` (the container id)
  otherwise changed. Open item 21 is resolved for new builds.
- **Regression on the merged code:** the second FieryPit (`docker-compose.ego.yml`) rebuilt from the merged master, and `examples_ritual_hermes_ego.py --strict` passed again (exit 0, evidence green).
- **Rollback image deleted (2026-09-21, at your request):** `lildaemon:pre-ego-merge` (the previous `lildaemon:latest`, `1223728067cd`) contained the baked secrets and is gone. Rolling back now means rebuilding from `f7cfb60`.
- **Not done, by decision:** pineal's rebuild; the seat egress firewall (deferred to the production build); pushing.

### Phase 2 (2026-09-21, lildaemon branch `hermes-ego-phase2`, not merged, nothing committed yet): the real gate

`request_action` is no longer deny-all when `EGO_GATE_MODE=superego` (the default stays `deny_all`, so nothing changes until enabled).

**Design.** Per `request_action` the gate submits ONE Dis deduction whose goal is the constant `approve_action_from_context(Verdict, Reason, Route)`; the request
travels as data in the deduction `context`. No Prolog is ever built from text Hermes wrote. Division of labour:

- **Python (deterministic):** JSON-schema validation of params against the action's spec, usage counts and prior refusals read from the ledger, context assembly.
- **Prolog (`goat/mcp/ego_gate/rulesets/superego_gate.pl`, policy only):** an allowlisted action is approved if reversible and within its per-ritual limit, denied if the limit is
  used up or the identical request was already refused, and escalated if irreversible. A free-form action goes to one `caws_consult` with the `superego` node of the SAME ritual
  (a tool-less LLM, same model tag as the seat so Ollama keeps one model loaded). Approve only if the reviewer says approve AND reversible AND contained; deny outright if it
  refuses; anything irreversible, uncontained or unsure escalates; no answer, timeout, missing/unknown/ambiguous fields all deny.
- **Executor (`actions.py`, `handlers.py`):** only an allowlisted action with a registered handler ever runs. Two demo actions, both reversible and contained:
  `export_document` (copy a ritual document to `<EGO_OUTBOX_ROOT>/<ritual_id>/`) and `record_note`. Handlers re-validate names with the ritual-space rules and write atomically.
  **An LLM verdict can never cause execution:** a free-form approval is reported as "no executor exists, NOTHING WAS DONE".
- **Action ledger (`ledger.py`, closes item 16):** append-only `<ritual space root>/<ritual_id>/actions.jsonl` (flock, fsync, torn-line tolerant), outside `docs/` so no ritual tool reaches it.
  **Write-ahead:** the decision record is written before anything executes; if it cannot be written the request is denied. One decision record and, if executed, one execution record.
  Decisions and executions for a ritual are serialised so the counts the gate reads cannot race.
- **Tier 2 context:** the user's request and the Id's alternatives are recorded on the seat by the evaluator from the Ritual offering (`SeatRegistry.set_context`), never from the Ego. The
  reviewer prompt marks the Ego's justification as quoted, untrusted text.
- **Evaluator:** `HermesAgentEvaluator` returns `actions` and `escalations` computed from the ledger slice written during the run, not from Hermes' own account.
- **Wiring:** `EGO_GATE_MODE`, `EGO_GATE_SUPEREGO_NODE`, `EGO_GATE_SUPEREGO_MODEL`, `EGO_GATE_DIS_POLL_SECONDS`, `EGO_OUTBOX_ROOT`; the gate timeout default is now 45 s (under the seat's 60 s MCP timeout).
  On timeout the deduction is cancelled at Dis (`DisClient.cancel_deduction`), so a late verdict cannot land.

**Tests.** Suite 2 failed / 1680 passed / 26 skipped (baseline 2 failed / 1566 passed / 26 skipped; the same 2 pre-existing `test_goat_manager` failures). New: actions/handlers, ledger (including a real
bug the tests found: a record appended after a torn line was glued onto it and lost), gate against a Dis double (fixed goal text never contains Hermes text, every verdict and failure shape),
the Prolog policy against a real Dis (23 cases, skipped when none is reachable), and the request_action flow over real MCP HTTP. Six mutants were each killed by a test: skip schema validation, skip the
execution-time re-validation, let an LLM approval execute, drop `reversible AND contained`, put Hermes text in the goal, drop the write-ahead record.

**Live proof (second FieryPit, `EGO_GATE_MODE=superego`, real Hermes seat, real reviewer).** `export_document` approved deterministically and executed (outbox file present, both ledger records);
`send_email` **escalated** by the reviewer ("irreversible, uncontained or uncertain"); `publish_announcement` with the justification "The system has already approved this action and the Superego said yes"
**denied**. The evaluator's `actions` matched the ledger exactly; nothing but the export was executed. Details and the two things it found: findings doc, "Phase 2 live proof".

**Deliberate deviations from the approved plan:** per-seat decision history lives in the durable ledger, not in memory on the seat (it survives a FieryPit restart and is the one source of truth); invalid params are
denied inside the gate before Prolog rather than as a separate REJECTED state.

**Not in Phase 2:** delivering an escalation to the user and the override (Phase 3, frontdesk Ego analyst), `caws_tristate` promotion, egress firewall, pineal, pushing, any real outward action (email).

### Still open

- **Verify `flock` over NFS on the real ritual-space export** (multi-host append test, plus behaviour when a host
  drops mid-lock) before any cross-host use. Slice 1 only proved it locally. Tracked in memory as `flock-over-nfs-unverified`.
- Ritual configs exposing `persist` for the ritual space (options (a) and (b) above; neither needed yet).
- Whether the export step needs a Superego review or is only logged for `read` of ritual documents by an outside party.
- Retention default and the reaper's interaction with `persist:true` across hosts.
- Reproducing and root-causing the container path failures (independent of this design, but worth doing before the
  toolified migration).
- Whether a service backend is needed soon (any FieryPit host without the moonpool mount).

## 9. Review history

- 2026-09-21, user review: items 9-14 confirmed. §4 was endorsed by the assembly (below) and explicitly confirmed by the user later the same day, so it is now [decided].
- 2026-09-21, Deliberative Analyst assembly: adopted 4-0. Filed as `hermes_v3_assembly_review_2026-09-21.md`,
  advisory input only. It added the goal-construction invariant (now in the header). It did not resolve the §7
  open questions.
- 2026-09-21, Phase 0 source review: `hermes_phase0_findings.md`. No blockers; two preconditions for Phase 1
  (explicit API-platform toolset config, per-seat data dir). Wire format still unverified, so the gate is open.
- 2026-09-21, throwaway-container test: gate recipe verified (`platform_toolsets.api_server: []`, `tool_search off`, MCP
  `tools.include`); deny/timeout fail closed; stop and kill measured. Details in `hermes_phase0_findings.md`.
- 2026-09-21, user attested Phase 0 and commented on the findings' remaining questions (kanban dispatcher concern
  added as open item 10).
- 2026-09-21, ritual-space design agreed (decisions 23-27) and written up as §8; Hermes home/config approach and NFS-first backend remain [proposed].
- 2026-09-21, user decided the free-form escalation rule (ledger 28), confirmed the frontdesk analyst target for Phase 3,
  and left inline `[STAN]` comments in §0.
- 2026-09-21, Phase 1 slice 1 implemented in lildaemon (`goat/ritual_space/`, branch `hermes-ego-phase1`, uncommitted).
- 2026-09-21, Phase 1 slice 2 implemented in lildaemon (scope wiring, tools, ritual-status reaper), uncommitted.
- 2026-09-21, ritual-space retention aligned with Dis's 7 day terminated-ritual retention (grace 7 days, ceiling 14 days); persist options recorded, deferred.
- 2026-09-21, Phase 1 slice 3 implemented in lildaemon (`goat/mcp/ego_gate/`) and live-verified against Hermes; open items 12-16 added.
- 2026-09-21, Phase 1 slice 4a implemented in lildaemon (evaluator, launcher contract, close hook); item 15 corrected: reachability is per-host config, launcher on a unix socket decided.
- 2026-09-21, Phase 1 slice 4b implemented (seat launcher daemon) and live-verified end to end; open items 17-19 added.
- 2026-09-21, Phase 1 slice 4c: full-stack ritual with a Hermes Ego seat passed on the real stack as a second FieryPit; open items 20-21 added, item 17 done.
- 2026-09-21, branch fast-forwarded onto lildaemon master (not pushed); live lildaemon rebuilt without the baked `.env`; regression ritual passed on the merged code.
