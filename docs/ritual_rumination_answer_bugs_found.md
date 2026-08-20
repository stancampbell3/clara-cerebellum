# Bugs found building the rumination-answer example (2026-08-19)

**Status: all four fixed.** See `docs/ritual_rumination_answer_bugs_fix_plan.md`
for the upstream fixes (implemented and committed) and a fifth, related
regression found and fixed the following day. This doc is the original,
point-in-time bug report — read the fix-plan doc for current status.

Four real bugs surfaced while building `lildaemon/examples_ritual_rumination_answer.py`
(a Ritual that answers a query using both Clara's own model and Edgequake,
combined — see `lildaemon/docs/ritual_rumination_answer_example.md` for the
full example writeup). All four were confirmed live via a minimal standalone
diagnostic Prolog goal, not guessed. Three are worked around inside the
example script; **one (#3) is a real `clara-prolog` library gap** worth
fixing upstream rather than working around again in the next example that
returns a structured result.

---

## 1. `the_rabbit.pl` / `the_cow.pl` are not auto-loaded into clara-api's Prolog runtime

**Symptom**: `existence_error(procedure, ponder_text/2)` thrown at
goal-execution time — but only visible in clara-api's own container log.
The `/evaluate` HTTP response itself just reports `"status": "converged"`
with `"prolog_solutions": []`, no error surfaced anywhere in the API
response.

**Root cause**: `the_coire.pl` (`caws_offer`/`caws_await`/`coire_topic_poll`)
is auto-loaded into clara-api's Prolog runtime, but `the_rabbit.pl`
(`ponder_text/2`) and `the_cow.pl` (`ruminate_opts/3` and friends) are not.
Every existing Ritual example only ever used `the_coire.pl`'s predicates,
so this had never been exercised before.

**Fix (in the example script)**: load both explicitly as the first two
`prolog_clauses` entries:
```prolog
:- use_module(library(the_rabbit)).
:- use_module(library(the_cow)).
```

**Owner-facing question**: is `the_coire.pl` special-cased somewhere in
clara-api's Prolog session bootstrap, or does it just happen to get loaded
as a side effect of something else Ritual-related? If the intent is that
*all* of `prolog-lib/` should be available without an explicit
`use_module`, that's a one-line fix in clara-api's session init. If not,
worth a note in `rituals_101.md` that anything beyond `the_coire.pl`
requires an explicit `use_module` directive in `prolog_clauses` — the
current silent-failure-with-no-visible-error is an easy trap.

---

## 2. Nested `ponder_text`/`ruminate_opts` calls need real wall-clock budget, not more cycles

**Symptom**: with a 60s `--answer-poll-max-wait-s`, the client got
`"did not converge: running, timed_out: True"` even though the deduction
wasn't actually hung.

**Root cause**: `ponder_text/2`'s `splinteredmind` tool calls back into the
*same* FieryPit's own `/evaluate` endpoint synchronously
(`clara-toolbox/src/tools/splinteredmind.rs::Operation::Evaluate` →
`FieryPitClient::evaluate`). One `answer_step` call makes three such
sequential round trips (two `ponder_text` + one `ruminate_opts`, itself
LLM-backed on Edgequake's own side) — confirmed to take ~90s with a
cold-ish Ollama model. Confirmed via direct testing this is **not** a
deadlock: FastAPI handles the nested `/evaluate` call as a genuinely
concurrent request (`KindlingEvaluator.evaluate_async`'s deduce path runs
in a thread via `asyncio.to_thread`; its LLM-prompt path is true async via
httpx) — it's just real sequential latency.

**Fix (in the example script)**: raised `--answer-poll-max-wait-s` default
to 180.0. Cycle-count budget (`--answer-max-cycles`/`--answer-patience-cycles`)
barely matters here since `answer_step` has no `caws_offer`/`caws_await` to
retry against — it converges or fails on cycle 1 regardless.

**Not something to fix upstream** — just a real cost of chaining LLM calls
through this tool-call path, worth knowing about before assuming a slow
`/evaluate` response means something is broken.

---

## 3. `clara-prolog`'s Rust FFI has no JSON conversion for SWI dict terms — real library gap

**Symptom**: a Prolog solution variable bound to an SWI dict (`_{key: val,
...}`) comes back in the `/deduce` response's `prolog_solutions` as a
plain **string** containing the dict's `write/1`-style printed
representation (e.g. literally `"_{citations:[...],...}"`), not a JSON
object. Nested structures compound the damage — a list of dicts inside
that dict (e.g. Edgequake citation records) individually stringify too,
producing one large, unparseable blob.

**Root cause**: `clara-prolog/src/backend/ffi/conversion.rs::term_to_json`
handles `PL_VARIABLE`, `PL_ATOM`, `PL_INTEGER`, `PL_FLOAT`, `PL_STRING`,
`PL_NIL`, `PL_LIST_PAIR`, and `PL_TERM` (compound terms, including a
special case for `Key-Value` pairs) — but has **no case for SWI's native
dict term type**. Dicts fall through to the function's final wildcard
branch, which stringifies via `term_to_string` (`PL_get_chars` with
`CVT_WRITE`, i.e. `write/1` semantics — lossy, not reparseable in general,
since unquoted atom values containing commas/colons are ambiguous with
structural delimiters).

**Fix (in the example script)**: avoided the problem entirely by binding
nine flat variables (`ClaraAnswer`, `EdgeAnswer`, `Citations`,
`CitationCount`, `CombinedAnswer`, plus the four input args) instead of one
nested `R = _{query:..., clara_answer:..., citations: [...], ...}` result.
Plain atoms and integers convert correctly through `term_to_json`'s
existing `PL_ATOM`/`PL_INTEGER` cases.

**This is a real upstream gap, not just this example's problem.**
`examples_ritual_rumination_ingest.py`'s own `R = _{snek: SnekResult,
edgequakeingest: IngestResult}` (a `caws_await` result, also an SWI dict)
almost certainly hits the exact same stringification — it was never
verified against the raw JSON at the time, only against a human summary of
its contents in that example's "Verified live" writeup. **Any future
Ritual example that returns a dict-shaped result will hit this.**

**Suggested fix**: add a case to `term_to_json` for SWI dict terms
(`PL_is_dict`/`PL_get_dict_ex`-style handling in the C API — SWI dicts are
not `PL_TERM` compounds at the C level, they need their own check),
converting each key/value pair into a proper `serde_json::Map` entry
(recursing `term_to_json` on each value, same as the existing `Key-Value`
compound-term case already does for the `-`/2 shape).

---

## 4. SWI dict dot-notation silently mis-evaluates for asserted (not consulted) clauses, plus two follow-on issues

**Symptom**: `extract_hohi_response(Dict, Response) :- Response =
Dict.hohi.response.response.` (copied from the experimental
`clara-prolog/docs/examples/ruminate/ruminate.pl`) neither threw nor
failed — it silently bound `Response` to a raw `'.'/2` cons-cell term
(effectively a malformed list literal containing the original `Dict`
nested inside it), which then serialized as an unreadable
`{"functor": ".", "args": [...]}` blob in the JSON response.

**Root cause**: SWI's `Dict.Key` dot-notation is goal-expansion sugar
applied at **clause-compile time** by the normal source file reader
(`expand_term/2`, dict functional-notation support). It does not apply the
same way to clauses loaded via `prolog_clauses`/`assert` (however
clara-prolog loads them) rather than a normally `consult`ed/`use_module`d
file — the raw `.`/2 term structure the reader produces stays unexpanded,
and plain term unification (not `get_dict/3`) is what actually runs at
call time. This is why `the_cow.pl`'s own `Result.answer`-style dot-notation
calls (`ruminate_answer/2` etc.) work fine — that file is loaded normally
via `:- use_module` — while the *same syntax* hand-authored into a
`prolog_clauses` string breaks.

**Fix (in the example script)**: rewrote using explicit `get_dict/3` calls
(an ordinary predicate, no load-time expansion dependency):
```prolog
extract_hohi_response(Dict, Response) :-
    (   is_dict(Dict) -> D = Dict
    ;   atom_string(A, Dict), atom_json_dict(A, D, [value_string_as(atom)])
    ),
    get_dict(hohi, D, D1),
    get_dict(response, D1, D2),
    get_dict(content, D2, Response).
```

**Two more issues found isolating this one, via the diagnostic goal**:

- **`ponder_text/2`'s `Result` is *always* an SWI string, never a parsed
  dict** (`is_dict/1` false, `atom/1` false, `string/1` true — confirmed,
  not content-dependent as first assumed). Needs the `is_dict/1` guard
  above, and `atom_string/2` normalization before `atom_json_dict/3` —
  which requires an atom specifically and **fails silently** (no
  exception) when fed a string.
- **The correct leaf field is `hohi.response.content`, not
  `hohi.response.response`.** `ruminate.pl`'s own snippet — the source this
  was copied from — has the wrong field name. It's flagged in this
  project's own working notes as experimental/never run live, which is
  presumably why the mismatch was never caught. `clara_evaluate/2`'s JSON
  envelope for the `splinteredmind` tool is `{status, hohi: {code,
  response: {content, tool_call}}}` — note this is a *reduced* envelope
  (just the Hohi portion), not the full `{timestamp, hohi, tabu, task_id}`
  shape a direct `/evaluate` HTTP call returns.

**Suggested fix for `ruminate.pl`**: since it's a doc/example file (not a
loaded library), either correct its `extract_hohi_response/2` to use
`get_dict/3` + the right field name, or add a comment warning that its
dot-notation form only works when the file is actually consulted, not when
copied into hand-authored `prolog_clauses`.

---

## Not a bug, but related: Prolog clause accumulation makes iterative debugging confusing

Not new — already documented in `lildaemon/docs/ritual_rumination_ingest_example.md`
("clara-api's Prolog engine retains `prolog_clauses` assertions across
separate `/deduce` requests within one server process lifetime"). Worth
re-flagging here because it directly affected debugging bugs #3 and #4:
repeatedly changing a `prolog_clauses` predicate body across test
iterations against the same long-lived clara-api process means Prolog
tries the *oldest* matching clause first — a stale, already-buggy version
of `extract_hohi_response/2` from an earlier iteration kept winning over a
corrected one asserted moments later, producing confusing repeat failures
that looked like the fix hadn't taken effect. `docker restart
docker-clara-api-1` between debugging iterations resolved it.
