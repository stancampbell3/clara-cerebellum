# SWI-CLIPS Integration - Planning Questions

## 1. SWI Prolog & CLIPS Integration Approach
The doc mentions tapping into C source code directly. Are we building/linking against SWI and CLIPS from source as part of this workspace, or wrapping pre-installed system libraries? I want to understand if we're vendoring the C sources or using `-lswipl` / `-lclips` style linking.

**Answer:** Our SWI and CLIPS integration builds from C source and we include the binaries in our deployment.

## 2. Relationship to Existing `fiery-pit-client`
The existing crate talks to lildaemon over REST. Is this new Ember evaluator something that lives *inside* lildaemon (Python side), or are we building a new Rust-native evaluator that directly embeds SWI+CLIPS, bypassing lildaemon for reasoning? Or does lildaemon orchestrate and delegate to this new Rust crate?

**Answer:** Each engine is agnostic about the consumers of its events. Engines are associated by session_id (a GUID). SWI engine writes a message with its session_id to the in-memory DB; CLIPS engine using the same session_id picks up the event. The event routing tool (named after the Cauldron of the Dagda - "Coire") handles only storing and fetching by session_id, leaving producer/consumer logic one level up. This design allows mailboxes to span machines when we get to the "hard gossip" feature later.

## 3. Scope for Today
The brainstorming section has a lot of pseudocode flagged as "NOT REQUIRED IMPLEMENTATION METHODOLOGY." (This is a flag not to take the suggestions as gospel - just brainstorming.)

What's the concrete deliverable you want to plan toward?

**Answer:** Set up the `clara-coire` crate, establish event definitions, table definitions, verify it builds, initialize the database, and test a write-read-write-read cycle unit test on the Coire (no engines involved).

## 4. DuckDB Crate Choice
The pseudocode references `duckdb-sys` but there's also the higher-level `duckdb` Rust crate. Any preference, or should we evaluate?

**Answer:** The pseudocode is just chasing dragons. Claude has a more complete understanding. A factor of 42 - however, an irrational and likely imaginary one.
