# CAW Language REPL - Quick Start Guide

## Starting the REPL

```bash
cargo run --bin caw --release
# or
./target/release/caw
```

You'll see:

```
╔═══════════════════════════════════════════╗
║  CAW Language REPL v0.1.0                ║
║  Type :help for commands, :exit to quit   ║
╚═══════════════════════════════════════════╝

caw [f:0] [r:0] [a:0] >
```

The prompt shows:
- `[f:N]` - Number of facts
- `[r:N]` - Number of rules
- `[a:N]` - Number of agents

## Demo Session 1: Declare Types and Agents

```
caw [f:0] [r:0] [a:0] > type Person = { name: String, age: Number }
✓ Type 'Person' declared [0ms]

caw [f:0] [r:0] [a:0] > let alice = Expert(Human.Knowledge._)
✓ Agent 'alice' created at domain Human.Knowledge._ [0ms]

caw [f:0] [r:0] [a:1] > let bob = Expert(Human.Skills._)
✓ Agent 'bob' created at domain Human.Skills._ [0ms]

caw [f:0] [r:0] [a:2] > :agents
--- Agents ---
  alice: Human.Knowledge._
  bob: Human.Skills._
```

## Demo Session 2: Query and Inspect State

```
caw [f:0] [r:0] [a:2] > :facts
No facts defined

caw [f:0] [r:0] [a:2] > :rules
No rules defined

caw [f:0] [r:0] [a:2] > :help
--- CAW Language REPL Help ---

Built-in Commands:
  :help   - Show this help message
  :facts  - List all facts in the session
  :rules  - List all rules in the session
  :agents - List all agents in the session
  :clear  - Clear the session state
  :export - Export current state to CLIPS
  :exit   - Exit the REPL
```

## Demo Session 3: Work with Rules

```
caw [f:0] [r:0] [a:2] > type Status = { state: String, confidence: Number }
✓ Type 'Status' declared [0ms]

caw [f:0] [r:0] [a:2] > rune "CheckStatus" when
  alice.verify(state)
then
  bob.confirm(state)
✓ Rule 'CheckStatus' defined [1ms]

caw [f:0] [r:1] [a:2] > :rules
--- Rules ---
  CheckStatus
```

## Demo Session 4: Declare Complex Types

```
caw [f:0] [r:1] [a:2] > type Result = { success: Boolean, value: String }
✓ Type 'Result' declared [0ms]

caw [f:0] [r:1] [a:2] > type Values = [Number]
✓ Type 'Values' declared [0ms]
```

## Demo Session 5: Create New Agent with Nested Domain

```
caw [f:0] [r:1] [a:2] > let reasoner = Expert(AI.Reasoning.Inference._)
✓ Agent 'reasoner' created at domain AI.Reasoning.Inference._ [0ms]

caw [f:0] [r:1] [a:3] > :agents
--- Agents ---
  alice: Human.Knowledge._
  bob: Human.Skills._
  reasoner: AI.Reasoning.Inference._
```

## Built-in Commands Reference

### `:help`
Show the help message with all available commands and syntax examples.

### `:facts`
List all facts currently in the session.

### `:rules`
List all rules currently in the session.

### `:agents`
List all agents currently in the session with their domains.

### `:clear`
Clear all session state (facts, rules, agents, types).

### `:export`
Export current session state to CLIPS syntax (for integration).

### `:exit`
Exit the REPL gracefully.

## Language Syntax in REPL

### Type Declarations

```caw
type Name = PrimitiveType
type Name = { field1: Type, field2: Type, ... }
type Name = [ ElementType ]
type Name = Type1 | Type2
type Name = (Type1, Type2) => ReturnType
```

**Examples:**
```
type Count = Number
type Person = { name: String, age: Number }
type Numbers = [Number]
type Status = Boolean | String
```

### Agent Declarations

```caw
let name = Expert(domain.path._)
let name = Expert(domain.path)
```

**Examples:**
```
let alice = Expert(Human.Knowledge._)
let reasoner = Expert(AI.Reasoning.Inference._)
let simple = Expert(Domain)
```

### Rule Declarations

```caw
rune "RuleName" when
  condition
then
  action
```

**Example:**
```
rune "CheckStatus" when
  alice.verify(state)
then
  bob.confirm(state)
```

## Tips for Demo

1. **Start Simple**: Begin with type declarations
   ```
   type Color = String
   type Status = Boolean
   ```

2. **Add Agents Gradually**: Show domain nesting
   ```
   let expert1 = Expert(Physics._)
   let expert2 = Expert(Physics.Quantum._)
   let expert3 = Expert(Physics.Quantum.Mechanics._)
   ```

3. **Use :agents to Show State**
   ```
   :agents
   ```

4. **Show Type System Flexibility**
   ```
   type Name = String
   type Count = Number
   type Items = [String]
   type Complex = { status: Boolean, value: String }
   ```

5. **Demonstrate Error Recovery**
   - Type invalid syntax and see error message
   - The REPL recovers and continues

6. **Use :clear to Reset**
   ```
   :clear
   ```

## Architecture Demo Points

- **Parser**: Demonstrates CAW grammar and single-statement parsing
- **Runtime**: Shows persistent session state across commands
- **AST**: Type system with records, unions, vectors
- **Agent Model**: Domain-scoped expert agents
- **Interactive**: Real-time feedback with colored output

## POC Demo Script (5 minutes)

```bash
# Start REPL
cargo run --bin caw --release

# Type: Intro
type Species = { name: String, habitat: String }

# Create agent
let biologist = Expert(Biology.Taxonomy._)

# Show agents
:agents

# Show help
:help

# Create another agent
let ecologist = Expert(Biology.Ecology._)

# Check agents again
:agents

# Exit
:exit
```

## Troubleshooting

**Issue**: REPL won't start
```bash
# Make sure you're in the clara-cerebrum directory
cd /path/to/clara-cerebrum
cargo build -p caw --bin caw
./target/debug/caw
```

**Issue**: Type parsing fails
- Ensure proper spacing: `type X = { field: Type }`
- No semicolons needed
- Whitespace is flexible

**Issue**: Commands not recognized
- Make sure command starts with `:` (colon)
- Commands are lowercase
- Use `:help` to see all commands

## Next Steps

1. Integrate REPL with web UI
2. Add persistent storage for sessions
3. Connect to vector database for fact search
4. Add rule evaluation with backtracking
5. Implement message passing between agents
