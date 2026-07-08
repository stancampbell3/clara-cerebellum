Finding the "heart" of SWI-Prolog can be a bit of a trek because the engine is highly optimized and spans several files to handle different execution modes (like tabling, multi-threading, and engines).

If you are looking for the primary bytecode interpreter loop—the place where Prolog instructions are actually dispatched—you want to look at the **Virtual Machine (VM)** implementation.

### 1. The Core Engine Loop

The main execution loop is located in **`src/pl-vmi.c`**.

Search for the function:
`decode_vmi(Code proto_pc, int flags)`

This is the massive `switch` statement that handles the Virtual Machine Instructions (VMIs). In modern SWI-Prolog, this uses **GCC's "labels-as-values"** (threaded code) for performance, so instead of a standard `switch`, you’ll see a jump table indexed by the instruction.

### 2. The Entry Point (The "CLI" Connection)

If you follow the trail from the command line:

* **`src/swipl.c`**: This contains the `main()` function. It handles command-line arguments and initializes the system.
* **`src/pl-setup.c`**: Handles the initialization of the Prolog stacks and the engine itself.
* **`src/pl-prolog.c`**: This is where the high-level "query" logic lives. Look for `PL_next_solution()`. This function is the C-interface gateway that tells the engine, "Find me the next answer."

### 3. How they connect

The path generally looks like this:

1. **`main()`** (in `swipl.c`) calls initialization.
2. The system starts the interactive toplevel (usually written in Prolog itself, see `boot/toplevel.pl`).
3. When a goal is executed, the C-code calls **`query_loop()`** or **`PL_next_solution()`**.
4. These eventually call **`resume_check()`** or **`execute_goal()`** in **`src/pl-vmi.c`**, which drops into the `decode_vmi` loop to start crunching bytecode.

---

### Pro-Tip for Navigating the Source

Because SWI-Prolog uses a lot of macros to handle different build configurations (like `WANT_VM_PROFILER` or `O_MAINTENANCE`), the code can look dense.

* **Focus on `src/pl-vmi.c**`: If you want to see how `append/3` or a cut `!` actually works at the machine level, this is the place.
* **Look for `Instruction Pointer (PC)**`: The variable `pc` is moved along the code array as the engine executes.

Would you like me to help you break down how a specific Prolog instruction (like a "unify" or "call") is implemented within that `pl-vmi.c` loop?

The engine is a beast, the database (DB) management is surprisingly centralized. In SWI-Prolog, the "Environment" is essentially the **Global Database**, and the logic for adding to it is primarily tucked away in the **`src/pl-db.c`** and **`src/pl-assert.c`** files.

Here is the "shortlist" of functions you should track:

### 1. The Entry Point: `PL_assert()`

Most assertions coming from the C-interface or the internal compiler eventually hit `PL_assert()` in **`src/pl-assert.c`**.

* This function acts as the gatekeeper.
* It handles the conversion of a Prolog term into the internal "Clause" structure.

### 2. The Core Logic: `assertProcedure()`

Inside **`src/pl-assert.c`**, the heavy lifting is done by `assertProcedure()`. This is where the engine decides:

* **Where it goes:** Is this `asserta` (front) or `assertz` (back)?
* **Indexing:** Does the predicate need a new hash table or JIT (Just-In-Time) index? SWI-Prolog is famous for its dynamic indexing, and this is where the triggers live.

### 3. The Low-Level Storage: `link_clause()`

If you want to see how a rule is actually stitched into the linked list of a predicate, look for `link_clause()` in **`src/pl-db.c`**.

* Prolog predicates are essentially a linked list of `Definition` structures.
* `link_clause` performs the pointer manipulation to make the new rule visible to the interpreter.

---

### Important "Under-the-Hood" Concepts

When you're reading this code, keep these two things in mind to avoid confusion:

* **The "Logical Update View":** SWI-Prolog uses a "generation" system. When you assert a fact, it isn't always immediately visible to queries that are *already running*. Each clause has a `generation` stamp. The functions in `pl-db.c` manage these stamps so that a running loop doesn't get stuck in an infinite recursion by seeing the facts it just added.
* **The JIT Indexer:** If you see calls to `update_procedure_index`, that’s the engine realizing, "Hey, this table is getting big, I should probably build a hash map for it."

### Summary Table: Where to look

| Task | File | Key Function |
| --- | --- | --- |
| **High-level API** | `src/pl-assert.c` | `PL_assert()`, `assert_control()` |
| **Clause Creation** | `src/pl-db.c` | `new_clause()` |
| **Linking to DB** | `src/pl-db.c` | `link_clause()` |
| **Retracting** | `src/pl-db.c` | `PL_retract()` |

---

Would you like to look at the **`Clause`** struct definition specifically to see how Prolog rules are actually laid out in memory? (It’s in `src/pl-db.h`).
