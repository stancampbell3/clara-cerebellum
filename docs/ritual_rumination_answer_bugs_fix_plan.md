# Fix plan: the 4 bugs (+1 related issue) from the rumination-answer example

**Status**: implemented and committed (`bcca6f0`, 2026-08-20). A real gap
in item 2 was found the same day (see "Regression found after shipping"
below) and has since been **fixed** (`73cf2a5`) — clause isolation is now
solid, verified live with a 10/10 and a 160/160 stress test.

**Companion doc**: `docs/ritual_rumination_answer_bugs_found.md` — the original
bug report (symptoms, root causes, example-local workarounds) written while
building `lildaemon/examples_ritual_rumination_answer.py`. This doc proposes
upstream fixes for the three items there that are real shared-infrastructure
gaps, not just this-one-example problems, plus a concrete root cause and fix for
the "not a bug, but related" clause-accumulation issue that doc left unexplained.

## Why fix these upstream instead of leaving them as example-local workarounds

Three of the four bugs are gaps in `clara-prolog` (the Rust↔SWI-Prolog FFI layer)
or in clara-api's Prolog session bootstrap — every current and future Ritual
example goes through this same code path. Leaving them as workarounds inside one
example script means the next example (or the next person extending this one)
re-discovers and re-works-around the same three issues from scratch. The fourth
(nested-call latency) is genuinely example-specific and isn't proposed for a
code fix here.

## Root cause found this session: Prolog clause accumulation across requests

The original bug report flagged this as "not a bug, but related" without
explaining why it happens. Reading `clara-prolog`'s Rust FFI source directly
this session found the actual mechanism:

`PrologEnvironment::new()` (`clara-prolog/src/backend/ffi/environment.rs:160`)
creates a genuinely separate SWI-Prolog *engine* per deduction
(`PL_create_engine`, correctly destroyed on `Drop` via `PL_destroy_engine` — no
engine leak). But SWI engines are implemented like Prolog *threads*: they share
one global dynamic-predicate database unless a predicate is explicitly declared
`:- thread_local`. `the_coire.pl` already relies on this — every one of its
mutable predicates (`caws_offer_sent/2`, `coire_session_id/1`, etc.) is declared
`:- thread_local` (`the_coire.pl:36-82`), which is *why* Coire state is
correctly session-isolated today. But `consult_string()`
(`environment.rs:298-322`) — the function every Ritual example's
`prolog_clauses` list goes through — just does a plain `assertz(T)` for each
parsed clause, with no `thread_local` declaration. So any hand-authored
predicate (`answer_step/9`, the ingest example's helper predicates, etc.) lands
in the shared global module and accumulates one extra clause per `/deduce`
call, for the life of the clara-api process — Prolog then always tries the
*oldest* matching clause first, which is what produced the confusing "fix
didn't take effect" symptom during debugging both example scripts.

## The 5 items, in dependency order

### 1. Auto-load `the_rabbit.pl` / `the_cow.pl`

**File**: `clara-prolog/src/backend/ffi/environment.rs`, inside
`ensure_prolog_initialized()` (~lines 89-103), right after the existing
`use_module(library(the_coire))` block.

Add the identical three-line pattern (build the goal via `PL_chars_to_term` +
`PL_call`, log success/failure, return `Err(...)` on failure) for
`library(the_rabbit)` and `library(the_cow)`. Once this lands, example scripts
no longer need their own `:- use_module(...)` directives as the first two
`prolog_clauses` entries — though leaving them is harmless, since `use_module`
on an already-loaded library is a no-op.

**Risk**: low. Purely additive, mirrors an existing, working pattern exactly.

### 2. Fix clause accumulation via auto-`thread_local` (do before #3/#4 rework)

**File**: `clara-prolog/src/backend/ffi/environment.rs`, `consult_string()`
(lines 298-322).

Currently, for each top-level parsed term `T`: directives get `call`ed,
otherwise it's `assertz(T)`. Change the "otherwise" branch so that the **first
time** a given predicate indicator `F/A` is seen within one `consult_string`
call, it's declared `:- thread_local F/A` before the `assertz` (a `catch/3`
guard makes re-declaring across different engines safe). `thread_local/1` on a
predicate that already has clauses from a *previous, now-destroyed* engine is
safe — each engine's thread-local store is independent and starts empty,
matching the existing `coire_session_id/1` precedent.

This is the highest-value fix in this list: it makes every current and future
Ritual example's hand-authored `prolog_clauses` correctly session-isolated by
construction. No caller-side changes needed in any example script, and
`docker restart docker-clara-api-1` between debugging iterations becomes
unnecessary.

**Risk**: medium. Touches the shared clause-loading path every Ritual example
and every direct `/deduce` caller uses. Needs the verification step below
before shipping — worth a second pair of eyes given the blast radius.

### 3. Add `PL_DICT` handling to `term_to_json`

**Files**: `clara-prolog/src/backend/ffi/bindings.rs` (new binding),
`clara-prolog/src/backend/ffi/conversion.rs::term_to_json` (new match arm,
before the `PL_TERM` case — the `PL_DICT` constant already exists at
`bindings.rs:77` but is unused in this match, confirmed by reading the file).

SWI's C API has a purpose-built dict iterator well-suited here:
`PL_for_dict(term_t dict, int (*func)(term_t key, term_t value, void
*closure), void *closure, int flags)` (declared in the vendored
`SWI-Prolog.h:577`, not yet bound in this codebase — `PL_is_dict` and
`PL_get_dict_key` are also available there at lines 595/569 if a simpler
approach is preferred). Bind `PL_for_dict`, add a `PL_DICT => { ... }` arm that
calls it with an `extern "C"` trampoline collecting into a `serde_json::Map`
via a boxed closure context (this codebase already establishes that pattern in
`callbacks.rs`, `coire_bridge.rs`, and `ritual_bridge.rs` — reuse it, don't
invent a new one). For each `(key, value)` pair: reuse the existing
key-stringification logic already written for the `-`/2 case
(`conversion.rs:144-147`), and recursively call `term_to_json(value)` for the
value, same as `PL_LIST_PAIR` already does. Any per-pair conversion error gets
stashed in the closure state and checked after `PL_for_dict` returns.

**This is a real, standalone `clara-prolog` bug independent of this plan** —
worth fixing regardless of what happens with items 1/2/4/5, since it silently
corrupts *any* dict-shaped Prolog solution returned via `/deduce`, not just
this example's.

**Risk**: low-medium. New code path, but additive (only affects the
previously-broken `PL_DICT` case) and isolated to one function.

### 4. Fix `ruminate.pl`'s `extract_hohi_response/2` + add a caveat comment

**File**: `clara-prolog/docs/examples/ruminate/ruminate.pl:28-30`.

Two independent fixes to the same 3-line predicate:
- Wrong field: `Response = Dict.hohi.response.response.` →
  `Dict.hohi.response.content` (the leaf field `clara_evaluate/2`'s reduced
  Hohi envelope actually uses — confirmed live this session).
- The dot-notation itself is fine to leave as-is *in this file* — it's
  normally `consult`ed (line 1: `:- use_module(library(the_cow))`), so SWI's
  compile-time dict-dot-notation expansion applies correctly here. Add a
  one-line comment above `extract_hohi_response/2` warning that this exact
  dot-notation form silently mis-evaluates if copied into a hand-authored
  `prolog_clauses` list (which loads via `assert`, not `consult`) — point at
  `get_dict/3` as the assert-safe alternative. This is exactly the trap this
  example fell into.

**Risk**: negligible. Doc/example file, not a loaded library.

### 5. Document the fixed state in `rituals_101.md`

**File**: `docs/rituals_101.md`, in the existing "Critical implementation
notes" section (starts ~line 299, alongside its existing subsections like "The
`dis_domain` identity trap" and "Patience timeout").

Add one new subsection, written *after* items 1-4 land (documents the fixed
state, not workarounds around it): (a) `prolog-lib`'s commonly-needed
libraries (`the_coire`, `the_rabbit`, `the_cow`) are auto-loaded into every
session — no `use_module` needed — but anything added to `prolog-lib/` later
isn't automatically in that list; (b) hand-authored `prolog_clauses`
predicates are now automatically session-isolated (`thread_local`), so no more
container restarts between debugging iterations, and no clause bleed between
separate Ritual runs; (c) dict dot-notation (`Dict.key.key2`) only works in
normally-`consult`ed library files, never in hand-authored `prolog_clauses` —
use `get_dict/3` there instead.

## Not proposing a fix for

**Nested `/evaluate` wall-clock timing** (bug #2 in the original report):
already correctly handled example-side (`--answer-poll-max-wait-s` default
raised to 180.0). It's real sequential LLM latency from chaining
`ponder_text`/`ruminate_opts` calls, not a bug — nothing to fix upstream.
Worth one line in the new `rituals_101.md` subsection ("nested
`ponder_text`/`ruminate_opts` calls cost real wall-clock time — budget
accordingly") but no code change.

## Known limitation found during verification (item 2)

The `thread_local` auto-declare fix (item 2) only protects predicates loaded
through `consult_string`'s own `assertz` path. It does **not** protect
against a predicate name that has already been compiled as a **static**
procedure via a genuine `consult/1` of a real file — SWI compiles
predicates static by default when loaded that way, and a static procedure
can never be converted to `dynamic`/`thread_local` afterward, in any engine,
for the life of the process. Hit live during this verification: manually
`consult`ing `ruminate.pl` (to validate item 4) defined `extract_hohi_response/2`
as static in the shared global module; every subsequent `/deduce` call
whose `prolog_clauses` tried to assert its *own* `extract_hohi_response/2`
(e.g. the answer example itself) then failed with
`permission_error(modify, static_procedure, extract_hohi_response/2)` until
clara-api was restarted. In practice this only bites if something
`consult/1`s a real file whose predicate names collide with names used in
hand-authored `prolog_clauses` elsewhere — worth a one-line callout in the
new `rituals_101.md` subsection (avoid naming collisions between real
library predicates and hand-authored `prolog_clauses` predicate names), but
not a code fix: the alternative (forcing every `consult/1`'d predicate to
also be thread_local) would break normal library-loading semantics.

## Regression found after shipping, and fixed same day (`73cf2a5`)

**Found 2026-08-20, day after item 2 shipped, building
`lildaemon/goat/app/assistant/`.** The `thread_local` fix did **not**
give true per-`/deduce`-call isolation. Confirmed live: after registering
several different Prolog sources over one evening that each defined their
own version of the same predicate (`assistant_turn/3`, different bodies), a
`/deduce` call referencing the *current* source's `prolog_source_id`
returned a result consistent with an *older*, different version of that
predicate — despite `resolve_prolog_source` only loading the one referenced
source's content. Restarting `docker-clara-api-1` made the identical call
behave correctly, which pointed toward an OS-thread-reuse race.

**That theory was wrong.** The actual root cause, found via a minimal
isolated repro (one fact + one rule, both brand-new predicate names, one
`consult_string` call, one engine, zero possibility of thread reuse — and
it still failed 100% of the time): `consult_string`'s "have I seen this
predicate before" bookkeeping used `functor(T, F, A)` on the whole parsed
clause term. For a **rule** (`Head :- Body`), that returns `:-`/2 — the
rule's own top-level functor/arity, since `:-` is a genuine binary
operator once a clause has a body — not the head predicate's indicator.
So every rule *after the first one* in a multi-predicate source got
bookkept under the same wrong, shared `F/A`, which the "already seen"
check then matched against the first rule's entry, silently skipping
`thread_local`/`retractall` for every subsequent rule. 100%-reproducible,
not timing-dependent — never caught earlier because every prior test used
single-predicate or fact-only cases; `general_assistant.pl`/
`terse_analyst.pl` (each with 4 rule predicates sharing names) were the
first multi-*rule* files ever exercised against it.

**Fixed**: extract `F/A` from `Head` when the parsed term unifies with
`(Head :- Body)`, falling back to `functor(T, F, A)` only for bare facts.
Verified live: the exact failing scenario (10 alternating `/deduce` calls
between the two rulesets, no restart) went from 5/10 to 10/10; the
original fix's own 160-call sequential+concurrent stress test (fact-only
predicates) still passes 160/160 — no regression. Ruleset hot-swapping
(no `clara-api` restart needed) is now confirmed safe. Full writeup:
project memory `thread_local_os_thread_reuse_bug` (now marked RESOLVED).

## Suggested order of work

1 → 2 → 3 → 4 → 5. Items 1-4 are independent of each other and could be done
in parallel by different people, but 5 (the doc update) should come last since
it documents the post-fix state. 2 carries the most risk and is worth its own
review/testing pass separate from the others.

## Verification plan (after items 1-4 land) — completed

All six steps below were run live against the rebuilt stack when items
1-4 shipped; see `docs/ritual_rumination_answer_bugs_found.md`'s status
line and the "Regression found after shipping" section above for the
follow-up verification after the item-2 fix itself needed fixing.

1. Rebuild clara-api and restart the container.
2. Re-run `examples_ritual_rumination_answer.py --skip-ingest` against the
   known pre-seeded `adhoc.how-to-store-fresh-basil-to-keep-it-long`
   workspace, with the (now-unnecessary) `use_module` directives still in
   `answer_step`'s `prolog_clauses` — confirm it still converges cleanly
   (regression check).
3. Remove the explicit `use_module` directives from
   `examples_ritual_rumination_answer.py`'s `build_answer_prolog_clauses()`
   and re-run — confirm it still works (validates item 1).
4. Submit two back-to-back `/deduce` calls directly via `curl`, using the same
   predicate name but a different body in each — confirm the second call's
   result reflects only its own body, with **no** container restart in
   between (validates item 2 / clause isolation).
5. Re-run the full two-phase pipeline once more and inspect the raw
   `/deduce` JSON response for `Citations` — confirm real nested JSON
   objects, not a stringified blob (validates item 3).
6. Consult `ruminate.pl`'s predicates directly (not via `prolog_clauses`) and
   confirm `extract_hohi_response/2` now returns the right text (validates
   item 4).
