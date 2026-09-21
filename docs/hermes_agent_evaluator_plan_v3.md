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
> Hermes output is parsed and bound as data only. (Recorded in the 2026-09-21 assembly review; see §8.)

> **Note:** The Freudian labels (Id, Ego, Superego) are mnemonics only. They impose no behavioral constraint
> beyond each seat's functional spec.

Status tags: **[decided]** the user made the call (items 9-14 originated with Clara and were confirmed
2026-09-21). **[proposed]** new in v3, needs review. **[open]** unresolved. **[rejected]** dropped, with
the reason.

## 0. Needs your decision first

1. ~~Params handling (§4).~~ Confirmed by the user 2026-09-21. The two sub-questions in §4 remain open.
2. **Phase 0 runs on limbic** (§6). The earlier docs said it needs Pineal. It does not, except for a final
   cross-host confirmation.
3. ~~Whether the adopted items in §1 are confirmed.~~ Items 9-14 confirmed by the user on 2026-09-21.

## 1. Decision ledger

| # | Item | Status | Source |
|---|---|---|---|
| 1 | One Superego seat per Ego seat, joined and left together | decided | addendum 1 §2 |
| 2 | Denial (or `caws_await` timeout) escalates to the user; no silent retry, no loop to Id | decided | addendum 1 §3 |
| 3 | Id stays a stateless pipeline stage; no Id quality pass in this work | decided | addendum 1 §4 |
| 4 | Native Hermes toolsets disabled is a hard Phase 0 gate | decided | addendum 2 §2.1 |
| 5 | One Hermes container per Ego seat, hard-stopped by the evaluator, cgroup-bounded, unless Phase 0 finds a real cancel API | decided | addendum 2 §2.2 |
| 6 | `caws_offer`/`caws_await` is the gate; timeout-to-deny is the enforcement; no Kafka control plane | decided | plan §3 |
| 7 | `request_action(action, params, justification)` is the only side-effecting tool | decided | plan §3 |
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
- **[open]** Should the free-form route require a stricter bar for irreversible action classes (semantic approval
  plus user confirm)? Does the known clara-prolog FFI dict-serialization gap force the data term to be a JSON
  string blob parsed Python-side?

## 5. Phase 0 facts gathered so far (limbic, read-only)

> Superseded by `hermes_phase0_findings.md` (2026-09-21, source review). Phase 0 is **not yet attested**: the exact
> tool-call wire format needs one live run. The list below is the earlier CLI-only pass.

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
- **Phase 1.** `AgentEvaluator` + `HermesAgentEvaluator` skeleton, container-per-Ego-seat with cgroup limits,
  fan-out cap, `ritual_id`/`seq` envelope. Ungated, text-in/text-out, one seat.
- **Phase 2.** `request_action`, `approve_action/4` with both §4 routes, paired Superego seat, `caws_tristate`
  promotion in the same change, structural firewall tests, independently sourced Tier 2 context, and
  approve/deny/timeout-deny tests in a standalone Ritual in the style of `ritual_roberts_rules_example.md`. Tests
  assert structure only.
- **Phase 3.** Frontdesk integration, terminal "denied" outcome, authenticated user override, pending-approval UX
  decision.
- **Phase 4 (named only).** Audit topics, latency budget, PitBoss auto-placement.

## 7. Open questions

1. Params: stricter bar for irreversible free-form actions; FFI serialization path (§4).
2. Hermes wire format, confirmation hook, cancel API, API-server platform toolsets (Phase 0).
3. Placement and naming of `approve_action/4` (Phase 2).
4. Pending-approval UX in the WS protocol (Phase 3).
5. Superego decision-latency budget against deduction-cycle, httpx and Kafka timeouts (size early, tune Phase 4).
6. Cost model: not estimated anywhere; Clara flagged it.

Resolved since the base plan: Superego sharing (item 1), denial handling (item 2), kill-switch mechanics (item 5,
pending Phase 0), governor (item 14).

## 8. Review history

- 2026-09-21, user review: items 9-14 confirmed. §4 was endorsed by the assembly (below) and explicitly confirmed by the user later the same day, so it is now [decided].
- 2026-09-21, Deliberative Analyst assembly: adopted 4-0. Filed as `hermes_v3_assembly_review_2026-09-21.md`,
  advisory input only. It added the goal-construction invariant (now in the header). It did not resolve the §7
  open questions.
- 2026-09-21, Phase 0 source review: `hermes_phase0_findings.md`. No blockers; two preconditions for Phase 1
  (explicit API-platform toolset config, per-seat data dir). Wire format still unverified, so the gate is open.
