Let's put together a plan for integration of clara-dagda "predicate cache" into our clara-cycle, our deduction reasoning cycle.

clara-dagda defines a schema as below:

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS dagda_predicates (
    session_id    VARCHAR NOT NULL,
    functor       VARCHAR NOT NULL,
    arity         INTEGER NOT NULL,
    args_json     VARCHAR NOT NULL,
    truth_value   VARCHAR NOT NULL DEFAULT 'unknown',
    updated_at_ms BIGINT  NOT NULL,
    PRIMARY KEY (session_id, functor, args_json)
);
CREATE INDEX IF NOT EXISTS idx_dagda_session
    ON dagda_predicates (session_id);
CREATE INDEX IF NOT EXISTS idx_dagda_functor
    ON dagda_predicates (session_id, functor, arity);
CREATE INDEX IF NOT EXISTS idx_dagda_truth
    ON dagda_predicates (session_id, truth_value);
";

The purpose of clara-dagda is to track the current tableau of predicates, substitutions, and truth values during a clara-cycle.
We'll need to be able to persist the tableau to duckdb similar to how we manage the clara-coire event cache.
This will be more of a cache of complete or in progress deductions, truth assignments, variable bindings, etc.

clara-dagda is not currently integrated, and we can make changes to schema, interface, or implementation as needed.

Please see "./docs/Clara Tableau - Sheet1.pdf" for a brainstorming example of how the tableau might work during a reasoning cycle after the introduction of seed and subsequent assertions.

We'll want to keep an in memory version for a live session and save it along with the deduction snapshot.
Restore should work similaryly.

Please create a plan for integrating the clara-dagda "tableau" feature into clara-cycle, managing the in memory and persistent database appropriately.

If a run doesn't converge in X cycles, and we are using persistent deduction, we'll save the in progress tableau as well as the deduction state.  When we restore one, we restore the other.

We'll modify the criteria for "converged" in the clara-cycle.  Instead of looking at the coire snapshot counts alone, we'll use an agenda consisting of the initial goal, any intermediate goals introduced by forward chaining.

When we are making no more progress regarding the top goal and the agenda is otherwise empty or unchanged since the last cycle, with pending events all processed, then we converge.

Besides providing the goal bindings, we'll want to provide for an "explanation" of how the goal was solved.  This can be a future feature but we need to leave room in our design for it.
