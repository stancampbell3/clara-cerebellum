# Functional tests for the composed `clara` analyst

*Status: first pass built and run live 2026-09-25 (8 path tests pass, 31 routing rows pass, 7 known gaps recorded). Second round is listed at the end.*

## What this is

`lildaemon/tests/functional/` drives the **live** `clara` analyst through its subpaths and checks what actually happened. It is opt-in
(`CLARA_FUNCTIONAL=1`), skipped in the normal suite, and asserts on **structure** (which analyst answered, which tier, citations, side
effects), never on the model's wording, so it survives prompt and persona changes.

## The trace that makes it possible

`POST /assistant/sessions/{id}/send` now returns, in addition to `reply`, `action_taken`, `workspace_slug`, `citation_count`:

| Field | Meaning |
|---|---|
| `analyst` | the ruleset that answered; for `clara`, the one the router chose |
| `tier` | the path inside it: progressive `local` (pondering alone) / `edgequake` (grounded) / `groq` (second opinion) / `deferred` (research queued); others `chat`, `research`, `deliberate`, `brainstorm`, `ego` |
| `citations` | `[{document_id, file_path}]` for each citation (snippets omitted) |
| `fallback` | true when the composed path failed and the single-analyst path answered |

Each analyst `.pl` gained `assistant_turn/6` (with `Tier`); `assistant_turn/5` remains as a wrapper. The frontdesk ignores the new fields.

## Layers

1. **Routing corpus** (`test_clara_routing.py`, no LLM): `route/2` from `clara_router.pl` is registered with the live Dis and run over a table of queries. It is the
   routing contract written down. Rows marked **KNOWN GAP** are strict expected-failures: things we expect to work that the rules get wrong today; fixing the
   router turns them into failures that ask for the marker to be removed. Current gaps: build/write-code requests are not sent to the Ego; any short
   question-free message is treated as chit-chat (including "Tell me about rituals"); a greeting with a question mark is not chit-chat; the bare word
   "adopt " and the phrase "decide between" trigger deliberation even for factual questions.
2. **Live paths** (`test_clara_paths.py`): greeting (terse path, no side effects); generic knowledge (tier `local`, no citations, so no Edgequake query);
   Edgequake baseline canaries (grounded, citations, expected phrase and source file; 2 of 3 fresh sessions must pass, because the sufficiency check can
   legitimately accept an ungrounded answer); decision question -> deliberative; brainstorm -> id; `clara` listed but not the default.
3. **Fallback** is covered in-process (`tests/test_composed_analysts.py`): a failing composed classify yields `fallback: true` and the default analyst.

## Running it

    # step 0: stack up, baseline verified, ComfyUI off, model warm
    scripts/ground_state.sh verify --baseline /mnt/moonpool/clara-archives/baseline_v1
    cd lildaemon
    CLARA_FUNCTIONAL=1 EDGEQUAKE_BASELINE_DIR=/mnt/moonpool/clara-archives/baseline_v1 \
        .venv/bin/python -m pytest -m clara_functional tests/functional -v

Environment: `CLARA_FUNCTIONAL_URL` (lildaemon, default `http://localhost:6666`), `DIS_BASE_URL` (default `http://localhost:8080`), `CLARA_FUNCTIONAL_USER` /
`CLARA_FUNCTIONAL_PASSWORD` (a dedicated user, registered on first use), `CLARA_FUNCTIONAL_CANARIES` (a canaries file instead of the baseline's).
The routing layer alone takes about 3 s; the path layer about 7 minutes (a warm-up turn, then real LLM turns; Edgequake turns are 25 to 30 s, a brainstorm
turn took 134 s). The run ends with a table of every turn (analyst, tier, citations, seconds, retries).

## Side effects and cleanup

The decision and brainstorm cases start a real deliberation/brainstorm in the background, and an Edgequake attempt that lands on `deferred` queues a research
crawl. The run prints a reminder when this happened. Afterwards:

    scripts/ground_state.sh queue reset-coupled --to /mnt/moonpool/clara-archives/queue_export_<date>
    python3 scripts/ground_state_kafka.py --dis http://127.0.0.1:8080 reset      # or wait for Dis's topic reaper
    scripts/ground_state.sh verify --baseline /mnt/moonpool/clara-archives/baseline_v1

## Known flakiness

Cold-model first turns fail (a throwaway warm-up turn absorbs it; a turn is retried once). Groq second opinions can 429, so no test requires tier `groq`.
Pass-rate cases print every attempt's trace on failure.

## Second round (not built)

Unknown-knowledge path (a research task is created, then polled to delivery, then the baseline is restored); delivery of deliberation and brainstorm results (the
session must stay alive: deleting it at teardown makes those rows fail); the Ego multi-file scenario (needs gate `superego` on pineal, scripted approve/deny on
escalations, and an allowlist decision: several `export_document` calls versus a new file-writing action, plus a routing cue for build requests); a voice-consistency check once
the shared Clara voice exists; an LLM judge for answer quality; a nightly wrapper that records trends.

## Findings from the first runs

- Router gaps listed above (encoded as strict expected-failures).
- **Mojibake in replies**: a reply contained a double-encoded em dash ("Ã¢Â\x80Â\x94"). Earlier Ego and example outputs showed the same garbling, so text is being
  mis-decoded somewhere between the model and the API; not yet located.
- A deliberation and a brainstorm row went to `failed` shortly after their session was deleted by test teardown, so delivery tests must keep the session open.
