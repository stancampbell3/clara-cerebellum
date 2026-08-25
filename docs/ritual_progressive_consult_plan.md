# New Ritual example: local + Groq splinter + Snek, progressive-research style

> **Status: draft for team review.** Not yet implemented. Comments/edits
> welcome, especially on the "Related work" / Option 1 vs. Option 2 fork
> below — that's the one open design question worth a second opinion
> before building starts.

## Context

`lildaemon` already has three standalone worked examples that exercise
Dis/FieryPit/Ritual/Kafka directly (no browser UI involved), all living as
flat `examples_ritual_*.py` scripts at the lildaemon repo root, each with a
companion doc under `lildaemon/docs/`:

- `examples_ritual_snek_splinter.py` — 2-party: `snek` (web crawl) joined as
  a standing Ritual participant, `clara_mind_splinter` (local Ollama,
  `qwen-clara:latest`) doing a one-shot `/deduce` that calls
  `caws_offer(snek, ...)` / `caws_await(...)`.
- `examples_ritual_rumination_ingest.py` — 3-party: adds `edgequakeingest`
  (drains Snek's crawled pages into Edgequake via
  `EdgequakeClient.submit_document`) as a second standing participant.
- `examples_ritual_rumination_answer.py` — 2-phase: runs the ingest example,
  then a second one-shot deduction that asks Clara directly
  (`ponder_text/2`) and Edgequake (`ruminate_opts/3`), reconciling both.

Separately, the **assistant demo** (`clara-frontdesk-poc`) has a
`progressive_research.pl` ruleset (`lildaemon/goat/app/assistant/rulesets/`)
that tries increasingly expensive tiers before answering: local ponder →
Edgequake-grounded ponder → background web research, checking
"is this answer good enough?" at each tier via `clara_fy/2,3`
(`the_rat.pl`, a local LLM-backed yes/no classifier).

The goal is a new standalone example, modeled on the `examples_ritual_*`
scripts (not wired into the frontdesk UI), that combines both ideas: extend
progressive_research's tiered/stop-early design with a **second LLM
consultation tier** — the Groq-backed `clara_mind_splinter_groq` evaluator
(same `ClaraMindSplinter` class, just an OpenAI-compatible remote transport
to Groq's API, model `qwen/qwen3.6-27b`) — before falling back to Snek's
live web research. Both evaluators are already registered in
`config/evaluators.yaml`; no new evaluator config is needed.

Order of consultation, stopping at the first tier whose answer is judged
sufficient by the **local** Clara LLM (`clara_fy`):

1. Local LLM (`clara_mind_splinter`, qwen-clara:latest)
2. Local knowledge (Edgequake, via a new `cow` evaluator/Ritual participant —
   see "Team input: a `cow` evaluator" below; adopted after team review,
   replacing the original plan's direct in-engine
   `ruminate_and_assert_citations/3` call)
3. Remote LLM (`clara_mind_splinter_groq`)
4. Web research (`snek` + `edgequakeingest`, same as the existing 3-party example)

If tier 4 is reached, Snek's crawled pages get inserted into Edgequake by
`edgequakeingest` (the existing document-pipeline-insert path — there is no
cross-engine Prolog-assertion mechanism in this codebase; each `/deduce` is
an isolated SWI engine, so "feeding results back into the deduction" always
means either a local `assertz` after a message-passing reply, exactly like
`the_cow.pl`'s `citation/8`/`cites/2` facts already do, or a real document
insert via Edgequake — both already established here, reused as-is), then
Edgequake is re-queried once more so the freshly-ingested pages can inform
the final combined answer.

**Design correction, already incorporated below**: do *not* use
`enable_evaluator/2` (Prolog → `splinteredmind` tool →
`POST /evaluators/set`) to switch which evaluator answers mid-deduction.
Instead, treat the local splinter and the Groq splinter as **peer Ritual
participants**, exactly like `snek`/`edgequakeingest` already are — the
deduction sends them a message via `caws_offer/4` and blocks on
`caws_await/2` for an async reply. This avoids ever touching FieryPit's
global "currently focused evaluator," so `clara_fy`'s own local LLM calls
(which *do* run through the currently-focused evaluator/`ponder_text`) are
never at risk of accidentally running against Groq.

**Loop semantics**: this is a single Prolog goal inside a single `/deduce`
call, not a Python-level or Prolog-level re-submission loop. Each
`caws_offer`/`caws_await` leg can itself take many of Dis's internal
retry cycles to resolve (exactly as `edgequakeingest`'s idle-wait already
spans dozens of cycles in the existing 3-party example). Dis's own
`max_cycles`/`evaluator_patience_cycles` — set generously high here, since
this pipeline chains four sequential legs instead of one or two — *is* the
outer loop: it bounds how long the whole chain (all four tiers, in order,
stopping early on the first sufficient one) is allowed to keep retrying
before giving up, with no separate recursion or re-attempt of already-tried
tiers needed.

## Related work: `docs/ritual_multi_evaluator_plan.md`

While researching this, we found an existing plan in this same `docs/`
directory targeting almost exactly the Clara↔Groq leg of this design — and
confirmed (via grep) that it has since been **implemented**, not just
planned: `peer_consult` is threaded through `RitualManager.py`,
`RitualParticipant.py`, `GoatWrangler.eval_slot`,
`goat/app/ritual_configs/router.py`, and the plain `POST /ritual/join` REST
body (`goat/app/ritual/router.py:54`), with unit test coverage
(`tests/test_ritual_participant_addressing.py`,
`tests/test_ritual_config_router.py`).

**What it actually does** (read directly from `RitualParticipant.py`
lines 286-328): a node joined with `peer_consult=True` auto-wraps *every*
incoming Offering into an inner deduction running the fixed goal
`reasoned_response(Query, Context, Response)` against that node's own
registered `prolog_source_id`, with `ritual_id` set so the inner
deduction's Prolog can itself reach other peers over the same Ritual
topic — this is what lets one evaluator node autonomously consult a peer
evaluator mid-deduction, entirely inside its own nested deduce call,
without any `enable_evaluator`/global-focus juggling either (it solves
the same problem the design correction above solves, one layer lower).

**Correction after digging further** (see the full Option 2 sketch below):
`ritual_multi_evaluator_plan.md`'s own Prolog template calls
`coire_poll(ritual/hohi, Env)` — that predicate **does not exist**.
`the_coire.pl`'s full export list has no `coire_poll/2` at all, only
`caws_offer/4`, `caws_await/2`, `caws_consult/4`, `caws_emit/4`, etc. So
the inner deduction a `peer_consult=True` node runs would reach a peer the
*same way everything else in this codebase does* — `caws_offer`/
`caws_await` — just one level deeper (inside the nested deduction) rather
than at the top level. That means the "unconfirmed Hohi payload shape"
risk flagged below was overstated: the real primitive is the same
`caws_offer`/`caws_await` + `extract_hohi_response/2` pattern this plan
already uses and has proven twice, not the nonexistent call in that
doc's illustrative template.

**Why this plan still doesn't default to it**: `peer_consult`'s mechanism
runs a *fixed* goal name (`reasoned_response/3`) automatically — built for
"this node tries its own answer, and if insufficient, autonomously
consults exactly one named peer," a clean 2-step chain. Our ordering
requirement is a 3-step chain with a non-peer step in the middle
(local → **Edgequake (REST, not a peer)** → Groq), which needs a custom
`reasoned_response/3` body either way. The real, corrected costs (see the
sketch below) are: a second registered Prolog source, a genuine citation-
fidelity loss (per-citation detail can't cross the inner/outer engine
boundary, only a count), and a less transparent timeout budget (one
participant's `eval_timeout_s` has to cover the *entire* inner chain,
nested inside Dis's own deduce-completion polling). None of that is fatal
— it's a real design tradeoff for reviewers to weigh, not a risk-avoidance
call the way it was first framed.

**Worth reviewer input**: Option 2 (sketched in full below) would register
a *second* Prolog source for the `local-splinter` slot containing a custom
`reasoned_response/3` that internally sequences ponder → Edgequake → Groq
(via nested `caws_offer`/`caws_await`, `peer_consult=True`), reducing the
top-level orchestrator to just two hops (ask `local-splinter` once, then
`research_step/8` if still insufficient). That would also double as the
first live exercise of `peer_consult` — `ritual_multi_evaluator_plan.md`'s
own Part D (end-to-end verification) has never been run. Worth a team call
between Option 1's transparency/citation-fidelity and Option 2's tighter
orchestrator/dog-fooding value.

**Separate follow-up, not part of this deliverable**: `ritual_multi_evaluator_plan.md`
itself needs a documentation pass — it's written entirely in
future-tense/to-do voice ("the system **cannot** run this test today...
This plan closes that gap"), but Parts A and B are both confirmed
implemented (see greps above) and Part D (live verification) still appears
open. Filing this as its own cleanup item so `docs/` reflects current
system state rather than a stale to-do list.

## Option 2 sketch: `peer_consult`-based local-splinter

For reviewer comparison — not the proposed default. Two things worth
knowing before reading the code: node ids with hyphens need Prolog atom
quoting (`'local-splinter'`, `'groq-splinter'` — bare, `groq-splinter`
parses as `-(groq, splinter)`), and `reasoned_response/3`'s fixed 3-ary
signature forces citations to be smuggled back as a JSON-encoded
`{text, citation_count}` atom rather than bound directly, since each
`/deduce` is an isolated SWI engine and per-citation detail asserted
inside `local-splinter`'s own inner deduction never reaches the
orchestrator's engine.

**1. `local-splinter`'s registered Prolog source** (built and registered
once per run, before joining, containing the *entire* ponder → Edgequake →
Groq chain in one clause):

```python
def build_local_splinter_source(workspace_id: str, llm_provider: str, llm_model: str) -> str:
    return f"""
extract_hohi_response(Dict, Response) :-
    (   is_dict(Dict)
    ->  D = Dict
    ;   atom_string(A, Dict),
        atom_json_dict(A, D, [value_string_as(atom)])
    ),
    get_dict(hohi, D, D1),
    get_dict(response, D1, D2),
    (   get_dict(content, D2, Response)
    ->  true
    ;   get_dict(response, D2, Response)
    ).

sufficiency_check(Answer, Query, Ctx) :-
    format(atom(Q), "Does \\"~w\\" adequately answer the prompt: ~w?", [Answer, Query]),
    clara_fy(Q, Ctx, true).

%% Auto-invoked by RitualParticipant.py's peer_consult wrapping as:
%%   current_context(C), reasoned_response(Query, C, R).
reasoned_response(Query, Ctx, ResponseJson) :-
    % Tier 1: local ponder.
    ponder_text_with_context(Query, Ctx, PonderRaw),
    extract_hohi_response(PonderRaw, PonderAnswer),
    (   sufficiency_check(PonderAnswer, Query, Ctx)
    ->  dict_to_json(_{{text: PonderAnswer, citation_count: 0}}, ResponseJson)
    ;   % Tier 2: ground with Edgequake.
        (   catch(
                ruminate_and_assert_citations(Query,
                    _{{context: Ctx, mode: hybrid, workspace: '{workspace_id}',
                      llm_provider: '{llm_provider}', llm_model: '{llm_model}'}},
                    EdgeResult),
                _Error, fail)
        ->  ruminate_answer(EdgeResult, EdgeAnswer),
            ruminate_citations(EdgeResult, EdgeCites),
            length(EdgeCites, EdgeCiteCount),
            format(atom(CombinePrompt),
                   "Two candidate answers to '~w'. Combine into one short, coherent answer.~nA (own knowledge): ~w~nB (grounded): ~w",
                   [Query, PonderAnswer, EdgeAnswer]),
            ponder_text_with_context(CombinePrompt, Ctx, CombinedRaw),
            extract_hohi_response(CombinedRaw, EdgeCombined)
        ;   EdgeCombined = PonderAnswer, EdgeCiteCount = 0
        ),
        (   sufficiency_check(EdgeCombined, Query, Ctx)
        ->  dict_to_json(_{{text: EdgeCombined, citation_count: EdgeCiteCount}}, ResponseJson)
        ;   % Tier 3: escalate to the groq-splinter peer over the Ritual.
            catch(
                (   caws_offer('groq-splinter', "consult/groq",
                                _{{prompt: EdgeCombined, context: Ctx}}, GroqCid),
                    caws_await(GroqCid, GroqRaw),
                    extract_hohi_response(GroqRaw, GroqAnswer),
                    format(atom(FinalPrompt),
                           "Two candidate answers to '~w'. Reconcile into one best answer.~nA (local+grounded): ~w~nB (second opinion): ~w",
                           [Query, EdgeCombined, GroqAnswer]),
                    ponder_text_with_context(FinalPrompt, Ctx, FinalRaw),
                    extract_hohi_response(FinalRaw, FinalAnswer),
                    dict_to_json(_{{text: FinalAnswer, citation_count: EdgeCiteCount}}, ResponseJson)
                ),
                _GroqError,
                dict_to_json(_{{text: EdgeCombined, citation_count: EdgeCiteCount}}, ResponseJson)
            )
        )
    ).
"""
```

**2. Registration + join code** (replaces Option 1's plain `local-splinter`
join):

```python
LOCAL_SPLINTER_NODE_ID = "local-splinter"
GROQ_SPLINTER_NODE_ID = "groq-splinter"

def register_local_splinter_source(dis_base_url, workspace_id, llm_provider, llm_model):
    resp = requests.post(
        f"{dis_base_url}/source",
        json={
            "source_type": "prolog",
            "content": build_local_splinter_source(workspace_id, llm_provider, llm_model),
            "label": "local-splinter-reasoned-response",
        },
    )
    resp.raise_for_status()
    return resp.json()["source_id"]

# in run_demo(), after creating the ritual and BEFORE joining local-splinter:
source_id = register_local_splinter_source(dis_base_url, workspace_id, llm_provider, llm_model)

join_info = requests.get(f"{dis_base_url}/ritual/{ritual_id}/join",
                          params={"participant": LOCAL_SPLINTER_NODE_ID}).json()
requests.post(
    f"{fierypit_base_url}/ritual/join",
    json={
        "ritual_id": ritual_id,
        "topic": join_info["topic"],
        "bootstrap_servers": kafka_bootstrap,
        "dis_domain": join_info["dis_domain"],
        "evaluator": "clara_mind_splinter",
        "node_id": LOCAL_SPLINTER_NODE_ID,
        "self_node_id": LOCAL_SPLINTER_NODE_ID,
        # Must cover the WHOLE inner chain: ponder + Edgequake + a full
        # groq-splinter round trip + two more ponders. Size generously —
        # this single asyncio.wait_for is the one clock for all of tiers 1-3.
        "eval_timeout_s": 180.0,
        "prolog_source_id": source_id,
        "peer_consult": True,
    },
    headers=auth_headers,
)

# groq-splinter joins exactly like Option 1 — plain, no source, no peer_consult.
join_info_groq = requests.get(f"{dis_base_url}/ritual/{ritual_id}/join",
                               params={"participant": GROQ_SPLINTER_NODE_ID}).json()
requests.post(
    f"{fierypit_base_url}/ritual/join",
    json={
        "ritual_id": ritual_id,
        "topic": join_info_groq["topic"],
        "bootstrap_servers": kafka_bootstrap,
        "dis_domain": join_info_groq["dis_domain"],
        "evaluator": "clara_mind_splinter_groq",
        "node_id": GROQ_SPLINTER_NODE_ID,
        "self_node_id": GROQ_SPLINTER_NODE_ID,
        "eval_timeout_s": 90.0,
    },
    headers=auth_headers,
)
```

**3. Orchestrator's `consult_step/N` shrinks to two hops** (vs. Option 1's
four):

```prolog
consult_local_splinter(Query, Ctx, Answer, CiteCount) :-
    caws_offer('local-splinter', "consult/local", _{prompt: Query, context: Ctx}, Cid),
    caws_await(Cid, Raw),
    extract_hohi_response(Raw, ResponseJson),
    atom_json_dict(ResponseJson, D, [value_string_as(atom)]),
    Answer = D.text, CiteCount = D.citation_count.

consult_step(Query, Ctx, MaxCrawls, TopicPath, IngestTopicPath, TopicSubject,
             IdleSeconds, MaxWaitS, WorkspaceId, LlmProvider, LlmModel,
             Answer, Citations, CitationCount, Action) :-
    consult_local_splinter(Query, Ctx, Answer1, CiteCount1),
    format(atom(Q1), "Does \"~w\" adequately answer the prompt: ~w?", [Answer1, Query]),
    (   clara_fy(Q1, Ctx, true)
    ->  Action = chat, Answer = Answer1, Citations = [], CitationCount = CiteCount1
        % Citations=[] here is the fidelity loss: we know HOW MANY citations
        % backed the answer, but not which ones (they never left local-splinter's
        % own engine) — good enough for a citation *count* in the reply, not for
        % per-citation display.
    ;   research_step(Query, MaxCrawls, TopicPath, IngestTopicPath, TopicSubject,
                       IdleSeconds, MaxWaitS, _R),
        (   catch(
                ruminate_and_assert_citations(Query,
                    _{context: Ctx, mode: hybrid, workspace: WorkspaceId,
                      llm_provider: LlmProvider, llm_model: LlmModel},
                    EdgeResult2),
                _, fail)
        ->  ruminate_answer(EdgeResult2, EdgeAnswer2),
            ruminate_citations(EdgeResult2, Cites2), length(Cites2, CiteCount2),
            format(atom(CombinePrompt2),
                   "Two candidate answers to '~w'. Combine into one short answer.~nA: ~w~nB (freshly researched): ~w",
                   [Query, Answer1, EdgeAnswer2]),
            consult_local_splinter(CombinePrompt2, Ctx, FinalAnswer, _),
            format(atom(Q2), "Does \"~w\" adequately answer the prompt: ~w?", [FinalAnswer, Query]),
            ( clara_fy(Q2, Ctx, true) -> Action = chat ; Action = exhausted ),
            Answer = FinalAnswer, Citations = Cites2, CitationCount = CiteCount2
        ;   Action = exhausted, Answer = Answer1, Citations = [], CitationCount = CiteCount1
        )
    ).
```

**Net comparison**: Option 2's orchestrator Prolog shrinks noticeably
(2 hops vs. 4), at the cost of a second registered source, a JSON-smuggled
citation count instead of per-citation detail, and one participant
(`local-splinter`) whose `eval_timeout_s` must cover its entire inner
chain in one shot — a single, less transparent clock instead of Option 1's
top-level `max_cycles`/`patience_cycles`. It would also be the first live
exercise of `peer_consult` in this codebase.

## Team input: a `cow` evaluator (Edgequake query-side, like Snek) — adopted

Team feedback on the first draft: could we have a `cow` evaluator, the
Edgequake counterpart to `snek`, so tier 2 is a Ritual participant like
every other tier instead of a direct in-engine Prolog call? Adopted — and
it turns out to fix something bigger than symmetry: it closes Option 2's
citation-fidelity gap (above), because citations can now travel as *data*
in a Hohi reply instead of as `the_cow.pl`'s thread-local `citation/8`
facts, which never survive a `caws_offer`/`caws_await` hop across engine
boundaries. This benefits both options, not just Option 2.

Confirmed by reading the actual Edgequake query path: `EdgequakeClient`
(`goat/models/EdgequakeClient.py`) currently only has *ingest*-side
methods (`submit_document`, `get_or_create_workspace`, etc.) — no query
method. The real RAG-query endpoint, used today only from Rust via
`clara-toolbox/src/tools/edgequake.rs`'s `query()` (line 161), is
`POST /api/v1/query` with `{query, mode, max_results?, llm_provider?,
llm_model?}` and an `X-Workspace-ID` header — the same shape
`the_cow.pl`'s `ruminate_opts/3` reaches indirectly through that Rust
tool. `cow` calls it directly over HTTP, the same way
`EdgequakeIngestEvaluator` already calls Edgequake's document API
directly rather than going through Prolog.

**1. `EdgequakeClient.query()`** (new method, mirrors `submit_document`'s
pattern):

```python
async def query(
    self, workspace_id: str, query: str, mode: str = "hybrid",
    max_results: Optional[int] = None,
    llm_provider: Optional[str] = None, llm_model: Optional[str] = None,
) -> dict[str, Any]:
    """POST /api/v1/query (X-Workspace-ID header). Mirrors the_cow.pl's
    ruminate_opts/3 result shape, reached directly over HTTP instead of
    via the Rust `edgequake` Prolog tool."""
    body: dict[str, Any] = {"query": query, "mode": mode}
    if max_results is not None: body["max_results"] = max_results
    if llm_provider is not None: body["llm_provider"] = llm_provider
    if llm_model is not None: body["llm_model"] = llm_model
    async with self._client({"X-Workspace-ID": workspace_id}) as client:
        response = await client.post("/api/v1/query", json=body)
    response.raise_for_status()
    return response.json()
```

**2. `CowEvaluator`** (new file, `goat/evaluators/custom/cow_evaluator.py`,
following `EdgequakeIngestEvaluator`'s pattern — plain `Evaluator`, no LLM
needed):

```python
class CowEvaluator(Evaluator):
    def __init__(self, edgequake_base_url, edgequake_api_key, tenant,
                 default_workspace_slug=None, evaluator_id=None, metadata=None):
        super().__init__(evaluator_id=evaluator_id, metadata=metadata)
        self._client = EdgequakeClient(edgequake_base_url, edgequake_api_key, tenant)
        self._default_workspace_slug = default_workspace_slug

    async def evaluate_async(self, offering: Offering) -> Tephra:
        data = offering.data
        query = data.get("query")
        if not query:
            return Tephra(tabu=Tabu(message="'query' is required", code=422))
        workspace_slug = data.get("workspace") or self._default_workspace_slug
        if not workspace_slug:
            return Tephra(tabu=Tabu(message="no workspace given and no default configured", code=422))
        try:
            workspace_id = await self._client.get_or_create_workspace(workspace_slug)
            result = await self._client.query(
                workspace_id, query, mode=data.get("mode", "hybrid"),
                max_results=data.get("max_results"),
                llm_provider=data.get("llm_provider"), llm_model=data.get("llm_model"),
            )
        except Exception as exc:
            return Tephra(tabu=Tabu(message=str(exc), code=502))
        sources = result.get("sources", [])
        # Citations travel as DATA here, not thread_local Prolog facts —
        # they survive a caws_offer/caws_await hop across engine boundaries.
        return Tephra(hohi=Hohi(response={
            "content": result.get("answer", ""),
            "citations": sources,
            "citation_count": len(sources),
        }))

    def evaluate(self, offering: Offering) -> Tephra:
        return asyncio.run(self.evaluate_async(offering))
```

**3. Register in `config/evaluators.yaml`**:

```yaml
- name: cow
  module: goat.evaluators.custom.cow_evaluator
  class: CowEvaluator
  parameters:
    edgequake_base_url: ${EDGEQUAKE_BASE_URL:-http://localhost:8000}
    edgequake_api_key: ${EDGEQUAKE_API_KEY:-}
    tenant: ${EDGEQUAKE_DEFAULT_TENANT}
```

Note the dual configuration this implies: `cow`'s own Edgequake
credentials/tenant come from these env vars (evaluator-registration time),
while our example script's own workspace resolution
(`resolve_workspace_id()`) uses its `--edgequake-base-url`/
`--edgequake-api-key`/`--tenant` CLI flags. These need to point at the
*same* Edgequake instance/tenant for `cow` to query the workspace our
script actually resolved and (on tier 4) ingested into — worth a comment
in the script, not just this doc.

**Effect on both options**:

- **Option 1**: tier 2 becomes `caws_offer(cow, "consult/edgequake", _{query:Query, context:Ctx, workspace:WorkspaceId, mode:hybrid, llm_provider:LlmProvider, llm_model:LlmModel}, Cid), caws_await(Cid, Raw)` — uniform with tiers 1/3/4, replacing the direct `ruminate_and_assert_citations/3` call. `consult_peer/5` (below) gets a citations-aware sibling to extract `content`/`citations`/`citation_count` from `cow`'s Hohi payload instead of `ruminate_answer/2`/`ruminate_citations/2`.
- **Option 2**: `local-splinter`'s inner `reasoned_response/3` replaces its direct `ruminate_and_assert_citations/3` call with the same `caws_offer(cow, ...)` — and can now forward `cow`'s *actual* citations list (not just a count) inside its own JSON-smuggled reply to the orchestrator, e.g. `dict_to_json(_{text: EdgeCombined, citations: EdgeCites, citation_count: EdgeCiteCount}, ResponseJson)`. This closes the citation-fidelity gap flagged in the Option 2 sketch above — the only remaining loss is Option 2 never asserting `citation/8` facts locally (Option 1 still does, via `cow`'s reply being fed straight through rather than via `ruminate_and_assert_citations/3`'s side effect — actually **neither** option asserts `citation/8` anymore once `cow` is in the picture, since `cow` returns data, not facts; if any downstream code relies on those thread-local facts existing, note that as a behavior change from `progressive_research.pl`'s original tier (b)).

## Ritual participants (Option 1, as proposed below)

Joined via `POST {fierypit_base_url}/ritual/join`, each bound to its own
dedicated wrangler slot (distinct `node_id` **and** distinct `self_node_id`
— the existing code has a documented, confirmed bug where identical
`self_node_id`s across participants collide into one Kafka consumer group
instead of each getting their own):

| node_id           | evaluator                  | purpose                          |
|-------------------|-----------------------------|-----------------------------------|
| `local-splinter`  | `clara_mind_splinter`      | tier 1 ask + reconciliation/combine asks at every later tier |
| `cow`             | `cow` (new — see "Team input" above) | tier 2 ask (Edgequake RAG query + citations, returned as data) |
| `groq-splinter`   | `clara_mind_splinter_groq` | tier 3 ask |
| `snek`            | `snek`                      | tier 4 web crawl (existing) |
| `edgequakeingest` | `edgequakeingest`          | tier 4 document-pipeline insert (existing) |

The outer `/deduce` submission itself still needs a focused evaluator set
via `POST /evaluators/set` before `POST /evaluate` (same as all three
existing examples) — reuse `clara_mind_splinter` for this too, with
`self_node_id="orchestrator"` (distinct from `local-splinter`'s slot) in
the deduce Offering. This means `clara_mind_splinter` effectively runs as
two separate instances (one unslotted/globally-focused for orchestration +
`clara_fy` sufficiency checks, one slotted as the `local-splinter` peer) —
a small resource redundancy, flagged here for visibility, in exchange for
uniform `caws_offer`-to-a-peer treatment of all three LLM tiers. Since
`clara_fy` always runs under the orchestrator's own (always-local) focus,
"satisfactory according to the local clara llm" holds true regardless of
which tier's answer is being checked.

## Prolog design (inline in the new example, adapted from `progressive_research.pl`)

Not registered as a frontend ruleset — embedded as a Python string in the
new script's own `build_prolog_clauses()`, same pattern as
`examples_ritual_snek_splinter.py`. Reuse verbatim where possible:

- `research_step/8` — copy unchanged from `progressive_research.pl` (tier 4:
  `caws_offer(snek, ...)` + `caws_offer(edgequakeingest, ...)`, await both).
- `extract_hohi_response/2` — copy unchanged (handles both
  `hohi.response.content` and `hohi.response.response` shapes).
- Sufficiency check — same `clara_fy(SufficiencyQ, Ctx, true)` pattern as
  `progressive_research.pl`, called after every tier.

New predicates:

- `consult_peer(NodeId, TopicPath, Prompt, Ctx, Answer)` — a small shared
  helper: `caws_offer(NodeId, TopicPath, _{prompt: Prompt, context: Ctx}, Cid), caws_await(Cid, Raw), extract_hohi_response(Raw, Answer)`.
  Used for `consult_peer(local_splinter, "consult/local", ...)` and
  `consult_peer(groq_splinter, "consult/groq", ...)` calls — tier 1's
  initial ask, tier 3's ask, and every combine/reconciliation step in
  between (all reconciliation goes through `local-splinter`, matching
  `progressive_research.pl`'s existing pattern of always re-pondering
  locally to synthesize a combined answer).
- `consult_cow(Query, Ctx, WorkspaceId, LlmProvider, LlmModel, Answer, Citations, CitationCount)`
  — tier 2's citations-aware sibling of `consult_peer/5`:
  `caws_offer(cow, "consult/edgequake", _{query:Query, context:Ctx, workspace:WorkspaceId, mode:hybrid, llm_provider:LlmProvider, llm_model:LlmModel}, Cid), caws_await(Cid, Raw)`,
  then extracts `content`/`citations`/`citation_count` from `cow`'s Hohi
  payload directly (no `ruminate_answer/2`/`ruminate_citations/2` needed —
  those are `the_cow.pl`-specific; `cow`'s reply is already flat JSON).
- `consult_step(Query, Ctx, WorkspaceId, LlmProvider, LlmModel, MaxCrawls, IdleSeconds, MaxWaitS, TopicPath, IngestTopicPath, TopicSubject, Answer, Citations, CitationCount, Action)`
  — the main sequential goal:
  1. `consult_peer(local_splinter, "consult/local", Query, Ctx, LocalAnswer)` → sufficiency check → `Action=chat` if sufficient.
  2. else `consult_cow(Query, Ctx, WorkspaceId, LlmProvider, LlmModel, EdgeAnswer, Cites, CiteCount)` (catch failure the same way the original direct call did) → combine via `consult_peer(local_splinter, ...)` reconciliation ask → sufficiency check.
  3. else `consult_peer(groq_splinter, "consult/groq", Query, Ctx, GroqAnswer)` → combine via `local-splinter` reconciliation ask → sufficiency check.
  4. else `research_step/8` (existing, unchanged) → `consult_cow(...)` once more → combine via `local-splinter` → final sufficiency check → `Action = chat` if sufficient, else `Action = exhausted` (not `deferred_query` — nothing is actually deferred/async-delivered here, the whole chain already ran synchronously inside one `/deduce` call).

## Python driver (`lildaemon/examples_ritual_progressive_consult.py`)

Structure mirrors `examples_ritual_rumination_ingest.py` closely (env-var
fallback via `.env` for every flag, same cleanup discipline):

1. `_fierypit_bearer_token(...)` (reused pattern).
2. `POST {dis_base_url}/ritual` → `ritual_id`.
3. For each of the five participants above (including `cow`): `GET /ritual/{id}/join?participant=X` then `POST {fierypit}/ritual/join {evaluator, node_id, self_node_id, eval_timeout_s}`.
4. Resolve the Edgequake workspace (`resolve_workspace_id()`, reused from `examples_ritual_rumination_answer.py`) — must resolve to the same tenant/workspace `cow`'s own `EDGEQUAKE_*` env vars point at (see "Team input" above).
5. `POST {fierypit}/evaluators/set {evaluator: clara_mind_splinter}`.
6. `POST {fierypit}/evaluate {data: {deduce: {ritual_id, self_node_id: "orchestrator", prolog_clauses: build_prolog_clauses(), initial_goal: consult_step(...), max_cycles, evaluator_patience_cycles, poll_max_wait_s}}}`.
7. Print a report: which tier produced the final answer, the final `Action`, the combined answer text, citation count.
8. `finally`: restore previous evaluator, `DELETE {fierypit}/ritual/{id}` (leaves all four participants at once), `DELETE {dis_base_url}/ritual/{id}`.

CLI (positional `query`, one-shot per invocation — matching all three
existing examples):

- Standard connection flags reused verbatim: `--dis-base-url`,
  `--fierypit-base-url`, `--kafka-bootstrap`, `--fierypit-username`,
  `--fierypit-password`, `--fierypit-identity`.
- `--local-splinter-evaluator` (default `clara_mind_splinter`),
  `--groq-splinter-evaluator` (default `clara_mind_splinter_groq`).
- `--max-crawls` (default 4, reused).
- `--edgequakeingest-idle-seconds` / `--edgequakeingest-max-wait-s` /
  `--edgequakeingest-eval-timeout-s` (reused defaults from the 3-party
  example).
- `--workspace-qualifier`, `--edgequake-base-url`, `--edgequake-api-key`
  (required), `--tenant` (required), `--llm-provider` / `--llm-model`
  (for Edgequake's own internal ruminate call — reused from
  `examples_ritual_rumination_answer.py`).
- `--max-cycles` (default **900** — notably higher than the largest
  existing example's 450, since this chains four sequential legs instead
  of two), `--evaluator-patience-cycles` (default **700**),
  `--poll-max-wait-s` (default **220.0**).

## Companion doc

`lildaemon/docs/ritual_progressive_consult_example.md` — same structure as
the three existing `ritual_*_example.md` docs (this repo's convention is
that every `examples_ritual_*.py` has one; the existing docs explicitly
cross-reference each other as required reading before writing the next).

## Files touched

- `lildaemon/examples_ritual_progressive_consult.py` — new.
- `lildaemon/docs/ritual_progressive_consult_example.md` — new.
- `lildaemon/goat/models/EdgequakeClient.py` — new `query()` method (see
  "Team input" above).
- `lildaemon/goat/evaluators/custom/cow_evaluator.py` — new (`CowEvaluator`).
- `lildaemon/config/evaluators.yaml` — new `cow` entry. This is the one
  change to a shared config file — the plan's original "purely additive,
  standalone" framing no longer fully holds now that `cow` needs
  registering the same way `snek`/`edgequakeingest` already are.
- No changes to `progressive_research.pl`, `runtime.py`, or
  `clara-frontdesk-poc`.

## Verification

- Smoke-test first with a cheap, fast configuration to confirm the
  four-participant join/leave lifecycle and Prolog wiring work before a
  full run: `--max-crawls 1 --edgequakeingest-max-wait-s 20 --max-cycles 100`
  against a query the local LLM can likely answer on tier 1 alone (fastest
  path, exercises join/cleanup without needing Groq or Snek).
- Full run against a query that plausibly needs escalation (something the
  local model can't answer from ponder alone or from the current Edgequake
  corpus), confirming: `GROQ_API_KEY` is set and the Groq tier is actually
  reached and answers; the web-research tier fires Snek + edgequakeingest
  and the final answer reflects newly-ingested content; the printed report
  names the correct terminal tier/`Action`.
- After any run (success or failure), `GET {dis_base_url}/ritual` should
  come back empty — confirms the `finally` cleanup actually left/deleted
  all five participants and didn't leak a standing Ritual.
- Check `docker logs` for the lildaemon/lildaemon-fierypit container during
  a run to confirm no `self_node_id` collision warnings (the documented
  bug from the 3-party example) and no evaluator-slot spawn errors.
- Confirm `cow` actually returns citations as data: force tier 2 (ask
  something the local model can't answer alone but that's in the
  Edgequake corpus) and check the printed `CitationCount` is nonzero and
  matches `cow`'s own Hohi payload — this is the whole point of the "Team
  input" change, so it's worth verifying explicitly rather than assuming
  it works because the code looks right.
