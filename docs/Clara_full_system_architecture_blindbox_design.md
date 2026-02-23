Clara full system design sync for full engine cycle integration
-- State of the Bedlam --

Here's where we are...

Projects: 
Little Daemon, python, ~/Development/lildaemon
  * the FieryPit REST server is defined here which manages Evaluator instances.
  * Evaluators specialize interaction with internal and external reasoning resources (here, our CLIPS and Prolog engines Ollama or Grok etc.)
  * bleat REPL and muBleat are our CLI like interfaces into a running FieryPit instances.  it lets us load and interact with evaluators their sessions, etc.
  * a LilDaemonClient is available in python here and in Rust under Cerebellum.  it provides access to the FieryPit REST API for dealing with daemons and devils (Evaluators).
  
Dagda, python - ~/Development/dagda
  * the Dagda helps us in training and exploring the system behaviour
  * it includes the CodexCobbler a one file little session context search and browser prototype for searching coversations, bookmarking intersing conversations, etc.  we'll use this data later for training Clara herself.
  * it has classes for generating (some using external models) synthetic training data 
  * it produces training data for our fastText classfier clara_classify which is made available in Prolog and CLIPS
  * the dagda fastText model collapses pairs of prompt/response question/answer values into expression/TruthValue
  * our system recognized four valued truth: true, false, unresolved, and unknown (unresolved means the q/a pair either made no sense in terms of truth or that the question or answer is too open to resolve right now) (unknown however just means we don't know and haven't looked, possibly yet).
  
Clara's Cerebrum, rust, c, c++, prolog, CLIPS - ~/Development/clara-cerebrum
  * homes declarative and deterministic resources for Clara.  
  * provides the DaemonicVoice REST API for dealing with LilDevils and their resources: Prolog and CLIPS engines, the Coire (the message persistent store for inter-engine propagation of facts/rules)
  * clara-toolbox provides tooling for Prolog and CLIPS callbacks with arbitrary JSON rpc tooling.  we use it mostly for interacting with LLM's from within a declarative (Prolog or CLIPS) session.
  

  So, we can currently do this:
  
  * lilDaemon client connects to a FieryPit (port 6666)
  * the client requests setting an evaluator ("kindle"")
  * the specific evaluator, running in the FieryPit will create a Prolog and a CLIPS session through 
  * clara-cerebrum - (port 8080) manages a shared in memory database of event mailboxes between the engines.
  * the prolog evaluator triggers the consultation of any additional resources outside of the_rabbit and coire language support in Prolog and CLIPS for things like clara-evaluate, clara-classify, etc.
  * the client makes api requests through the lildaemonclient such as evaluate on a prompt, etc.
  * when the client makes an evaluate request through a new *deduce* message type (passed through the client to Evaluator by way of a Fish) it kicks of the new reasoning/deduction loop as outlined below.
   
  In essence goal -> evaluator -> prolog/deduce -> kick off the deduction cycle in clara-cerebellum via a new endpoint/command in the DaemonicVoice API -> deduction engine/cycle accessing its coire-adapter (there's one for CLIPS as well) -> within the prolog-coire adapter, pull any pending fact assertion/retraction and rule introduction from the store (Coire) -> prolog engine runs -> push new fact assertions/retractions and new predicate definitions to -> CLIPS -> picks up the new facts rules -> CLIPS runs until completion picking up new events as it cycles including fact assertion/retraction and new rule definitions -> push onto Coire for -> Prolog continuing until the master "goal stack" run by Rust but mediated by CLIPS is empty or the reasoning loop is interrupted (timeout or signal).

  Here is an example:

  1. client connects to fierypit and sets the kindling evaluator (which has Ollama LLM capabilities together with Prolog and CLIPS evaluator functions).
 
  2. initial rules and facts are loaded (we'll flesh out other sources but for now assume the client will seed them)
  3. the client submits an evaluate request to the KindlingEvaluator using "deduce" in JSON payload as the command (see how other Evaluators in the KindlingEvaluator filter out their commands).
  4. the KindlingEvaluator uses the DaemonicVoice protocol with a new deduce endpoint to trigger the engine cycle regardless of whether a given client is dual mode or not.  we may push other events into the Coire event store so it may be valid to "pump" the metaphorical reasoning accelerator pedal until something turns over. ;0)
  5.  the deduction cycles through the prolog/event/clips/event/possible other evaluators supporting "deduce" in the circle, back to the controller to test for end of deduction then potentially back around or return a result.
  6.  since we're modelling "deduce" as a special form of "evaluate" or "prompt" returning a Tephra from the evaluator makes sense.  we should provide status updates asynchronously to the evaluator to check on the state of a running deduction but collect the result only after its done.  let's leave open querying the individual Prolog or CLIPS engines belonging to the evaluator, etc. through the DaemonicVoice API and or pushing that up through the Evaluator to the client via the LilDaemonClient and FieryPit.
    
  Maybe this.. deduce returns a descriptor JSON to the calling LilDaemonClient.  it includes a deduction key we can use to find state info on the participants of the deduction (evaluator through FieryPit and/or prolog and clips through DaemonicVoice).
  
  We can poll (for now) for the completion or failure of a given deduction.  If its complete, we pick up the Result through the API which should be a Tephra (having hohi success or tabu error data).
  
  --------------------- SAMPLE DEDUCION LOOP
  So, if we have the following starting Prolog:
  
  man(stan).
  has_plan(stan).
  man_with_the_plan(Man) :- man(Man), has_plan(Man).
  
  we can start with no goal, kick off deduce.
  
  * Clara's rust layer initializes the engine run and kicks off prolog with the facts and rules seeded by the evaluator or configuration
  * the prolog system runs but has no goal.  however, when we consulted our prolog source it defined two facts and a rule.  we push those onto the message store for CLIPS.
  * Seed the incoming events into the environment for CLIPS
  * CLIPS starts its turn, sees the waiting events and pushes new facts and rules into CLIPS.
  * CLIPS forward chains, find the match for the two facts on the man_with_the_plan rule, pushes "man_with_the_plan" with no variable substitutions onto Coire.
  * CLIPS fires any other rules or events and takes action, pushing potentially zero or more goals onto the stack.
  * Clara's rust layer does an evaluator checkin with any peers (we'll start with none) then checks the goal stack.
  * If the stack is empty or we have been interrupted we return the current possibly incomplete but marked-so result.
  
  ------------
  
  The following is a transcript of a "blind box" conversation with Claude online regarding the broader semantic layer (the overlaid reasoning loop).  The model did NOT have access to our current implementation, so where our current architecture disagrees with the transcript favour our decisions (keep the good) or raise a question during our planning phase.
  
  Our first step is to unify the broader ideas in the following transcript with the reality of the current system.  It should provide some low level hints which will be useful when we implement our plan, however.
  Please review the transcript, the referenced projects (they're part of this full system architecture), and we'll iterate over the document until it is consistent, coherent, and actionable.
  
-- RECENTLY ADDED FRAMEWORKS --
We recently implemented extended functionality in the Coire to support this activity (the full multi engine multi evaluator reasong cycle).

Details can be found in the docs directory of clara-cerebrum:
./docs/COIRE_PROLOG_FRAMEWORK.md
./docs/COIRE_CLIPS_FRAMEWORK.md

These give some hooks and prolog/CLIPS language hints and utility ideas as well as providing for programmatic emit and poll for events within the Prolog and CLIPS engines during processing of a query or CLIPS run.

We've done some additional work regarding automagically mapping between Prolog <-> CLIPS fact assertions/retractions and non-recursive rules.  Remember, control is in the Rust layer.  The engines know nothing about each other but receive updates to our "picture of truth".
  
--- THE FOLLOWING is the transcript --
  

# Clara — Full System Architecture

## Overview

Clara is a heterogeneous reasoning system with two major components: **Clara Cerebrum**, a Rust server that owns the core reasoning cycle (SWI-Prolog, CLIPS, and Coire), and the **FieryPit**, a Python REST server that hosts and manages intelligent evaluator agents called LilDaemons and LilDevils. The two systems communicate over the **DaemonicVoice** REST protocol. 🔥

---

## System Map
```
┌─────────────────────────────────────────────────────────────────────┐
│                         Clara Cerebrum (Rust)                       │
│                                                                     │
│  ┌─────────────┐  ┌─────────────┐  ┌──────────────────────────┐   │
│  │ SWI-Prolog  │  │    CLIPS    │  │   CycleMember slots       │   │
│  │  Engine     │  │   Engine    │  │   (LilDaemon proxies)     │   │
│  └──────┬──────┘  └──────┬──────┘  └────────────┬─────────────┘   │
│         └────────────────┴──────────────────────┘                  │
│                           │                                         │
│                       ┌───┴───┐                                     │
│                       │ Coire │                                     │
│                       └───────┘                                     │
│                                                                     │
│              Reasoning Cycle Controller                             │
│                  REST API (DaemonicVoice)                           │
└────────────────────────┬────────────────────────────────────────────┘
                         │  DaemonicVoice (REST)
┌────────────────────────┴────────────────────────────────────────────┐
│                        FieryPit 🔥 (Python)                         │
│                                                                     │
│  ┌──────────────────┐        ┌──────────────────────────────────┐  │
│  │ LilDaemonClient  │        │     Pit Residents                │  │
│  │  (management)    │        │                                  │  │
│  └──────────────────┘        │  LilDaemon        LilDevil       │  │
│                              │  (logic-based)    (LLM-based)    │  │
│                              │  LilDaemon        LilDevil       │  │
│                              │  ...              ...            │  │
│                              └──────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Component Glossary

| Name | Role | Runtime |
|---|---|---|
| **Clara Cerebrum** | Core reasoning server — owns cycle, engines, Coire | Rust |
| **Coire** | Unified shared store — sole inter-component communication channel | Rust (in-process) |
| **FieryPit** | REST server — hosts, creates, and manages evaluator agents | Python |
| **LilDaemon** | LLM-based evaluator agent (generative, neural reasoning) | Python |
| **LilDevil** | Logic-based evaluator agent (rules, classifiers, symbolic reasoning) | Python |
| **LilDaemonClient** | Management interface for FieryPit residents | Python |
| **DaemonicVoice** | REST protocol between FieryPit and Clara Cerebrum | HTTP/JSON |

---

## Clara Cerebrum (Rust)

Owns and drives all core reasoning. Unchanged from core architecture — Prolog, CLIPS, Coire, cycle controller, C-level hooks. Additionally exposes a DaemonicVoice REST API so FieryPit residents can participate in the reasoning cycle and manage sessions. (( deduction cycle, "deduce" command ))

### Session Management via DaemonicVoice

LilDaemons and LilDevils are first-class participants — they can create and work with Prolog and CLIPS sessions on the Cerebrum directly. The Cerebrum exposes session endpoints:
```
POST   /sessions/prolog          → create a Prolog session, returns session_id
POST   /sessions/clips           → create a CLIPS session, returns session_id
DELETE /sessions/{id}            → tear down a session
POST   /sessions/{id}/assert     → assert facts into session
POST   /sessions/{id}/query      → run a Prolog query or CLIPS run
GET    /sessions/{id}/wm         → retrieve current WM snapshot
POST   /sessions/{id}/rules      → install dynamic rules
GET    /sessions/{id}/goals      → retrieve goal queue state
POST   /cycle/register           → register a LilDaemon as a CycleMember
POST   /cycle/pass/{member_id}   → Cerebrum calls this on each cycle pass
GET    /cycle/coire/snapshot     → pull current Coire state
POST   /cycle/coire/push         → push evaluator results into Coire
```

A LilDaemon can therefore manage its own private Prolog or CLIPS session for internal reasoning, while also participating in the shared Cerebrum reasoning cycle through Coire.

---

## FieryPit 🔥 (Python)

A Python REST server that is the home of all evaluator agents. Responsible for:

- **Spawning** LilDaemons and LilDevils on demand
- **Lifecycle management** — start, stop, pause, health check
- **Session brokering** — residents call Cerebrum over DaemonicVoice to create and work with Prolog/CLIPS sessions
- **Exposing** each resident's pass endpoint so Cerebrum can invoke them during the cycle

---

## LilDaemon vs LilDevil

Both are FieryPit residents and both implement the same interface toward the Cerebrum. The distinction is internal reasoning strategy.

### LilDevil (logic-based just a version of Daemons with Prolog or CLIPS instead of LLM backend)

Uses symbolic or statistical methods — rule engines, classifiers, similarity search, structured inference. Deterministic or near-deterministic. May manage its own private Prolog or CLIPS session on Cerebrum for internal reasoning.

### LilDaemon (LLM-based)

Uses a language model for evaluation — semantic scoring, natural language inference, generative reasoning. Non-deterministic. May use Cerebrum Prolog sessions to ground its outputs in structured facts before returning results.

---

## Reasoning Cycle — Full Picture
```
loop {

    // Prolog derivation — C-hook syncs Coire before WAM executes
    cerebrum.prolog_pass()

    // FieryPit residents — Cerebrum calls each registered member
    for resident in cycle_members {
        response merge with deduce(currentDeductionPrompt) in cycle // just leave space for other evaluators to participate, we'll implement later
    }

    // CLIPS forward chain — C-hook syncs Coire before EnvRun
    cerebrum.clips_pass()

    if agenda_empty
        && no_pending_goals
        && no_pending_evaluations
        && fixpoint()
    {
        break Converged
    }

    if interrupt    { break Interrupted }
    if max_cycles   { break Error }
}
```

LilDaemons and LilDevils are invoked between Prolog and CLIPS passes by default, so NLI scores and classifications are available to CLIPS rules within the same cycle that produced the underlying facts.

* we can probably use the Coire for this, pushing metrics onto the shared but tagged message database and collecting the full stats at the end of deduction.
* 
---

## Prolog / CLIPS Session Lifecycle for Residents

A resident can manage its own Cerebrum sessions independently of the shared cycle.  We may wish to gate out of band updates to between engine invocations to avoid threading issues.  This just means an evaluator running outside of the deduction cycle can still write to its resources but those updates should be applied at reasonable times during the engines' cycles.

Private sessions are invisible to the shared cycle — they exist purely to support a resident's internal reasoning. Only what the resident explicitly pushes to Coire participates in the shared reasoning cycle.

---

## C-Level Engine Hooks (Cerebrum Internal)

Both engines are built from source with Coire sync hooks at inference entry points. This is Cerebrum-internal and invisible to FieryPit.

<< THE BELOW IS BLINDBOX CODE AND MAY NOT FIT BUT THE C HINTS ARE USEFUL >>

### Hook Interface
```c
typedef struct {
    void *coire_handle;
    int (*pre_inference)(void *coire_handle, EngineId engine);
    int (*post_inference)(void *coire_handle, EngineId engine);
} CoireHooks;

void prolog_install_coire_hooks(CoireHooks *hooks);
void clips_install_coire_hooks(CoireHooks *hooks);
```

### SWI-Prolog — `pl-fli.c`
```c
qid_t PL_open_query(module_t ctx, int flags, predicate_t pred, term_t t0) {
    if (g_coire_hooks && g_coire_hooks->pre_inference)
        g_coire_hooks->pre_inference(g_coire_hooks->coire_handle, ENGINE_PROLOG);
    // original body...
}

int PL_close_query(qid_t qid) {
    int rc = original_close_query(qid);
    if (g_coire_hooks && g_coire_hooks->post_inference)
        g_coire_hooks->post_inference(g_coire_hooks->coire_handle, ENGINE_PROLOG);
    return rc;
}
```

### CLIPS — `agenda.c`
```c
long long EnvRun(Environment *theEnv, long long runLimit) {
    if (g_coire_hooks && g_coire_hooks->pre_inference)
        g_coire_hooks->pre_inference(g_coire_hooks->coire_handle, ENGINE_CLIPS);

    long long fired = original_env_run(theEnv, runLimit);

    if (g_coire_hooks && g_coire_hooks->post_inference)
        g_coire_hooks->post_inference(g_coire_hooks->coire_handle, ENGINE_CLIPS);

    return fired;
}

// Also hooked so dynamic rule metadata reaches Coire immediately
int EnvBuild(Environment *theEnv, const char *buildStr) {
    int rc = original_env_build(theEnv, buildStr);
    if (rc && g_coire_hooks && g_coire_hooks->post_inference)
        g_coire_hooks->post_inference(g_coire_hooks->coire_handle, ENGINE_CLIPS);
    return rc;
}
```

---

<< THIS IS IMPORTANT TO GET RIGHT>>
We don't worry about the order of execution in the logic engines.  We allow the Rust process to control the deduction cycle and coordinate through the Coire between and among sessions.

Rules and facts are bidirectional between Prolog and CLIPS and represent our "working knowledge" of the World.
Our larger truth system includes values for false, true, unresolved, and unknown.

We have a proof of how this works but its outside the scope here.  The main point is that rules are always non-recursive and facts are just assertions and retractions of truth values some of which can be unknown or unresolved.

## Fact / Rule Mapping Reference

### Ground Facts
```prolog
person(stan, 42, engineer).     →     (person (name stan) (age 42) (role engineer))
employed(stan).                 →     (employed stan)
```

### Non-Recursive Predicates → CLIPS Rules
```prolog
adult(X) :- person(X, Age, _), Age >= 18.
```
```clips
(defrule derive-adult
  (person (name ?x) (age ?age) (role ?))
  (test (>= ?age 18))
  =>
  (assert (adult ?x)))
```

### Mapping Table

| Prolog | CLIPS |
|---|---|
| Base fact | `deffacts` or `assert` at init |
| Rule head (derived fact) | `assert` on RHS |
| Body literal | LHS pattern |
| Arithmetic condition | `(test ...)` |
| Negation `\+` | `(not ...)` on LHS |
| Multiple clauses, same head | Multiple rules, same conclusion form |
| Side-effecting call | RHS action |
| Goal propagation | Assert `goal` fact on RHS |
| Disjunction `;` | Split into multiple rules |

---

## Boundary Semantics

| Layer | Responsibility |
|---|---|
| **Prolog** (Cerebrum) | Epistemic — four-valued truth, NLI-informed derivation, rule generation |
| **CLIPS** (Cerebrum) | Action — forward chain on committed facts, goal lifecycle, side effects |
| **LilDaemons** (FieryPit) | Signal/logic — symbolic scoring, classification, structured inference |
| **LilDevils** (FieryPit) | Signal/neural — LLM scoring, NLI, generative evaluation |
| **Coire** (Cerebrum) | Membrane — sole communication channel between all components |
| **DaemonicVoice** | Protocol — REST/JSON bridge between Cerebrum and FieryPit |

---

## Key Design Properties

**Engines are stateless between cycles.** All durable state lives in Coire. C-level hooks hydrate engines on demand.

**Coire is the sole communication channel.** No component calls another directly. Cerebrum mediates everything.

**LilDaemons and LilDevils are first-class cycle members.** They implement the same pass interface as Prolog and CLIPS from the controller's perspective.

**Residents can own private sessions.** A LilDaemon or LilDevil may create and manage its own Prolog or CLIPS sessions on Cerebrum for internal reasoning. Only explicit Coire pushes participate in the shared cycle.

**Dynamic rule generation is live.** Prolog synthesizes CLIPS rules from meta-reasoning. Coire serializes and installs them via `EnvBuild` before the next CLIPS pass.

**Fixpoint detection is structural.** Cycle converges when Coire snapshot delta is empty, agenda is empty, no pending goals, and no pending evaluations.

**The FieryPit is independently scalable.** Residents are Python processes managed by a REST server. New LilDaemons or LilDevils drop in via `LilDaemonClient` without touching Cerebrum source.

Our branch for this feature is ghost_watch ;0)
