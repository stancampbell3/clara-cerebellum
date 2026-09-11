Copilot’s stream-oriented pipeline treats these components as continuous background queues. Framing them instead as a **bounded Meta-Ritual (a "ritual of rituals")** provides significantly better structural control, determinism, and state clarity.

In this model, an overarching task runs as a long-running, goal-oriented Meta-Ritual session managed by the **Ego**. The Ego acts as the executive chair, spawning, mediating, and terminating child sub-rituals deterministically rather than reacting to infinite ambient topics.

---

## Architectural Framework: Meta-Ritual & Sub-Rituals

### Sub-Ritual Deconstruction

* **Id Sub-Ritual (The Divergent Phase):** A bounded generation session spawned on ephemeral Kafka topics (e.g., `ritual.id.<session_id>`). Id agents generate raw, unconstrained solution candidates in parallel until a specific proposal quota, timeout, or entropy threshold is met.
* **Ego Sub-Ritual (The Grounding Phase):** The Ego ingests the raw Id proposals, querying DuckDB for current state/history and `the_cow` (Edgequake) for structural RAG context. Using CLIPS/Prolog rules, it prunes invalid outputs, resolves context, and synthesizes viable candidates into formal Robert's Rules "Motions."
* **Superego Sub-Ritual (The Deliberation Phase):** The Ego submits the formalized motion to the `/deduce` endpoint. A Robert's Rules cohort debates, amends, and votes on the motion over Kafka ritual channels using formal logic rules and policy constraints.

---

### Lifecycle & Termination Controls

* **Epoch-Based Execution:** The Meta-Ritual runs in discrete epoch cycles, preventing runaway execution loops while allowing multi-step problem solving over extended timelines.
* **Stateful Synchronization:** DuckDB records intermediate state transitions and task progress per epoch, while `the_cow` graph updates with approved action nodes to inform subsequent Id proposal cycles.
* **Termination Criteria:** The Meta-Ritual gracefully exits when either the Superego passes an explicit "Motion to Adjourn" (objective achieved) or a hard epoch/token limit is exhausted.

---

This recursive sub-ritual structure enforces clean boundaries between generation, grounding, and deliberation while granting the Ego clear executive authority over session lifecycles.
