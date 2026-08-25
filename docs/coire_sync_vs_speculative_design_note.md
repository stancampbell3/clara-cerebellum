# Coire's two interaction patterns — read this before fixing the sequential caws_await bug

**Status: design framing note, 2026-08-25.** Written to be read alongside
[`dis_sequential_caws_await_bug.md`](dis_sequential_caws_await_bug.md) (the
repro/evidence writeup) before anyone scopes a fix — the point here is
that the bug reflects a gap in how Coire's *synchronous* messaging
primitive is implemented relative to what Coire as a whole is actually for,
and the fix should be scoped with that in mind rather than as a narrow
patch to make exactly two sequential calls work.

## The observation

Coire is meant to support two structurally different interaction patterns,
not one:

1. **Local activity coordination** — a reasoning process (a Ritual
   participant's own deduction) needs to ask a *specific* other party a
   question and use its *specific* reply to continue reasoning, possibly
   several times in sequence, possibly branching on what comes back. This
   is a conversation the goal itself is directing: "ask local-splinter;
   if that's not enough, ask cow; if that's still not enough, ask Groq."
   The party being asked is known in advance and the reply is meaningful
   only to the asker.

2. **Distributed speculative reasoning** — an agent doesn't know in
   advance who else, if anyone, is reasoning about the same subject
   elsewhere in the Dis domain. It publishes (or polls) a named topic for
   that subject, and *any* other agent — another process, another
   language, joined to no particular Ritual, with no prior coordination —
   can independently notice and pick up what's been asserted there. "I
   see another agent in my Dis domain is talking about my subject area,
   so I can use the citation/fact it already asserted" is exactly this
   pattern: nobody addressed anybody, nobody is waiting on a specific
   reply, the value is in *passive discoverability* of an ongoing,
   shared, asynchronous body of work.

These aren't hypothetical — both already have dedicated, already-shipped
primitives in `clara-prolog/prolog-lib/the_coire.pl`:

| Pattern | Primitives | Design doc |
|---|---|---|
| 1. Local coordination | `caws_offer/4`, `caws_await/2`, `caws_consult/4` (offer+await sugar) | `ritual_typed_edges.md`, `deduction_redux.md` |
| 2. Speculative/discoverable | `coire_topic_create/1`, `coire_topic_publish/2,3`, `coire_topic_poll/2,4`, `caws_squawk/3` | `coire_ad_hoc_topics.md` |

`coire_ad_hoc_topics.md` states pattern 2's intent almost verbatim: "a
research agent can spin up a topic, publish/poll on it, and let other
agents — in another process, another language, or both — discover it
later via `coire_topic_list`, **with no prior coordination**."

## Why this distinction matters for the bug

The two patterns are implemented on structurally different foundations,
and that difference is exactly where
[`dis_sequential_caws_await_bug.md`](dis_sequential_caws_await_bug.md)'s
failure lives.

**Pattern 2 (topic poll) is built on a durable, monotonic offset cursor.**
`coire_topic_poll/4`'s `+SinceOffset, -NextOffset` pair means polling the
same topic twice, from the same offset, is safe and idempotent — it's a
read of a durable log, not a one-shot consumption. That property is
exactly what's needed for Dis's execution model, which re-evaluates the
whole submitted goal fresh on every cycle: re-reading the same topic from
the same offset on every fresh re-evaluation just gets you the same
(or a growing) result, never a lost one.

**Pattern 1 (`caws_offer`/`caws_await`) is built on a single-shot
correlation cache**, not a durable log: `caws_offer_sent/2` (memoizes the
outstanding correlation id by `(Target, Topic, Payload)`),
`caws_result/2`/`caws_failed/2` (the resolved-or-failed cache, populated by
*draining* the Ritual-scoped mailbox via `coire_poll_ritual/2` — see
`the_coire.pl`'s own header comment: "results/failures are cached so a
resolved consult stays resolved"). This was designed, and until now only
ever exercised, for **exactly one round trip per goal**. The moment a goal
needs a *second*, sequential, dependent round trip — ask A, use A's reply
to decide what to ask B — every fresh re-evaluation of the whole goal has
to correctly re-derive *all* previously-resolved correlations *and*
discover newly-resolved ones, atomically, on the same pass. That's
precisely the case the bug doc's repros isolate as broken.

## What this means for scoping the fix

Framed this way, there are two different kinds of fix on the table, not
one:

- **(a) Harden the existing correlation-cache mechanism** so it correctly
  supports an arbitrary *chain* of resolved asks per goal, not just one.
  This keeps `caws_offer`/`caws_await`'s current design and plugs the gap
  in it directly — but whoever does this should deliberately test with
  3+ sequential calls and a call inside a loop, not just 2, since "works
  for exactly two" would just move the undiscovered edge further out.

- **(b) Borrow pattern 2's already-proven mechanics for pattern 1.** Since
  the durable, offset-cursor design underneath `coire_topic_poll/4`
  already tolerates Dis's "re-evaluate everything fresh every cycle"
  model safely — that's the whole reason it was designed that way for
  ad hoc topics — it may be worth asking whether a chained
  `caws_offer`/`caws_await` conversation should be modeled the same way
  internally (each resolved correlation as an appendable, re-readable log
  entry rather than a single-consumption drain into a plain cache),
  instead of inventing a second, bespoke fix for the same underlying
  "must survive repeated re-evaluation" requirement that pattern 2 already
  solved once.

Either is a legitimate answer; the point of this note is only to make sure
that choice gets made deliberately, with both of Coire's intended usage
patterns in view, rather than the fix being scoped purely against "make
`consult_step`'s specific test case pass."

## Not affected / not in scope here

Pattern 2 (topic-based speculative reasoning) is not implicated in the bug
and needs no change — it's cited here only as the existing proof that
Coire's design already knows how to do "safe under repeated
re-evaluation" correctly; it just hasn't been applied to pattern 1's
implementation. `caws_squawk/3` (fire-and-forget, addressed but
uncorrelated) is likewise unaffected — it never awaits a reply, so it has
no analogous "second sequential resolution" case to break.
