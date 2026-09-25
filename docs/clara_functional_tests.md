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
- **Mojibake in replies (FIXED 2026-09-25, uncommitted)**: a reply contained a double-encoded em dash ("Ã¢Â\x80Â\x94"). Root cause: in Dis's Prolog FFI
  (`clara-prolog/src/backend/ffi`), every text-in call read ISO-8859-1 (`PL_chars_to_term` for goals and clauses, `PL_put_atom_chars` / `PL_put_string_chars`,
  `PL_unify_string_chars` / `PL_unify_atom_chars` for LLM replies and Coire events), so each UTF-8 byte became its own character (an em dash was 3 characters;
  `atom_length` counted bytes). Fixed by routing all of them through `PL_put_term_from_chars` / `PL_put_chars` / `PL_unify_chars` with `REP_UTF8`
  (`goal_text_to_term`, `put_text_utf8`, `unify_text_utf8` in `conversion.rs`). Correct text then produced wide strings, which exposed `PL_get_string` (narrow
  only) in the string-to-JSON path; that now uses `PL_get_chars` with `REP_UTF8`. Tests: 3 in `prolog_integration_tests.rs` and a functional regression
  (`test_non_ascii_text_is_not_corrupted_on_the_way_out`). The CLIPS boundary was audited afterwards (below).
- A deliberation and a brainstorm row went to `failed` shortly after their session was deleted by test teardown, so delivery tests must keep the session open.

## Text-encoding audit of the polyglot boundaries (2026-09-25, follow-up to the mojibake)

Strings cross Python, Rust, Prolog and CLIPS, so each boundary was checked with a known non-ASCII string (em dash, accented letter, CJK, emoji).

| Boundary | Result |
|---|---|
| Python <-> Rust HTTP/JSON, Kafka envelopes, DuckDB, container locales (`C.UTF-8` on limbic and pineal) | clean |
| Rust -> Prolog (goals, clauses, values, LLM replies, Coire events) | **was broken**, fixed (`REP_UTF8` entry points; `PL_get_chars` on the way out) |
| Prolog sources and library files (`consult_string`, `the_coire.pl`) | clean after the FFI fix |
| Rust <-> CLIPS (`Eval`, `Build`, routers, facts, `str-length`, `printout`, 5000-character output) | clean; CLIPS counts characters and keeps the bytes |
| **Prolog <-> CLIPS transpiler** (`clara-cycle/src/transpile.rs`) | **was broken**: read strings byte by byte (mojibake), stopped names at the first accented letter and silently dropped the rest (`cafe(x)` with an accented e became `(caf)`), rejected non-ASCII atoms, ignored any text after the term. Now decodes whole characters, accepts Unicode atoms and variables, and rejects trailing text (one closing full stop is allowed). |
| **CLIPS `coire-publish`** (`clara-clips/clp-lib/the_coire.clp`) | **was broken, independent of encoding**: built its JSON payload by concatenation without escaping, so a goal or fact containing a double quote, backslash or newline was invalid JSON and the event was silently lost. New `coire-json-escape`. |

Tests added: `clara-cycle/tests/transpile_utf8.rs` (9), `clara-cycle/tests/utf8_deduce_test.rs` (a full Prolog -> CLIPS -> Prolog deduction carrying the text and embedded quotes),
`clara-clips/tests/utf8_tests.rs` (5). CLIPS gotchas found on the way: `loop-for-count` needs `do`; CLIPS reads an unknown escape such as backslash-t as the plain letter `t`
(a first version of the escape function turned every `t` into a tab); `upcase` does not change non-ASCII letters (CLIPS behaviour, not corruption).

Still open (left for the transduction / Cobbler editor work, see memory `transduction_revisit_lockdown`): `clara-cycle/src/transduction.rs` sanitizes generated
names with `is_ascii_alphanumeric`, so non-ASCII names collapse to `_` (two names differing only by accented letters would collide); `coire-emit` failures are still silent.

## Finding: the sufficiency check escalates trivial questions

Measured 2026-09-25 on "What is the capital of France?" through `progressive`, 8 fresh sessions each: 3 `local`, 3 `groq`, 2 `edgequake` (fixed build) and 2 `local`,
5 `edgequake`, 1 `deferred` (pre-fix build), so the encoding fixes did not cause it. The "is pondering enough?" check is an LLM call and is inconsistent even on a question
the model answers correctly every time, so roughly half of trivial turns pay for Edgequake, a Groq call (rate-limit exposure) or even queue a research crawl. The local-knowledge
test therefore requires only that the retrieval-free path exists and is clean (1 of 5 sessions). Candidate improvements, none started: a stricter or constrained-decoding
verdict for the sufficiency check (see memory `constrained_decoding_verdict_model`), a cheap "well-known fact" pre-check, or caching verdicts.
