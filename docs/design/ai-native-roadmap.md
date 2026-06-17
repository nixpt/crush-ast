# Crush AI-Native Language Design Roadmap

> **Thesis**: Most Crush programs will be written by AI agents, not humans.
> Design for the agent as primary author; the human is director and reviewer.
>
> Current date: 2026-06-17 (s302)

---

## The Problem: Agents Navigate Codebases Badly

When an AI agent works on a codebase it faces a set of structural problems that
don't exist for human developers who carry context in memory across sessions.
These were observed directly while building crush-ast in s302.

### 10 Root Causes

**1. File is the wrong unit of abstraction**
Files are organized for human editors. A 850-line `scheduler.rs` contains 3 functions
relevant to any given task. The agent reads all 850 lines to find them. The meaningful
unit for an agent is a *semantic chunk* — a type, a protocol, a subsystem — not an
arbitrary file boundary.

**2. Invisible contracts (the WHY lives nowhere)**
Code encodes WHAT it does; it rarely encodes WHY it was written that way.
`Rc<RefCell<...>>` encodes a single-threaded design decision that must be inferred.
"No context-switch mid-borrow-mut" is a system invariant that lives in no file —
it's in the developer's head until it's violated and tests catch it.

**3. Relationship discovery is O(N), not O(1)**
"What calls `need_array`?" → grep → filter false positives → open each result →
determine call context. This is a multi-step exploration when it should be a single
query. Same for: what depends on module X, what changed recently, what does this
type flow into downstream.

**4. The reading DAG problem**
To understand module A, the agent needs type B from module B, which uses concept C
from module C. By the time the chain is followed, the original context of A is lost.
There is no way to ask "summarize what module A needs me to know about module B" —
the agent must read all of B and extract the relevant parts manually.

**5. Exhaustive match discovery**
Adding `Value::Handle` required updates at 12+ sites across 6 files (`stdlib.rs`,
`bus.rs`, `caps.rs`, `portable_vm.rs`, `scheduler.rs`, `vm.rs`). Rust's exhaustive
match caught the missing arms — but there was no way to know in advance how many sites
existed or where they were. Non-exhaustive languages (JS, Python) would produce silent
bugs at runtime.

**6. Test-to-code mapping opacity**
There is no way to ask "what covers this function?" or "what tests break if I change
this behavior?" — the agent must read all test files and infer. `docs/tasks/vm-pipeline-gaps.md`
documents 18 untested error paths, but there is no way to query "which error paths have
zero coverage?" from the codebase itself.

**7. Dead vs alive ambiguity**
Are these imports still needed? Is this function still called? The agent must grep.
`Arc`/`Mutex`/`JoinHandle` sat unused in `vm.rs` silently until a compiler warning
surfaced them — after the file had been read multiple times.

**8. No semantic search — only structural**
An agent can grep for strings. It cannot ask "where does this codebase handle the case
where a thread is waiting for another?" — that is a semantic query. Every semantic
question costs full file reads plus judgment.

**9. Naming ambiguity at scale**
`run` exists in `vm.rs` and `scheduler.rs`. At scale, string search becomes unreliable.
Concept-tagged names would let agents search by semantic role, not string match.

**10. No global change surface awareness**
At session start the agent does not know what changed since last time. `git log` gives
commits, not "what behaviors changed and what invariants were touched."

### Root Pattern

The codebase has structure, relationships, and contracts — but they are **implicit**,
scattered across files, extractable only by reading. Agents pay the full reading cost
every session. Humans amortize this through memory; agents start fresh.

---

## Existing Partial Solutions

| Tool | Layer | What it addresses |
|---|---|---|
| `dejavue` | Temporal | Why decisions were made; architectural history across sessions |
| `crush-symbols` | Structural | What symbols exist (extraction, heuristic, post-hoc) |
| **`crush-index` (proposed)** | **Semantic** | **Cross-file relationships, from compiler, always authoritative** |

dejavue and crush-symbols are addons layered on top of source. `crush-index` would be
authoritative — produced by the same compiler pass that generates bytecode, never stale.

---

## The Design: Contracts as First-Class CAST Nodes

Make contracts, relationships, and invariants **explicit language constructs** — not
documentation that gets stale, but compiler-checked nodes in the CAST that agents
query for free.

### Proposed Annotations

```crush
// Every module declares itself — compiler enforces presence
@module {
    purpose: "cooperative green-thread scheduler for CVM1"
    exports: [run_scheduled, StepAction]
    invariants: ["no-preemption-mid-instruction", "rc-refcell-not-send"]
    related: [vm.types, bytecode.opcodes]
}

// Sum types track all exhaustive-match sites
@exhaustive-match-sites Value

// Functions declare their error surface and access pattern
fn execute_one(thread: &mut GreenThread, ...)
    @errors [StackUnderflow, StepQuota, BadJump, UnknownOpcode]
    @reads  [thread.ip, thread.stack, thread.call_stack]
    @writes [thread.ip, thread.stack, thread.out_parts]
    @does-not-write [program]

// Tests declare what they cover
@covers VmError::StackUnderflow
fn test_stack_underflow() { ... }

// Invariants are named and scoped, not free-text comments
@invariant "rc-refcell-not-send" {
    applies_to: [execute_one, run_scheduled]
    reason: "Rc<RefCell<...>> is not Send; cooperative scheduling prevents re-entrancy"
    consequence: "spawn_parallel must deep-clone array/map values at OS thread boundary"
}
```

Every annotation is a typed CAST node. It cannot be ignored by the compiler, cannot
go stale silently, and is queryable at runtime via host caps.

---

## The `crush-index` Crate

A new crate that consumes CAST from all compilation units and builds a cross-referenced
index. Authoritative because it comes directly from the compiler, not from heuristic
source extraction.

**Index contents:**
- Symbol table: name → file, line, signature, `@module` manifest
- Call graph: function → {callers, callees}
- Dependency graph: module → {imports, importers}
- Invariants: named invariant → {applies_to, reason, consequence}
- Coverage map: error path / code path → `@covers` test (or absence)
- Exhaustive match sites: type → all match-on sites
- Change feed: integration point for dejavue timeline

**Storage:** SQLite for queryability; JSON export for portability.

---

## The `codebase.*` Host Capabilities

Host caps that expose the index to Crush programs running as agents. An agent running
in the CVM1 calls these caps instead of doing filesystem operations.

```
codebase.modules()
    → [{name, purpose, file, exports, invariants}]
    -- full workspace map; fits in ~20 context lines for a typical project

codebase.callers("fn_name")
    → [{file, line, context}]

codebase.definition("module.symbol")
    → {file, line, signature, errors, reads, writes, invariants}

codebase.invariants("module_name")
    → [{name, reason, consequence, applies_to}]
    -- agent reads this before touching the module

codebase.uncovered_paths()
    → [{error_variant, file, line}]
    -- all error paths with no @covers test

codebase.exhaustive_sites("TypeName")
    → [{file, line, missing_arms?}]
    -- all match sites; flags missing arms before compilation

codebase.semantic_search("natural language query")
    → [{module, relevance_score, purpose}]
    -- ranked by purpose-field similarity, not string match
```

---

## Agent Session Protocol (Target State)

When an agent starts working on a Crush codebase, it should be able to:

```crush
// 1. Get workspace map — O(1), fits in context
let map = codebase.modules();

// 2. Get invariants for modules it will touch — before reading source
let inv = codebase.invariants("scheduler");

// 3. Find all exhaustive-match sites for a type it will modify
let sites = codebase.exhaustive_sites("Value");

// 4. Check coverage before adding new error paths
let gaps = codebase.uncovered_paths();

// 5. Find callers before changing a function signature
let callers = codebase.callers("execute_one");
```

Total context cost: ~50 lines. Current cost for the same information: 3000+ lines of
source reading, multiple grep rounds, manual inference.

---

## Build Order

| Step | Work | Crate |
|---|---|---|
| 1 | Spec `@module`, `@invariants`, `@errors`, `@covers` as formal CAST node types | `crush-cast` |
| 2 | Parser: recognize annotations in Crush source | `crush-frontend` |
| 3 | Compiler: emit annotation nodes into CAST output | `crush-frontend` |
| 4 | `crush-index`: consume CAST → build SQLite index | new crate |
| 5 | `codebase.*` cap implementations over the index | `crush-lang-sdk` |
| 6 | Host cap provider wired into default SDK runtime | `crush-lang-sdk` |
| 7 | `@exhaustive-match-sites` compiler tracking + warning | `crush-frontend` |
| 8 | dejavue ↔ crush-index integration (change feed) | `crush-index` |

Steps 1–3 are pure language additions; no VM changes. Steps 4–6 are new infrastructure
that doesn't touch existing code. Step 7 is a compiler lint. Step 8 connects the
temporal layer (dejavue) to the semantic layer (crush-index).

---

## Connection to Agent-Native Opcodes

The `ai.*` host caps (`ai.query`, `ai_tool_chain`, `ai_agent_delegation`) planned
for the CVM1 are the *execution* layer of agent-native design. The `codebase.*`
caps and `crush-index` are the *navigation* layer. Together:

- Navigation layer: agent understands what exists and what to touch
- Execution layer: agent acts on that understanding via AI capabilities

Both are required for a Crush program to be a fully autonomous agent.
