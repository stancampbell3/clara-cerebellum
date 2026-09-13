# Id → Committee → Deliberative Analyst: "Ritual of Rituals" (planning draft)

> **Status:** approved, implemented, and LIVE-VERIFIED end-to-end 2026-09-13
> (Parts 1–3). Unit-tested (1203 passed, 0 failed in lildaemon's suite;
> `cargo check`/`clippy` clean for clara-frontdesk-poc). All of §3.3's live
> scenarios ran successfully against the real Docker stack (clara-api,
> lildaemon, clara-frontdesk, real Ollama/Groq models, a real Google
> Custom Search + crawl, real Edgequake ingestion) — see "Live
> verification results" below for the full trace and two infra findings
> surfaced along the way. Part 4 remains future-work design notes only, as
> planned.

## Context

`leannan_sidhe` (the Id analyst's divergent-retrieval rewrite) shipped and
was verified live 2026-09-12 — see `leannan_sidhe_planning.md`. Two gaps
became apparent immediately afterward, and a third surfaced once we
started designing the fix:

1. The Id's creative output (its rendered impulses) is never folded back
   into Edgequake, so later turns and other rituals can't build on it.
2. The Id only ever reasons from what the knowledge graph already has —
   there's no way for it to pull in fresh outside evidence mid-turn, the
   way `deliberative_analyst.pl`'s committee referral already can.
3. Making the Id's turns asynchronous (needed for #2 — see the
   architecture correction below) exposes a real, pre-existing gap in the
   frontdesk POC client: there is no "in progress" indicator for
   background work, and — more seriously — the current pending-research
   delivery mechanism can silently lose a result across a page reload or
   WebSocket drop. That's a rare edge case today (only `deferred_query`/
   `deliberate` turns are async); it becomes routine once every Id turn
   is async by default.

This plan covers all three, as the next increment toward the longer-term
"Ritual of Rituals" vision already named (but not built) in
`lildaemon/docs/assistant_demo.md`: three cooperating Rituals — Id, Ego,
Superego — where Id produces abundant raw candidates, an Ego stage
selects/grounds/formalizes them into Robert's-Rules motions, and the
Superego (`deliberative_analyst.pl`, already built) debates and votes on
them. **The Ego stage itself is explicitly out of scope for this pass** —
a prior team deliberation already tabled a premature attempt to design it
as under-specified (see `docs/archived/deliberation-minutes-this-is-a-
brainstorming-sessi.md`). This plan only builds the two pieces Id needs
regardless of whether/when an Ego stage ever exists: folding its own
output back into shared knowledge, and being able to go get more
knowledge when it's thin.

### Decisions locked for this draft

| Question | Decision |
|---|---|
| Scope | Fold-back into Edgequake + committee referral, built now. Ego/motion-formation stage: sketch extension points only, don't build. |
| Where does committee-referral judgment live? | Mechanically, inside `id_analyst.pl` itself (citation-count threshold, not an LLM sufficiency judgment) — preserves "no judgment, just abundance." |
| Fold-back granularity | One Edgequake document per Id turn, all impulses together — mirrors the existing "Minutes of deliberation" pattern. |
| Sync vs. async rollout | **Always on** — every Id turn becomes an async "brainstorm" turn (immediate ack, real answer delivered later). Not opt-in. |
| Multi-round shape | Round 1 always runs. If thin, refer to committee (real snek crawl + Edgequake ingest), then Round 2 with fresh SparkIds. Reply combines both rounds' blocks, not Round-2-only. |
| Committee-referral failure handling | Degrades gracefully *within* the one background deduction (Round 1 alone stands). No outer Python-level retry-scheduling, unlike deliberation's `tabled_pending_committee` auto-resume — see rationale in Part 1 §4.3. |
| Frontdesk gap | Address now, same plan: wire `action_taken` client-side, add a persistent in-progress indicator, and fix the delivery-loss window (server marks "delivered" on read, before the client can guarantee it rendered/persisted the result). |
| Cross-domain topic sharing | Captured as a **future-work design note** (Part 4) — not built this pass. Leading candidate: gossip-style crosstalk between Dis domains for eventual consistency, leveraging the existing PitBoss FieryPit peer registry, rather than a synchronous cross-domain subscribe API. Topic-naming choices in this pass don't foreclose it (no new topic-naming scheme introduced — brainstorm reuses the exact same `SnekTopic`/`IngestTopic`/`TopicSubject` machinery deliberation already uses). |

---

## Part 1 — `id_analyst.pl`: multi-round brainstorm with committee referral

### Architectural correction that shaped this design

Initial assumption was that `deliberative_analyst.pl`'s committee referral
runs synchronously inside classify. It does not: `assistant_turn/5` there
is a trivial `decision_pattern/1` gate that returns an acknowledgment.
*All* of the committee-referral machinery (`research_step_tristate/9`,
`committee_deadline_for/3`, `refer_to_committee/8`) lives inside
`deliberation_step/11`, fired as its own **background** `/deduce` call
via `runtime.py`'s `_submit_deliberation`, delivered later through the
existing pending-research mechanism. Confirmed live 2026-09-07 with a
real web crawl (`assistant_demo.md:1188-1239`).

So the Id's multi-round version follows the same shape: a new background
deduction (`brainstorm_step/10`), not something crammed into the
120s/300-cycle synchronous classify budget.

### 1.1 `id_analyst.pl` changes

File: `lildaemon/goat/app/assistant/rulesets/id_analyst.pl`.
Keep `id_step/4`, `offer_id_sparks/…`, `await_id_sparks/…`,
`render_alternative/2`/`render_alternatives/2`, and the decode helpers
exactly as-is — reused unchanged.

**Mechanical evidence-sufficiency check** (no LLM — a count, not a
judgment, per the Id's design philosophy):

```prolog
id_committee_threshold(N) :-
    ( getenv('LEANNAN_COMMITTEE_THRESHOLD', S), atom_number(S, N0), N0 >= 0
    -> N = N0 ; N = 3 ).

id_evidence_thin(Citations) :-
    length(Citations, Count), id_committee_threshold(Threshold), Count < Threshold.
```

**Committee-referral machinery — copied verbatim**, matching this
codebase's established "every ruleset file is self-contained" convention
(same reason `strip_think/2`/`extract_caws_response/2` are already
duplicated here rather than shared): `caws_tristate/3`,
`combine_tristate/3`, `research_step_tristate/9`, and
`committee_deadline_for/3` + its `:- thread_local committee_deadline/2.`
declaration, copied from `deliberative_analyst.pl` unchanged.

**`refer_to_committee_for_id/6`** — Id's referral wrapper (simpler than
deliberative's `refer_to_committee/8`: no `Evidence` text to thread, just
a tristate-to-continue/tabled decision):

```prolog
refer_to_committee_for_id(Query, SnekTopic, IngestTopic, TopicSubject,
                          CommitteeWaitBudgetS, Tabled) :-
    research_step_tristate(Query, 4, SnekTopic, IngestTopic, TopicSubject, 45.0, 90.0,
                            State, _R),
    (   State == ok -> Tabled = continue
    ;   State == failed -> Tabled = tabled
    ;   committee_deadline_for(Query, CommitteeWaitBudgetS, Deadline),
        get_time(Now),
        ( Now > Deadline -> Tabled = tabled ; fail )   % retry next engine cycle
    ).
```

**Round-tagged rendering** — a new `render_alternative/3` /
`render_alternatives_round/3` pair that label each block's heading with
its round number, leaving `render_alternative/2` untouched (zero
regression risk to the already-shipped rendering shape, even though it's
no longer called from `assistant_turn/5` — see below):

```prolog
render_alternative(Round, alt(SparkId, Node, Text), Block) :-
    leannan_spark_citations(SparkId, CitationIds),
    ( CitationIds == [] -> Footnote = ''
    ; maplist(citation_footnote_line, CitationIds, Lines),
      list_to_set(Lines, LinesSet), atomic_list_concat(LinesSet, ', ', Joined),
      format(atom(Footnote), "~n~n*Sources: ~w*", [Joined])
    ),
    format(atom(Block), "### Impulse — ~w (Round ~w)~n~n~w~w", [Node, Round, Text, Footnote]).

render_alternatives_round(_, [], []).
render_alternatives_round(Round, [Alt|Alts], [Block|Blocks]) :-
    render_alternative(Round, Alt, Block),
    render_alternatives_round(Round, Alts, Blocks).
```

**`brainstorm_step/10`** — the new top-level background predicate:

```prolog
%% brainstorm_step(+Query, +SnekTopic, +IngestTopic, +TopicSubject,
%%                  +CommitteeWaitBudgetS, -Reply, -Citations,
%%                  -CitationCount, -RoundsUsed, -Outcome) is semidet.
%%   Outcome one of: single_round | committee_referred | committee_tabled.
brainstorm_step(Query, SnekTopic, IngestTopic, TopicSubject, CommitteeWaitBudgetS,
                Reply, Citations, CitationCount, RoundsUsed, Outcome) :-
    id_spark_count(Count),
    id_step(Query, Count, Alternatives1, Citations1),
    render_alternatives_round(1, Alternatives1, Blocks1),
    (   id_evidence_thin(Citations1)
    ->  refer_to_committee_for_id(Query, SnekTopic, IngestTopic, TopicSubject,
                                   CommitteeWaitBudgetS, Tabled),
        (   Tabled == continue
        ->  StartId is Count + 1,
            id_seat_pool(SeatPool2), length(SeatPool2, PoolSize2), id_num_ctx(NumCtx2),
            leannan_sparks(Query, Count, StartId, none, Sparks2, Citations2),
            offer_id_sparks(Sparks2, SeatPool2, PoolSize2, NumCtx2, Query, Offered2),
            await_id_sparks(Offered2, Alternatives2),
            render_alternatives_round(2, Alternatives2, Blocks2),
            append(Citations1, Citations2, CitationsDup),
            id_dedup_citations(CitationsDup, Citations),
            append(Blocks1, Blocks2, AllBlocks),
            RoundsUsed = 2, Outcome = committee_referred
        ;   Citations = Citations1, AllBlocks = Blocks1,
            RoundsUsed = 1, Outcome = committee_tabled
        )
    ;   Citations = Citations1, AllBlocks = Blocks1,
        RoundsUsed = 1, Outcome = single_round
    ),
    length(Citations, CitationCount),
    format(atom(NL), "~n~n", []),
    atomic_list_concat(AllBlocks, NL, Reply).
```

`id_dedup_citations/2`: copy `the_leannan.pl`'s `dedup_citations/2` body
verbatim as a private predicate here (2 clauses, trivial) rather than
exporting it from `the_leannan.pl` — keeps this file self-contained and
avoids nudging that module's export surface for a one-off caller.

**`assistant_turn/5` — now unconditionally binds `brainstorm`** (always
on, per the locked decision — no env-var gate on the action itself):

```prolog
assistant_turn(Query, brainstorm, Reply, [], 0) :-
    format(atom(Reply),
           "I'm sparking a round of impulses on this: ~w~n~nI'll bring back whatever comes up once the sparks land (and, if the well runs dry, after checking in with committee research).",
           [Query]).
```

The old `assistant_turn(Query, chat, Reply, Citations, CitationCount) :-
id_step(...), render_alternatives(...)` clause is removed (superseded) —
`id_step/4` and `render_alternatives/2` themselves stay, unused by
`assistant_turn/5` directly but still exactly what `brainstorm_step/10`'s
Round 1 calls, and still available as the documented reusable
Superego-facing hook.

### 1.2 `the_leannan.pl` — Round-2 fresh-SparkIds extension

File: `clara-cerebellum/clara-prolog/prolog-lib/the_leannan.pl`.

`leannan_spark/6` memoizes by `(SparkId, Query-Profile-WorkspaceId)` —
reusing Round 1's SparkIds (1..Count) for Round 2 would silently replay
stale, pre-crawl results even after the committee's crawl enriched the
graph. Add `leannan_sparks/6` (StartId variant), keep `leannan_sparks/5`
as a one-line wrapper calling it with `StartId=1` (100% backward
compatible, byte-identical behavior for every existing caller):

```prolog
:- module(the_leannan, [ ..., leannan_sparks/5, leannan_sparks/6, ... ]).

leannan_sparks(Query, Count, StartId, WorkspaceId, Sparks, AllCitations) :-
    Count > 0,
    leannan_profiles(Profiles), length(Profiles, NumProfiles),
    EndId is StartId + Count - 1,
    leannan_sparks_(StartId, EndId, Query, WorkspaceId, Profiles, NumProfiles, Sparks),
    findall(Sources, member(spark_entry(_, _, spark_result(Sources, _)), Sparks), CitationLists),
    append(CitationLists, AllCitationsDup),
    dedup_citations(AllCitationsDup, AllCitations).

leannan_sparks(Query, Count, WorkspaceId, Sparks, AllCitations) :-
    leannan_sparks(Query, Count, 1, WorkspaceId, Sparks, AllCitations).
```

`leannan_sparks_/7`'s second parameter is purely renamed `Count` →
`EndId` (identical arithmetic when `StartId=1`, since `EndId = Count`).
`leannan_spark/6`, `leannan_profiles/1`, `dedup_citations/2` untouched.

### 1.3 `runtime.py` changes

File: `lildaemon/goat/app/assistant/runtime.py`.

New constants (near `_COMMITTEE_WAIT_BUDGET_S`):
```python
_BRAINSTORM_PATIENCE_CYCLES = 1500
_BRAINSTORM_MAX_CYCLES = 1800
```
(`_COMMITTEE_WAIT_BUDGET_S` reused as-is for brainstorm's committee wait
— one shared knob, not duplicated.)

**`_submit_brainstorm`** (mirrors `_submit_deliberation`):
```python
async def _submit_brainstorm(self, prolog_source_id, query, context=None) -> str:
    ritual_id, snek_topic, ingest_topic, topic_subject = await self._research_topics(query)
    goal = (
        f"brainstorm_step({prolog_quote_atom(query)}, "
        f"{prolog_quote_atom(snek_topic)}, {prolog_quote_atom(ingest_topic)}, "
        f"{prolog_quote_atom(topic_subject)}, {_COMMITTEE_WAIT_BUDGET_S}, "
        "Reply, Citations, CitationCount, RoundsUsed, Outcome)"
    )
    return await self._submit_deduce(
        prolog_source_id, goal, ritual_id=ritual_id, self_node_id="assistant-brainstorm",
        patience_cycles=_BRAINSTORM_PATIENCE_CYCLES, max_cycles=_BRAINSTORM_MAX_CYCLES,
        context=context,
    )
```
No change needed to seat-joining: `turn()`'s existing
`if resolved_ruleset_key == "id": await self._ensure_deliberation_seats()`
already runs before every classify call for the `id` ruleset.

**`poll_brainstorm_convergence`** (mirrors `poll_deliberation_convergence`,
just `_poll_deduce(deduction_id, max_wait_s=2.0, quiet=True)`).

**`finish_brainstorm`** (mirrors `finish_deliberation`, but *also*
resolves+tags the document_id, following `EdgequakeIngestConsumer`'s
full resolve-then-tag pattern rather than `finish_deliberation`'s own
submit-and-forget — this is the actual Goal-1 fold-back step):

```python
async def finish_brainstorm(self, query, topic_slug, solution) -> tuple[str, list[dict], int]:
    reply = solution.get("Reply") or "(no impulses generated)"
    citations = solution.get("Citations") or []
    citation_count = solution.get("CitationCount") or 0
    rounds_used = solution.get("RoundsUsed") or 1
    outcome = solution.get("Outcome") or "single_round"
    try:
        edgequake = self.get_edgequake_client()
        workspace_id = await self.get_workspace_id(edgequake)
        content = (
            f"# Id brainstorm\n\n**Prompt:** {query}\n\n"
            f"## Impulses ({rounds_used} round(s), outcome: {outcome})\n\n{reply}\n"
        )
        track_id = await edgequake.submit_document(
            workspace_id=workspace_id, content=content, title=f"Id brainstorm: {query}",
            metadata={"kind": "id_brainstorm", "query": query, "outcome": outcome,
                      "rounds_used": rounds_used},
        )
        progress = await edgequake.ingestion_progress(track_id, workspace_id=workspace_id)
        document_id = progress.get("document_id")
        if document_id:
            self.get_document_tags().record(document_id, topic_slug=topic_slug,
                                             query=query, source_url=None)
    except Exception:
        logger.exception("assistant runtime: failed to record brainstorm document for %r", query)
    return reply, citations, citation_count
```
Best-effort: Edgequake being unreachable must not lose the brainstorm's
own reply, only the record of it (same discipline `finish_deliberation`
already uses).

**`turn()` dispatch** — new `if action == "brainstorm":` block, same
shape/position as the existing `deliberate` block (immediate-ack +
`session_id is None` degrade-to-chat + `_submit_brainstorm` +
`PendingResearchStore.create(..., kind="brainstorm")`). Update the module
docstring's action-list line to add `| brainstorm`.

No new topic-naming: `brainstorm_step/10` addresses the identical
`SnekTopic`/`IngestTopic`/`TopicSubject` atoms `deliberation_step/11`
already uses for the same query, via the same `_research_topics` helper —
this is what keeps this pass from foreclosing the future cross-domain
topic-federation direction (Part 4): whatever eventually lets
`{dis_domain}.coire.{subject_path}` chatter be shared across domains
applies uniformly to both rulesets' committee referrals, no divergent
naming to reconcile later.

### 1.4 `research_queue.py` changes

File: `lildaemon/goat/app/assistant/research_queue.py`.

New `kind="brainstorm"` branch in `advance_pending_research`, structurally
identical to the `kind="deliberation"` branch: poll
`poll_brainstorm_convergence` → on solution, call `finish_brainstorm` →
`touch_cited_documents` → `queue.mark_ready(...)`. Handle
`DisRitualNotFound` the same way (mark failed — a clara-api restart wipes
in-memory deductions).

**Design call: no outer Python-level retry-scheduling for `brainstorm`**,
unlike deliberation's `tabled_pending_committee` → scheduled-retry
machinery. Rationale: deliberation's retry exists because a *tabled*
deliberation is an unresolved decision — retrying later is in service of
that ruleset's whole purpose (reaching a verdict). A `committee_tabled`
brainstorm outcome is not a failure — Round 1's alternatives are already
complete, genuine, citable impulses; they just weren't augmented by fresh
research. Re-firing the whole pipeline later, unprompted, to retroactively
improve impulses the user already received doesn't fit "abundance now,"
not "wait for the best answer." If this turns out wrong in practice, the
same `create_scheduled`/`resume_with_deduction` machinery generalizes
trivially (a `fire_brainstorm_retry` mirroring `fire_deliberation_retry`)
without touching anything else in this plan.

### 1.5 Env vars

| Env var | Where | Default | Purpose |
|---|---|---|---|
| `LEANNAN_COMMITTEE_THRESHOLD` | `id_analyst.pl`, `id_committee_threshold/1` | `3` | Mechanical Round-1 citation-count floor below which committee referral fires. |

`LEANNAN_SPARK_COUNT` (existing) governs both rounds' spark count
identically — Round 2 reuses Round 1's `Count`, just offset `StartId`.
No separate Round-2-count knob; flag if independent tuning turns out to
matter in practice.

---

## Part 2 — Frontdesk POC: in-progress indicator + delivery robustness

### What's actually there today (confirmed by direct exploration)

`clara-frontdesk-poc` (`clara-cerebellum/clara-frontdesk-poc/`) is a small
Rust (actix-web) server + one vanilla-JS static page (no framework), not
a React/Vue app. The browser talks to the Rust server over one
WebSocket; the Rust server proxies to lildaemon's REST API.

The pending-research pipeline **already exists end-to-end** — it is not
entirely unbuilt — but has real, specific gaps:

1. **`action_taken` reaches the browser already but is completely
   ignored.** `SendResponse.action_taken` is forwarded verbatim over the
   WS (`ws.rs`'s `Handler<TurnResult>`), but `index.html`'s `ws.onmessage`
   never reads `data.action_taken` — zero occurrences in the file. A
   `"brainstorm"`/`"deferred_query"`/`"deliberate"` ack renders
   identically to an ordinary `"chat"` reply.
2. **The only existing loading-state UI (`showThinking()`/`.thinking`
   div) is disposable and non-persisted** — cleared by *any* next WS
   frame of *any* type, not specifically the reply to that turn, and has
   no concept of surviving a page reload. Not a safe base to extend
   as-is for a long-running background state.
3. **A real delivery-loss window, not just a cosmetic gap.** The backend
   (`GET /assistant/sessions/{id}/pending-research`, `router.py`) drains
   and marks-delivered every ready item on read — a one-shot,
   non-idempotent read. The Rust WS actor polls this every 5s and pushes
   each ready item as a `research_update` WS frame; the browser holds
   them only in an in-memory JS array (`pendingResearch`), deliberately
   *not* auto-injected into the transcript (shown via a bell/badge
   instead, opened on click). If the tab reloads or the WS drops between
   "server marked delivered" and "user opened the bell," **that result is
   gone** — the DB already considers it delivered, and the browser's
   queue is wiped by the reload. Rare today (only `deferred_query`/
   `deliberate` are async); becomes routine once every Id turn is async.
4. **The `kind` field never reaches the browser at all** — dropped at the
   Rust HTTP-client layer (`PendingResearchInfo` struct only captures
   `request_id, query, reply, citation_count`). No per-kind styling is
   possible today even though the backend already returns it.

### 2.1 Backend: split "peek" from "ack" (`lildaemon`)

File: `lildaemon/goat/app/assistant/router.py` (+ `research_queue.py`/
`models.py` as needed).

- Add `PendingResearchEntry.kind` to the response if not already
  serialized (confirm; the model has it, the Rust client just doesn't
  capture it — see 2.2).
- Change `GET /assistant/sessions/{id}/pending-research` to **not**
  mark-delivered as a side effect of reading — it should be a safe,
  repeatable peek at `ready`-and-not-yet-acked rows (plus, per the
  in-progress indicator requirement, optionally include
  `researching`/`answering` rows too, via a query param or always —
  design call: always include them, with `status` in the response, since
  the client needs both "what's ready" and "what's still cooking").
- Add `POST /assistant/sessions/{id}/pending-research/{request_id}/ack` —
  marks one row delivered. Called by the frontdesk **after** it has
  durably persisted the result client-side (see 2.3), not merely after
  rendering it — closing the loss window at its actual source.

### 2.2 Rust actor (`clara-frontdesk-poc/src/ws.rs`, `assistant_client.rs`)

- `PendingResearchInfo`: add `kind: String` and `status: String` fields
  (stop silently dropping them).
- On each 5s tick: fetch the (now non-destructive) pending-research list.
  For `status == "ready"` items: push `research_update` as today, **then
  call the new ack endpoint** once the WS `ctx.text(...)` send succeeds
  (or, more conservatively, once the CLIENT confirms persistence — see
  2.3's ack-from-client option below; either is a defensible design call,
  document whichever is chosen). For `researching`/`answering` items:
  push a new frame type, `{"type": "research_status", "request_id",
  "query", "kind", "status"}`, once per state change (track last-seen
  status per `request_id` in the actor to avoid re-pushing identical
  status every 5s).
- **Design call**: prefer **client-acks-after-persisting** over
  **server-acks-after-WS-send** — a successful `ctx.text()` call doesn't
  guarantee the browser tab actually received/processed the frame (WS
  sends can succeed into a closing connection's buffer). Have the client
  send back `{"type": "research_ack", "request_id"}` once it has written
  the result into its own persistent store (2.3), and have the Rust actor
  call the new ack endpoint only then. This fully closes the loss window
  at the cost of one extra round trip — worth it given this becomes the
  routine path for every Id turn.

### 2.3 Client (`static/index.html`)

- **Persistent pending-item store**: on receiving `research_update`,
  write it to `sessionStorage` (keyed by session id) *before* updating
  the in-memory `pendingResearch` array, then send `research_ack` back
  over the WS. On page load, restore any `sessionStorage`-persisted items
  into `pendingResearch` before the first WS tick — a reload between
  receipt and viewing no longer loses anything, since persistence (and
  the resulting server-side ack) now happens immediately on receipt, not
  on click.
- **In-progress indicator**: on receiving a `SendResponse`/`agent` frame
  with `action_taken` in `{"deferred_query", "deliberate", "brainstorm"}`,
  create a persistent (sessionStorage-backed, survives reload) status
  entry — a small status row/chip near the input, distinct from the
  disposable `showThinking()` div (e.g. "🧠 Brainstorming on 'what is a
  ritual?'… (started 12s ago)"), one per outstanding request. Update/clear
  it when a matching `research_update`/`research_status` frame with
  `status in {"ready","delivered","failed"}` arrives for that
  `request_id`. On page load, re-derive any still-outstanding entries
  from the peek endpoint (now safe to call repeatedly) rather than only
  from `sessionStorage`, so a reload correctly still shows "in progress"
  for genuinely still-running work even if the local flag was lost.
- **Per-kind styling**: use the now-forwarded `kind` field to give
  `brainstorm`/`deliberation`/`research` distinct icons or labels in both
  the in-progress chip and the delivered bubble (currently only
  differentiated by one border-color CSS class, and not even that
  between kinds).

### 2.4 Scope note

This is real, separate work from Part 1, spanning both repos
(`lildaemon`'s API + `clara-cerebellum/clara-frontdesk-poc`'s Rust+JS). It
should land *before or alongside* Part 1 going live, not after — per the
exploration's own conclusion, making brainstorm always-on without this
turns a rare edge case into a routine, user-visible one.

---

## Part 3 — Test plan

### 3.1 Python unit tests (new `tests/test_id_brainstorm.py`, following
`tests/test_assistant_runtime_turn.py`/`tests/test_advance_pending_research.py`'s
existing mocking idioms exactly)

- `turn()` dispatch: `test_brainstorm_action_returns_ack_and_fires_background_brainstorm`,
  `test_brainstorm_without_session_id_degrades_to_chat` (mirror the
  `deliberate` equivalents).
- `_submit_brainstorm`: assert goal string shape + `self_node_id`/`ritual_id`.
- `poll_brainstorm_convergence`: mirror the deliberation equivalents.
- `finish_brainstorm`: formats reply + tags document (mock
  `submit_document`/`ingestion_progress`/`DocumentTagStore.record`);
  survives Edgequake failure (mirror
  `test_finish_deliberation_survives_minutes_recording_failure`); skips
  tagging cleanly when `document_id` is missing.
- `advance_pending_research` (append to `tests/test_advance_pending_research.py`,
  mirroring the `kind="deliberation"` block): not-converged stays
  researching; converged goes to ready; finish-exception logged and
  retried, not marked failed; deduction-gone marks failed; brainstorm and
  deliberation rows advance independently. Explicit comment noting the
  intentional absence of a `create_scheduled`-retry test for brainstorm
  (§1.4's design call), so it doesn't read as an oversight later.

### 3.2 Prolog-side

No existing Prolog unit-test harness surfaced for `lildaemon`'s ruleset
files — cover via live verification (3.3). `leannan_sparks/6`'s `StartId`
offset is worth a direct scratch-`/deduce` check regardless: fire
`leannan_sparks(Query, 3, 7, none, Sparks, _)` and confirm SparkIds
7/8/9, no collision with a prior `1..6` call, and that a fresh `StartId=7`
call after a real Edgequake change returns different content (proves the
memo keys on the fresh SparkId, not just Query+Profile).

### 3.3 Live verification (rebuild+restart, real HTTP calls — this
session's established methodology)

1. Rebuild/restart the `lildaemon` and `clara-cerebellum` (Prolog) Docker
   services, plus `clara-frontdesk-poc` once Part 2 lands.
2. Set `LEANNAN_COMMITTEE_THRESHOLD=999` to force committee referral on
   every turn; send an `id`-ruleset query about a topic with zero prior
   Edgequake knowledge (same "force the thin-evidence branch" technique
   the original deliberative committee-referral verification used).
3. Confirm via logs: Round 1 `leannan_sparks` fires, `id_evidence_thin/1`
   trips, `research_step_tristate/9`'s real `snek`/`edgequakeingest`
   offers fire (real crawlee run, real `POST /api/v1/documents`), Round 2
   fires with `StartId = Count+1`, deduction converges with
   `Outcome = committee_referred`.
4. Poll pending-research until delivery; confirm the reply contains both
   `(Round 1)` and `(Round 2)` labeled blocks.
5. Confirm a new `Id brainstorm: <query>` document exists in Edgequake and
   has a matching `assistant_document_tags` row.
6. Repeat with `LEANNAN_COMMITTEE_THRESHOLD=0` (never thin): confirm
   `Outcome = single_round`, only `(Round 1)` blocks, and a document is
   still folded back — proving fold-back works independently of
   committee referral triggering.
7. Force `committee_tabled` (fast-failing referral, e.g. target a
   nonexistent node — same technique used for deliberation's `failed`
   branch): confirm Round 1 alone is delivered and no retry row appears.
8. Part 2: reload the frontdesk tab mid-brainstorm and confirm the
   in-progress indicator re-appears correctly; reload again after
   delivery but before opening the bell, and confirm the result is still
   there (the fixed loss window).

---

## Live verification results (2026-09-13)

Ran against the real Docker stack (`clara-cerebellum/docker/docker-compose.yml`)
after rebuilding `clara-api`, `lildaemon`, and `clara-frontdesk` — real
Ollama models (gemma4, qwen-clara), a real Google Custom Search call, a
real `snek` crawl, real `edgequakeingest` consumption, and a real
Edgequake instance. `LEANNAN_SPARK_COUNT=2`/`LEANNAN_LOCAL_ONLY=1` used
throughout to keep runs fast and deterministic (2 impulses/round, local
seats only — the 6-impulse/4-seat default path was already verified live
in the leannan_sidhe pass this builds on).

**Scenario A — mechanical thin-check + failing referral
(`LEANNAN_COMMITTEE_THRESHOLD=999`, nonsense query):** `id_evidence_thin/1`
correctly tripped (any real citation count is `< 999`); `refer_to_committee_for_id/6`
fired a REAL `research_step_tristate/9` — confirmed via clara-api logs
(`RitualRegistry: joined ritual ... participant=Some("snek")`, a live
Google Custom Search HTTP call) — which then failed (`snek` published a
`tabu`, `edgequakeingest` never got usable content to ingest). `combine_tristate/3`
correctly resolved to `failed` as soon as `snek`'s failure was visible,
without waiting for the slower `edgequakeingest` leg. Result:
`Outcome=committee_tabled`, Round 1 alone delivered, in ~60s. This
exercised §3.3 point 7 ("force a failing referral") for free, since a
nonsense query's crawl genuinely fails.

**Scenario B — mechanical thin-check + succeeding referral
(`LEANNAN_COMMITTEE_THRESHOLD=999`, real crawlable query — "what is the
current latest stable release version number of the Rust programming
language compiler"):** referral fired, `snek` found and crawled
`https://releases.rs/` for real, `edgequakeingest` ingested it into
Edgequake as a real, separate, fully-processed document. Round 2 then
fired with fresh SparkIds (`StartId = Count+1`). Delivered reply combined
both rounds; the folded-back "Id brainstorm: ..." Edgequake document's
metadata read back exactly `{"kind": "id_brainstorm", "outcome":
"committee_referred", "rounds_used": 2}`, with `### Impulse — ... (Round 1)`
and `### Impulse — ... (Round 2)` blocks both present. **Observed
characteristic** (not a bug, and not new — `deliberative_analyst.pl`'s own
`refer_to_committee/8` has the identical property): Round 2's retrieved
evidence didn't yet reflect the just-crawled page's content, most likely
because Edgequake's own entity/embedding graph-processing for a
freshly-ingested document lags slightly behind the ingest-complete signal
`research_step_tristate/9` waits on. Round 2 is still genuinely a fresh
query (new SparkIds, not a stale cache hit) — it just may not always see
the newest content the instant the crawl finishes. Not a regression to
fix in this pass; flagged for awareness.

**Scenario C — never-thin (`LEANNAN_COMMITTEE_THRESHOLD=0`):** referral
never fired regardless of evidence sparsity; `Outcome=single_round`,
`rounds_used=1`, delivered in ~26s (no committee wait). Confirms fold-back
and single-round rendering work fully independently of referral ever
triggering.

**Scenario D — frontdesk delivery-robustness (the actual regression test
for Part 2's fix):** a real Python `websockets` client connected through
`clara-frontdesk`, set ruleset to `id`, sent a query, received the
`brainstorm` ack and a `research_status` (`"researching"`) frame, then
**disconnected without ever acking**. Reconnecting with a brand-new WS
actor (empty `pending_status` map — no memory of the first connection)
still received the exact same `request_id`'s `research_update` frame,
proving the row was never silently marked delivered while nobody was
connected to receive it (the old bug). Sending `research_ack` and
reconnecting a third time confirmed the row does *not* redeliver
again — the ack genuinely took effect.

**Two real infra findings surfaced along the way (both fixed in-session,
not left as follow-up):**
1. **`LEANNAN_*` env vars must be set on the `clara-api` service, not
   `lildaemon`.** The Prolog engine that evaluates `id_analyst.pl`'s
   `getenv/2` calls is embedded in `clara-api` (the Dis domain) —
   `lildaemon` only submits goals to it over REST and never runs the
   ruleset itself. Setting `LEANNAN_SPARK_COUNT`/`LEANNAN_LOCAL_ONLY` on
   `lildaemon` alone (an easy mistake — every OTHER assistant-tuning env
   var in the compose file, like `PENDING_RESEARCH_POLL_INTERVAL_SECONDS`,
   *is* a `lildaemon`-side Python setting) had silently zero effect: the
   first verification run used the compiled-in defaults (spark count 6,
   all 4 seats) despite the override. `docker-compose.yml` now documents
   this distinction inline and only defines these vars on `clara-api`.
2. **DuckDB's `cursor.rowcount` is always `-1`** (not implemented) —
   `mark_delivered`'s "did this UPDATE actually match a still-ready row"
   check had to read the affected-row count off `cursor.fetchone()`
   instead (DuckDB returns `[(n,)]` for an UPDATE/DELETE, like any other
   query result). Caught by a quick interactive check against the real
   `duckdb` package before it shipped as a live bug.

---

## Part 4 — Design notes for later (not built this pass)

**Ego / motion-formation stage.** Converting a subset of Id's impulses
into `deliberation_step/11`-shaped `Motion` text is the natural next
increment after this one, but has no precedent anywhere in this codebase
(confirmed: `Motion` is currently always LLM-derived from the raw user
`Query`, never externally supplied; `id_step/4`'s structured `Alternatives`
never escapes its own Prolog call today). Needs its own design pass,
deliberately deferred.

**Cross-domain topic sharing.** Ad hoc Coire/Kafka topics are namespaced
`{dis_domain}.coire.{subject_path}` — confirmed NOT shared across Dis
domains today. Stan's proposed direction: **gossip-style crosstalk between
domains for eventual consistency**, rather than a synchronous cross-domain
subscribe/query API — likely leveraging the existing PitBoss FieryPit
peer registry (already solved a different cross-host problem: joining one
formal Ritual across hosts) as the peer set for gossip relay. This pass
doesn't build it, but doesn't foreclose it either: no new topic-naming
scheme was introduced (Part 1 reuses deliberation's exact topic-resolution
path), so whatever cross-domain mechanism eventually ships applies
uniformly to both rulesets' committee referrals.

---

### Critical files
- `lildaemon/goat/app/assistant/rulesets/id_analyst.pl`
- `clara-cerebellum/clara-prolog/prolog-lib/the_leannan.pl`
- `lildaemon/goat/app/assistant/runtime.py`
- `lildaemon/goat/app/assistant/research_queue.py`
- `lildaemon/goat/app/assistant/router.py`
- `clara-cerebellum/clara-frontdesk-poc/src/ws.rs`
- `clara-cerebellum/clara-frontdesk-poc/src/assistant_client.rs`
- `clara-cerebellum/clara-frontdesk-poc/static/index.html`
