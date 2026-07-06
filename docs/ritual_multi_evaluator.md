Let's enter planning mode and ensure we have full support for interoperability of graphs of evaluators in a Ritual.
Specifically, let's design the following test around rituals.  If the current system doesn't support the activity, we'll note that and iterate on the design.

1. We've already shown a single deduce capable evaluator converging on a reasoned response during performance (Run) of a Ritual.
2. Let's add a non-deduce capable evaluator on the same FieryPit as the first which will provide an alternative answer.
    - current example of reasoned_response tries to answer the prompt using the built in Clara LLM at the predicate level.
    - we'll add a Groq variation of the ClaraMindSplinter evaluator (one exists and is actually the default for Cobbler sessions) to the Ritual
    - we'll consult Groq on the prompt as well as using our ponder predicate
    - to achieve this, we'll need an edge for Offerings between the two nodes (Clara and Clara/Groq).
    - drawing that edge in the Ritual editor should draft Prolog and/or CLIPS rules which detect the incoming Offering (initial) on the Clara evaluator, fire placing the Offering on the Coire to be picked up by the Clara/Groq evaluator, the Clara/Groq evaluator returns its Tephra also on the Coire, the first Clara evaluator picks up the result Tephra from Clara/Groq, uses its internal clara ponder/etc. predicates to synthesize a combined answer, and returns that as the converged result.
