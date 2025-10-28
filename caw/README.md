# CAW Language - Prototype Implementation

**CAW** (Cognition with Agentic Wisdom) is a translational rule-based language designed to bridge CLIPS expert systems with modern distributed AI agents.

## Overview

This is a **prototype implementation** of the CAW language, supporting:
- **Phase 1**: CLIPS-compatible DSL with transpilation to CLIPS
- **Phase 2 (planned)**: Distributed agent messaging, vector databases, and AI-powered reasoning

## Architecture

```
CAW Source (.caw)
    ↓
    | PEG Parser (pest)
    ↓
AST (Abstract Syntax Tree)
    ↓
    ├─→ Runtime Execution
    └─→ CLIPS Transpilation
```

## Current Features

### ✅ Implemented

1. **Parser** - Complete PEG grammar for CAW syntax
   - Type declarations (primitives, records, unions, vectors, functions)
   - Fact declarations (feather)
   - Rule declarations (rune)
   - Agent declarations (Expert)
   - Expression parsing (literals, identifiers, function calls, message sends)

2. **AST** - Full Abstract Syntax Tree representation
   - Programs, statements, expressions
   - Type expressions with support for complex types
   - Rule and fact definitions

3. **Type System** - Basic type checking infrastructure
   - Type environment for binding names to types
   - TypeChecker for expression validation

4. **Runtime** - Basic execution engine
   - Program loading and execution
   - Fact storage and retrieval
   - Rule registration
   - Agent registry

5. **CLIPS Transpiler** - Convert CAW to CLIPS syntax
   - Type definitions → deftemplate
   - Facts → assert
   - Rules → defrule

### 🚧 In Progress / Planned

- [ ] Full rule evaluation engine
- [ ] Message passing between agents
- [ ] Vector database integration for distributed facts
- [ ] Session management for agent communication
- [ ] CSP-style channels (tell/ask)
- [ ] Metadata and confidence tracking for facts
- [ ] Rule tracing and debugging

## Language Syntax

### Type Declarations

```caw
type Particle = {
  type: String,
  state: String,
  mass: Number
}

type Status = "stable" | "unstable" | "decaying"

type Numbers = [Number]

type Transformer = (Number, Number) => Number
```

### Fact Declarations (Feathers)

```caw
feather radium: Particle = {
  type: "radium",
  state: "unstable",
  mass: 226
}
```

### Rule Declarations (Runes)

```caw
rune "DecayLaw" when
  particle.state == "unstable"
then
  assert Particle(state: "decaying")
```

### Agent Declarations (Experts)

```caw
let albert = Expert(Physics.Nuclear.Particle._)
let marie = Expert(Chemistry.Nuclear._)

// Message passing (planned)
marie.research(radium) ! albert.analyze _
```

## Building and Running

### Build the CAW crate

```bash
cargo build -p caw
```

### Run the example

```bash
cargo run --example parse_and_eval -p caw
```

### Example Output

```
=== CAW Language Demo ===

Parsing CAW program...

Source:
type Particle = {
  type: String,
  state: String
}

feather radium: Particle = {
  type: "radium",
  state: "unstable"
}

let albert = Expert(Physics.Nuclear._)

✓ Parsed successfully!

Program structure:
  Statements: 2

Executing program...

✓ Execution successful!

Runtime State:
  Facts: 0
  Rules: 0
  Agents: 1
    - albert: Physics.Nuclear._
```

## Project Structure

```
caw/
├── src/
│   ├── lib.rs              # Main library exports
│   ├── ast.rs              # AST node definitions
│   ├── parser.rs           # PEG parser implementation
│   ├── types.rs            # Type system
│   ├── runtime.rs          # Execution engine
│   ├── transpiler.rs       # CLIPS transpilation
│   └── caw.pest            # PEG grammar
├── examples/
│   ├── parse_and_eval.rs   # Example program
│   └── simple.caw          # Sample CAW source
└── Cargo.toml
```

## Grammar (PEG Specification)

See `src/caw.pest` for the complete grammar.

### Key Rules

- `program` - Sequence of statements
- `statement` - Type, agent, feather, or rune declaration, or expression
- `type_decl` - Type definition
- `agent_decl` - Agent (Expert) instantiation
- `feather_decl` - Fact declaration
- `rune_decl` - Rule definition
- `expression` - Literals, identifiers, function calls, message sends

## Naming Conventions (Irish/Raven Mythology)

- **feather** - Fact/assertion
- **rune** - Rule
- **flock** - Group of agents
- **cairn** - Knowledge capsule or module
- **skyline** - Communication layer
- **echo** - Message receipt or reply
- **veil** - Namespace or domain boundary

## Next Steps

1. **Complete Rule Evaluation** - Implement pattern matching and rule firing
2. **Message Passing** - Add CSP-style tell/ask semantics
3. **Vector DB Integration** - Connect to Qdrant/Weaviate for similarity-based fact retrieval
4. **Session Management** - Persistent agent state across messages
5. **Confidence & Metadata** - Track fact provenance and confidence scores
6. **CLIPS Integration** - Full bidirectional transpilation and execution

## Dependencies

- `pest` - PEG parser
- `serde` / `serde_json` - Serialization
- `thiserror` - Error handling
- `log` - Logging
- `anyhow` - Error context

## References

- PEG Parser: https://pest.rs/
- CLIPS Expert System: https://clipsrules.sourceforge.io/
- CSP Messaging: https://en.wikipedia.org/wiki/Communicating_sequential_processes
- CAW Design Document: `./docs/CAW\ lang\ design1.txt`

## Author

Implemented as a prototype for the Clara Cerebrum project.
