# Progressive-consult Ritual example: live verification status

**Status: in progress, paused 2026-08-25 for team review.** Implementation
(`examples_ritual_progressive_consult.py`, Option 1 from
`ritual_progressive_consult_plan.md`, plus the `cow` evaluator) is
committed. Live verification against the real dev stack started today,
found and fixed two real bugs, and is currently blocked on a third,
not-yet-root-caused issue. Nothing has converged end-to-end yet — this
is a checkpoint, not a "verified live" writeup.

## Summary

Implemented the reviewed design (`clara-cerebellum/docs/ritual_progressive_consult_plan.md`,
`lildaemon/docs/ritual_progressive_consult_example.md`) and began live
verification. In order:

1. Reset the dev stack to a clean baseline (see below).
2. Hit and fixed a deployment gap: `cow` wasn't registered in the running
   container (code changes on disk don't reach a baked Docker image).
3. Hit and fixed a real bug in my own new Prolog: a response-shape
   mismatch parsing `caws_await` results.
4. Hit and fixed a real, pre-existing bug in a shared `clara-cerebellum`
   library file (`the_rabbit.pl`), exposed by this being the first caller
   to run `clara_fy` under a tool-calling evaluator's focus.
5. Hit a fourth issue — a missing "classify" tool in a bare one-shot
   `/deduce` session — not yet root-caused. Paused here.

## Dev-stack reset performed

Per direction that resetting rituals/the Edgequake assistant workspace is
acceptable (and expected) on this dev stack, before live-testing:

- **Dis rituals**: 15 active rituals found (13 `assistant-demo`, 2 orphaned
  `rumination-ingest-demo` test artifacts) — all deleted via
  `DELETE /ritual/{id}`, confirmed by team decision to clear everything
  rather than guess which were still-live frontdesk sessions.
- **FieryPit standing participant**: one leftover participant (tied to one
  of the deleted `assistant-demo` rituals) was still running on
  `docker-lildaemon-1`'s own side after the Dis-side delete — cleaned up
  separately via an authenticated `DELETE {fierypit}/ritual/{id}` call.
  Worth flagging on its own: Dis's registry and FieryPit's own
  `RitualManager` can drift apart (a ritual deleted from Dis doesn't
  automatically stop a standing participant still running in lildaemon).
- **Edgequake `assistant.general` workspace**: 17 documents (real content
  from prior background-research runs — pages about "OCSP fallback,"
  "gitlab pipeline fallback caches," etc.) deleted via
  `EdgequakeClient.delete_document`. Confirmed empty afterward.
- **Not touched**: FieryPit/system user accounts — out of scope for
  today's minimal, ad hoc cleanup (team chose "just clear what's needed
  now" over building a standalone reset script). Worth revisiting if a
  proper reset script ever gets built.

## Bugs found and fixed today

### 1. `cow` not registered in the running container (deployment gap, not a code bug)

`docker-lildaemon-1` bakes `config/` and `goat/` into its image at build
time — editing files on the host has no effect on the running container.
**Fix**: `docker cp` the three changed files in
(`config/evaluators.yaml`, `goat/evaluators/custom/cow_evaluator.py`,
`goat/models/EdgequakeClient.py`) then `./clara restart lildaemon` (the
project's proper compose wrapper — see "Deployment notes" below). This is
a **live patch**, not a rebuilt image — see "Outstanding" below.

### 2. `extract_hohi_response/2` shape mismatch for `caws_await` results — fixed, committed

**Root cause**: `RitualParticipant._publish_tephra`
(`lildaemon/goat/models/RitualParticipant.py`) sets a reply envelope's
payload to `tephra.hohi.to_dict()` directly — `{"response": {...}, "code":
200}`, **one** level of nesting. `extract_hohi_response/2` (copied from
`progressive_research.pl`) expects **two** levels
(`{hohi:{response:{...}}}`) — the shape of `ponder_text/2`'s direct
`clara_evaluate/2` FFI-call envelope, a different code path this example
never actually calls (every LLM consult here goes through `caws_offer`/
`caws_await` to a named Ritual participant instead).

This looked at first like a cycle-budget/timing race — convergence
happened within about one poll cycle of the real Hohi being published,
and the failure showed up almost identically across cycle counts as low
as 13 and as high as 21 regardless of `max_cycles`/`patience_cycles`
(900/700 vs the debug run's 60/40 made no difference). Cross-referencing
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
diagnostic (a bare `consult_local` call, no other tiers) returning
Clara's real answer text.

**Status**: fixed in `lildaemon/examples_ritual_progressive_consult.py`.
Not yet committed (uncommitted on top of `ab052a9` as of this writeup).

### 3. `the_rabbit.pl`'s `descriminate/2` family — pre-existing shared-library bug, fixed but not committed

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

**Status**: fixed on disk
(`clara-cerebellum/clara-prolog/prolog-lib/the_rabbit.pl`), live-patched
into `docker-clara-api-1` (`docker cp` + `./clara restart clara-api`),
confirmed the "could not extract response" error is gone. **Not yet
committed** — this is a shared library file the live frontdesk demo also
depends on, worth a second look before committing.

## Current blocker — not yet root-caused

After fix #3, a new, different error surfaced:

```
[ERROR clara_toolbox::ffi] Tool execution error: Tool not found: classify
```

`clara_fy`'s `classify_text/2` needs a "classify" tool (the fastText-based
"Ember Devil" classifier, per `the_rat.pl`'s own docstring) that isn't
available in the bare one-shot `/deduce` session this example creates —
even though `clara_fy` is confirmed working in the live
assistant-demo/`progressive_research.pl` deployment.

**Open questions, not yet investigated**:
- Is "classify" tool registration tied to a session-setup step the
  assistant-demo's runtime performs that this example's plain
  `POST /evaluate {data:{deduce:{...}}}` call doesn't replicate (a
  specific `POST /devils/sessions` parameter, `user_id`, or evaluator
  choice)?
- Is it evaluator-scoped the same way MCP tools are (per-evaluator
  metadata in `evaluators.yaml`), or registered globally in
  `clara-toolbox` and simply unavailable/misconfigured for ad hoc/
  anonymous deduce sessions specifically?

This blocks every tier's `sufficient/3` check, so no live run has
actually converged with a real `Action`/`Answer` yet — tier 1's
`consult_local` alone is confirmed working (fix #2), but the chain as a
whole hasn't cleared this blocker.

## Related architectural note raised today (not investigated further)

Edgequake runs on the same GPU as the rest of Clara (Ollama). Live testing
today showed `local-splinter`'s real Ollama calls taking 3-6 seconds each
for a trivial prompt ("what is 2 + 2?"); a full 4-tier chain reaching tier
4 (web crawl + Groq + multiple Edgequake queries) could meaningfully
contend for GPU time across Clara and Edgequake simultaneously. Worth a
future design discussion on whether Edgequake's own LLM/embedding calls,
or some pipeline legs of this example, should move to Groq or another
remote LLM to avoid blocking Clara's own GPU usage. Flagged, not acted on.

## Deployment notes (for whoever picks this back up)

- Both `docker-lildaemon-1` and `docker-clara-api-1` bake their
  application code into the image at build time — they are **not**
  live-mounted. Use `docker cp <file> <container>:<path>` +
  `./clara restart <service>` (the project's compose wrapper —
  `/mnt/moonpool/Development/clara`, fronting
  `clara-cerebellum/docker/docker-compose.yml`) to live-patch during
  iteration; this does **not** survive a full rebuild or a fresh
  `docker compose up` on another machine/checkout.
- Container runtime env vars come from `clara-cerebellum/docker/.env`,
  **not** `lildaemon/.env` — confirmed different values are set in each
  (e.g. `EDGEQUAKE_BASE_URL`/`EDGEQUAKE_API_KEY`/`ASSISTANT_WORKSPACE_SLUG`
  are unset in the docker `.env`, falling back to `cow`/`edgequakeingest`'s
  own YAML defaults — this matches `edgequakeingest`'s existing, already-
  working configuration, so not a new problem, but worth knowing when
  debugging "why didn't my env var take effect" in this stack).
- Both `EdgequakeClient` and lildaemon's own auth (`ritual-demo` /
  `ritual-demo-pw`, registered once, 409-on-repeat) are safe/idempotent to
  reuse across diagnostic scripts.

## Outstanding / next steps

1. Root-cause the "classify" tool availability gap (the actual blocker).
2. Decide whether/how to commit `the_rabbit.pl`'s fix to
   `clara-cerebellum` — small, additive, well-justified, but touches a
   file the live frontdesk demo depends on.
3. Commit the `extract_caws_response/2` fix to `lildaemon` (currently only
   live-patched + on-disk, per this session's "commit what we have" cycle
   not yet run for this specific fix).
4. Once both fixes are settled, rebuild (not just live-patch) both
   `docker-lildaemon-1` and `docker-clara-api-1` images so they survive a
   normal redeploy rather than being lost on next container recreation.
5. Resume live verification of the full 4-tier chain once the
   classify-tool blocker clears — nothing has converged end-to-end yet.
6. GPU/Groq offload architecture discussion (see above) — separate from
   this example's own correctness, but relevant to how it (and Edgequake
   generally) should be tuned/deployed going forward.
7. Carried over, unrelated to today: `ritual_multi_evaluator_plan.md`
   still needs its own documentation pass (see
   `ritual_multi_evaluator_doc_stale_todo` memory) — still open.

## Files touched today, by commit status

**Committed** (`lildaemon@ab052a9`, `clara-cerebellum@275e599` and
earlier): the original Option 1 implementation, `cow` evaluator,
`EdgequakeClient.query()`, `evaluators.yaml` entry, companion docs.

**Live-patched, not yet committed**:
- `lildaemon/examples_ritual_progressive_consult.py` — `extract_caws_response/2`
  fix (item 2 above).
- `clara-cerebellum/clara-prolog/prolog-lib/the_rabbit.pl` —
  `extract_llm_response/2` fix (item 3 above).

**Config restored, not code changes**: `lildaemon/config/evaluators.yaml`
was blanked by a hung pytest run mid-session and restored via
`git checkout HEAD` + reapplying the `cow` entry — already back to its
committed+`cow`-added state, nothing further needed there.
