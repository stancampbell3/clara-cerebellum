# Progressive-consult Ritual example: live verification status

**Status: verified live end-to-end at tier 1 only, 2026-08-25. Tiers 2-4 are
currently blocked by an open Dis engine bug — see
[`dis_sequential_caws_await_bug.md`](dis_sequential_caws_await_bug.md).**
Implementation (`examples_ritual_progressive_consult.py`, Option 1 from
`ritual_progressive_consult_plan.md`, plus the `cow` evaluator) is
committed. All four tiers were verified individually via a new
`tests/test_ritual_progressive_consult_example.py` integration suite, plus
one full end-to-end run — 5/5 passing against the real dev stack. Six real
issues were found and fixed along the way (four code bugs, one deployment
gap, one test-authoring gotcha); see below for each. **A later pass
(same day) building deterministic mocked tests to force the full
orchestrator through tiers 2-4 found that `consult_step/15`'s tiers 2-4
never actually complete in production**: any second `caws_offer`/
`caws_await` round trip issued after an earlier one has resolved, within
the same deduction goal, never converges to a solution. Full writeup,
repro steps, and root-cause hypotheses: `dis_sequential_caws_await_bug.md`.

## Summary

Implemented the reviewed design
(`clara-cerebellum/docs/ritual_progressive_consult_plan.md`,
`lildaemon/docs/ritual_progressive_consult_example.md`) and carried it
through to full live verification:

1. Reset the dev stack to a clean baseline (rituals, Edgequake workspace).
2. Fixed a deployment gap: `cow` wasn't registered in the running
   container (code on disk doesn't reach a baked Docker image without a
   rebuild).
3. Fixed a real bug in this example's own new Prolog: a response-shape
   mismatch parsing `caws_await` results (`extract_caws_response/2`).
4. Fixed a real, pre-existing bug in a shared `clara-cerebellum` library
   file (`the_rabbit.pl`'s `descriminate/2` family), exposed by this being
   the first caller to run `clara_fy` under a tool-calling evaluator's
   focus.
5. Hit a missing "classify" tool (`clara_fy`'s fastText classifier) —
   resolved by pointing `docker-clara-api-1`'s config at a filesystem-
   mapped classify model and rebuilding (done outside this session).
6. Fixed a second real bug in this example's own Prolog: `consult_cow/8`
   used SWI dict dot-notation (`D1.content`, `D1.get(...)`), which
   silently fails to evaluate for a clause loaded via `prolog_clauses`/
   assert rather than normally consulted.
7. Wrote `tests/test_ritual_progressive_consult_example.py` (one isolated
   test per tier, reusing the shipped `build_prolog_clauses()` output
   directly rather than a re-typed copy, plus one full end-to-end test).
   Found two more issues purely through writing and running these tests:
   a wrong Kafka bootstrap address, and a subtle "unique-looking test
   query poisons the real web search" gotcha (see below) — both fixed.

## Dev-stack reset performed

Per direction that resetting rituals/the Edgequake assistant workspace is
acceptable (and expected) on this dev stack, before live-testing:

- **Dis rituals**: 15 active rituals found (13 `assistant-demo`, 2 orphaned
  `rumination-ingest-demo` test artifacts) — all deleted via
  `DELETE /ritual/{id}`, per explicit direction to clear everything rather
  than guess which were still-live frontdesk sessions.
- **FieryPit standing participant**: one leftover participant (tied to one
  of the deleted `assistant-demo` rituals) was still running on
  `docker-lildaemon-1`'s own side after the Dis-side delete — cleaned up
  separately via an authenticated `DELETE {fierypit}/ritual/{id}` call.
  Worth flagging on its own: Dis's registry and FieryPit's own
  `RitualManager` can drift apart (a ritual deleted from Dis doesn't
  automatically stop a standing participant still running in lildaemon).
- **Edgequake `assistant.general` workspace**: 17 documents (real content
  from prior background-research runs) deleted via
  `EdgequakeClient.delete_document`. Confirmed empty afterward.
- **Not touched**: FieryPit/system user accounts — out of scope for this
  pass's minimal, ad hoc cleanup. Worth revisiting if a proper reset
  script ever gets built.

## Bugs found and fixed

### 1. `cow` not registered in the running container (deployment gap, not a code bug)

`docker-lildaemon-1` bakes `config/` and `goat/` into its image at build
time — editing files on the host has no effect on the running container,
and a live `docker cp` patch is lost the next time the container is
recreated from an unrebuilt image (confirmed live: this happened once
mid-session). **Fix**: proper rebuild (`./clara build lildaemon &&
./clara up -d lildaemon`) once the code was committed — durable, unlike
the earlier live-patch.

### 2. `extract_hohi_response/2` shape mismatch for `caws_await` results — fixed, committed

**Root cause**: `RitualParticipant._publish_tephra`
(`lildaemon/goat/models/RitualParticipant.py`) sets a reply envelope's
payload to `tephra.hohi.to_dict()` directly — `{"response": {...}, "code":
200}`, **one** level of nesting. `extract_hohi_response/2` (copied from
`progressive_research.pl`) expects **two** levels
(`{hohi:{response:{...}}}`) — the shape of `ponder_text/2`'s direct
`clara_evaluate/2` FFI-call envelope, a different code path this example
never actually calls.

This looked at first like a cycle-budget/timing race — convergence
happened within about one poll cycle of the real Hohi being published,
and the failure showed up almost identically across cycle counts
regardless of `max_cycles`/`patience_cycles`. Cross-referencing
`docker-lildaemon-1` and `docker-clara-api-1` logs by timestamp showed the
Hohi *was* published correctly and *was* received — the extraction
immediately after was what failed, every time, deterministically.
`clara-cycle::controller`'s own `has_converged` logic (read directly from
source) confirmed this: it blocks convergence while offers are genuinely
pending, and only converges once the root goal reaches a *resolved* truth
value (`known_false` counts) — i.e. it converged correctly, on a goal that
had genuinely, permanently failed.

**Fix**: new `extract_caws_response/2` predicate (one level of nesting),
used by `consult_peer/5` and `consult_cow/8` instead of
`extract_hohi_response/2`. Verified via an isolated single-tier
diagnostic, then via `test_tier1_local_splinter_answers_directly` and
`test_tier3_groq_splinter_answers`.

**Status**: fixed and committed (`lildaemon@da67d15`).

### 3. `the_rabbit.pl`'s `descriminate/2` family — pre-existing shared-library bug, fixed and committed

**Root cause**: `descriminate/2`, `descriminate_k/3`, and
`descriminate_k_with_context/4` (`clara-prolog/prolog-lib/the_rabbit.pl`
— internal helpers `clara_fy` depends on) extract the LLM's text via a
**naive** `extract_nested(LLMSez, [hohi, response, response], Response)`
— only the plain-`OllamaEvaluator` response shape. This is the exact same
"`ponder_text` envelope shape is deterministic per evaluator class, not
random" issue already root-caused and fixed elsewhere (see project
memory; `progressive_research.pl`'s own `extract_hohi_response/2` already
handles both shapes) — but `the_rabbit.pl`'s own internal helper was
never given the same fix, because every existing `clara_fy` caller
happens to run under a plain, non-tool-calling evaluator's focus.

This example is the first caller to invoke `clara_fy` while a
**tool-calling** evaluator (`clara_mind_splinter`) is focused — required
here since that's also the evaluator driving the whole deduce dispatch —
which exposed the latent gap: `ponder_text`'s internal shape under a
tool-calling focus is `hohi.response.content`, not
`hohi.response.response`, so the naive extraction failed every time,
logged server-side as `Error: Could not extract response from LLM
output.`

**Fix**: new `extract_llm_response/2` helper (the same defensive
content-then-response fallback), used by all three `descriminate*`
predicates.

**Status**: fixed and committed (`clara-cerebellum@ceff84a`).

### 4. Missing "classify" tool — resolved (infrastructure, not code)

After fix #3, a new error surfaced: `[ERROR clara_toolbox::ffi] Tool
execution error: Tool not found: classify`. `clara_fy`'s `classify_text/2`
needs a "classify" tool (the fastText-based "Ember Devil" classifier, per
`the_rat.pl`'s own docstring) that wasn't available in a bare one-shot
`/deduce` session, even though `clara_fy` already worked in the live
assistant-demo deployment.

**Resolution** (done outside this session): `docker-clara-api-1`'s
compose config/`.env` was pointed at a filesystem-mapped classify model
(`/etc/clara/models/dagda-0.2.bin`), and the stack was rebuilt. Confirmed
via container logs: `Loading classify tool with model: ...` →
`ClassifyTool loaded model from: ...` → `Classify tool registered
successfully`. Not root-caused further (e.g. whether the model was simply
never configured for this evaluator/session type before, vs. a narrower
session-scoping issue) — the practical fix landed, and `clara_fy` is
confirmed working under a tool-calling evaluator's focus in every one of
this example's tiers now.

### 5. `consult_cow/8` dict dot-notation — fixed, not yet committed

**Root cause**: `consult_cow/8` originally read `Answer = D1.content`,
`Citations = D1.get(citations, [])`, `CitationCount = D1.get(citation_count, 0)`
— SWI-Prolog dict dot-notation. Confirmed live: this **silently fails to
evaluate** for a clause loaded via `prolog_clauses`/assert rather than
normally consulted — it returns the raw, unevaluated `'.'(Dict, content)`
compound term instead of the actual value. Same landmine already
documented in `examples_ritual_rumination_answer.py`'s own
`build_answer_prolog_clauses` docstring (dot-notation is goal-expansion
sugar applied at clause-*compile*-time by the normal source reader, which
asserted clauses never go through) — but this is a *second*,
independent instance of it, in a different file, found by isolated tier-2
testing.

**Fix**: rewrote to explicit `get_dict/3` calls with if-then-else
defaults, matching the established safe pattern.

**Status**: fixed in `lildaemon/examples_ritual_progressive_consult.py`,
verified via `test_tier2_cow_queries_edgequake`. Not yet committed.

### 6. Wrong Kafka bootstrap address in the new test file — fixed

The new integration test file initially followed the sibling test files'
own documented `Run:` example (`KAFKA_BOOTSTRAP=localhost:9094`). This is
wrong for this environment: `bootstrap_servers` is passed straight through
in the `/ritual/join` request body, which FieryPit — running inside
`docker-lildaemon-1`, a normal docker-compose bridge network with no host
networking — uses to connect *its own* Kafka consumer/producer, not the
test process. A host-facing address there makes every `caws_offer`/
`caws_await` round trip silently time out after exhausting its full
`evaluator_patience_cycles` budget (~200-400 cycles, several minutes)
rather than fail fast. The real example script's own CLI already defaults
`--kafka-bootstrap` to `kafka:9092` for exactly this reason; the sibling
test files' docstrings documenting `localhost:9094` appear to be stale for
this environment's topology (not fixed there — out of scope for this
change).

**Fix**: corrected the new test file's own `Run:` docstring and usage to
`kafka:9092`, with an explanatory comment.

### 7. UUID-suffixed test queries silently zero out web search — fixed

`test_tier4_research_step_snek_and_edgequakeingest` initially used the
same "real-looking but decorated with a random hex suffix" query
convention as `test_ritual_rumination_ingest_example.py`
(`f"itest ritual progressive consult tier4 {uuid.uuid4().hex[:8]}"`, later
`f"how to store fresh basil to keep it longer (itest {uuid})"`). Both
failed identically and fast (~25-40s, ~70 cycles, nowhere near patience
exhaustion) with `SnekEvaluator` publishing a real Tabu
(`"No seed URLs found for query: ..."`, `snek_evaluator.py:158`) almost
immediately after a *successful* (200) Google Custom Search call.

Confirmed directly (`web_search(...)` called by hand): the bare query
`"how to store fresh basil to keep it longer"` returns 10 results; the
identical query with an 8-character random hex suffix appended — with or
without parentheses — returns **zero**. Google Custom Search's matching
appears to effectively require all query terms to correspond to
something indexed; a random hex token doesn't, and drags the whole query
to zero results. This is not a code bug anywhere in this stack — it's a
property of the search query text itself — but it makes any test that
decorates a real query with random-looking uniqueness noise fail
deterministically. `test_ritual_rumination_ingest_example.py`'s own query
convention (pure noise, no real query at all) carries the same latent
risk and was not fixed here (out of scope for this file).

**Fix**: keep the actual search query text completely clean; fold the
uuid into the **topic subject** only (`research_step/8` already takes
`TopicSubject` as an argument separate from `Query`), so uniqueness for
ad hoc Coire topic naming doesn't touch what's actually sent to Google.

## Related architectural note raised during this work (not investigated further)

Edgequake runs on the same GPU as the rest of Clara (Ollama). Live testing
showed `local-splinter`'s real Ollama calls taking 3-6 seconds each for a
trivial prompt; a full 4-tier chain reaching tier 4 (web crawl + Groq +
multiple Edgequake queries) could meaningfully contend for GPU time across
Clara and Edgequake simultaneously. Worth a future design discussion on
whether Edgequake's own LLM/embedding calls, or some pipeline legs of this
example, should move to Groq or another remote LLM to avoid blocking
Clara's own GPU usage. Flagged, not acted on.

## Deployment notes (for whoever picks this back up)

- Both `docker-lildaemon-1` and `docker-clara-api-1` bake their
  application code into the image at build time — they are **not**
  live-mounted. `docker cp` + `./clara restart <service>` works for quick
  iteration but does **not** survive a full rebuild or container
  recreation from an unrebuilt image (confirmed live: this happened once
  and silently reverted the `cow` registration). Prefer a proper
  `./clara build <service> && ./clara up -d <service>` once code is
  settled — this is what actually landed both Prolog fixes durably.
- Container runtime env vars come from `clara-cerebellum/docker/.env`,
  **not** `lildaemon/.env` — confirmed different values are set in each
  (e.g. `EDGEQUAKE_BASE_URL`/`EDGEQUAKE_API_KEY`/`ASSISTANT_WORKSPACE_SLUG`
  are unset in the docker `.env`, falling back to `cow`/`edgequakeingest`'s
  own YAML defaults — matches `edgequakeingest`'s existing, already-working
  configuration, so not a new problem).
- `bootstrap_servers` passed to `POST /ritual/join` must be the
  Docker-internal Kafka address (`kafka:9092`), never a host-facing one,
  regardless of what address the calling script/test itself uses to reach
  Dis/FieryPit's own HTTP APIs (`localhost:...` is fine for those).
- Test/example queries sent to `snek` (Google Custom Search) must be real
  text — never decorate them with random-looking uniqueness suffixes.
  Achieve uniqueness (e.g. for ad hoc Coire topic naming) in a separate
  identifier instead.
- `docker-lildaemon-1`'s own Prolog-adjacent state (crawlee caches, etc.)
  and `docker-clara-api-1`'s Prolog engine both accumulate state across
  requests within one process lifetime (`prolog_clauses` assertions are
  never cleared between `/deduce` calls) — a full container restart is a
  cheap, reliable way to rule this out as a cause when debugging
  inconsistent live-test behavior.

## Verified live, 2026-08-25

`tests/test_ritual_progressive_consult_example.py`, 5/5 passing against
the real dev stack (`KAFKA_BOOTSTRAP=kafka:9092 CEREBELLUM_URL=http://localhost:8080
FIERYPIT_BASE_URL=http://localhost:6666 ... python -m pytest -m integration
tests/test_ritual_progressive_consult_example.py -v`, 117s total):

- `test_tier1_local_splinter_answers_directly` — `consult_local/3` via
  `local-splinter`, real Ollama answer.
- `test_tier2_cow_queries_edgequake` — `consult_cow/8` via `cow`, real
  Edgequake query round trip (structure-only assertions; citation-count
  correctness depends on Edgequake's own retrieval-readiness timing for a
  freshly-ingested document, out of this example's control).
- `test_tier3_groq_splinter_answers` — `consult_groq/3` via
  `groq-splinter`, real Groq API answer.
- `test_tier4_research_step_snek_and_edgequakeingest` — `research_step/8`
  via `snek` + `edgequakeingest`, real crawl + real ingest.
- `test_full_chain_converges_at_tier1` — the full `run_demo()` orchestrator,
  five participants joined/left cleanly, `consult_step/15` converging with
  `Action = chat` at tier 1.

Not yet exercised live: a query that genuinely escalates past tier 1
through tiers 2-4 in the *full* orchestrator (as opposed to each tier
being individually confirmed) — both manual full-chain attempts today
happened to self-satisfy at tier 1, since `clara_mind_splinter` has its
own `web_search` tool and can "self-serve" current information. Worth a
future live run with a query engineered to need genuine escalation
(without falling into bug #7's trap).

## Outstanding / next steps

1. Commit `consult_cow/8`'s dot-notation fix and the new integration test
   file (pending explicit go-ahead, per this session's commit discipline).
2. Rebuild (not just live-patch) is already done for both fixes shipped
   today — future changes should follow the same discipline rather than
   live-patching only.
3. **Blocking, newly found 2026-08-25**: tiers 2-4 of the full orchestrator
   don't actually work — see `dis_sequential_caws_await_bug.md` for the
   full writeup and suggested triage options. This supersedes item 3's
   original "nice-to-have" framing from earlier today: it turned out to be
   a genuine, previously-undetected production bug, not just a missing
   test.
4. GPU/Groq offload architecture discussion (see above) — separate from
   this example's own correctness.
5. Carried over, unrelated to this work: `ritual_multi_evaluator_plan.md`
   still needs its own documentation pass (see
   `ritual_multi_evaluator_doc_stale_todo` memory) — still open.

## Files touched, by commit status

**Committed**: `lildaemon@ab052a9` (original implementation),
`lildaemon@da67d15` (`extract_caws_response/2` fix),
`clara-cerebellum@275e599` and earlier (design docs, `cow` evaluator),
`clara-cerebellum@ceff84a` (`the_rabbit.pl` fix).

**Fixed, not yet committed**:
- `lildaemon/examples_ritual_progressive_consult.py` — `consult_cow/8`
  dot-notation fix (item 5 above).
- `lildaemon/tests/test_ritual_progressive_consult_example.py` — new
  integration test suite (item 7 above, plus the tier 1-4 isolated tests).

**Infrastructure change, outside this session's git history**:
`docker-clara-api-1`'s classify-model configuration (item 4 above).
