# Crush Gaps — What it Takes to Support Everything Natively

Generated from a full cross-cutting audit of walkers, compilers, both VMs,
and both codegen backends.

## The Four-Layer Stack

```
┌─────────────────────────────────────────────────────────┐
│ Layer 1: Walkers (Python, JS, Rust, C, Go, Bash, ...)   │
│          Source → CAST AST                                │
├─────────────────────────────────────────────────────────┤
│ Layer 2: Compiler (crush-frontend)                       │
│          CAST → CASM bytecode                             │
├─────────────────────────────────────────────────────────┤
│ Layer 3: Execution (CVM1 / FastVM / JIT / AOT)          │
│          CASM → result                                    │
├─────────────────────────────────────────────────────────┤
│ Layer 4: Codegen (Rust AOT / C AOT)                      │
│          CASM → .so                                       │
└─────────────────────────────────────────────────────────┘
```

Each layer has its own gaps. But the critical insight is: **they compound**.
A feature missing in Layer 1 means Layer 2-4 never see it. A feature
compiled in Layer 2 but missing in Layer 3 means it fails at runtime.

## Priority 0: Layer 3 (Execution) — the Foundation

If the VMs can't run a CASM instruction, nothing above them matters.

### CVM1 (PortableVm)

**Complete.** All 46 bytecode opcodes have real implementations.
8 built-in capabilities registered. Zero stubs. This is the gold standard.

### FastVM

78 of 79 FastOps implemented. Two gaps:

| Gap | Impact | Fix difficulty |
|-----|--------|---------------|
| `ArrSet` missing from FastOp enum | Cannot write to array elements in FastVM | ~50 lines — add `ArrSet` variant, implement in execution.rs |
| `CrossLangCall` stubbed (always errors) | Polyglot cell execution blocked | Requires hooking into subprocess/polyglot runtime |
| No AI opcodes in FastOp enum | 16 AI ops compile but can't run | ~200 lines — add AI variants as FastYield::Request(HostRequest::...) |

### JIT (Cranelift)

Only handles ~20 ops. No arrays, no strings, no caps, no functions.
JIT is for math loops only. Fixing this is a full rewrite.

### AOT (Rust and C codegen)

Only 28 of ~46 opcodes handled. Massive gap:

| Missing category | Ops |
|------------------|-----|
| Bitwise | `bitand`, `bitor`, `bitxor`, `bitnot`, `shl`, `shr` |
| Stack manipulation | `rot`, `pick`, `roll` |
| Type operations | `typeof`, `cast` |
| Arrays | `arr_get`, `arr_set`, `arr_len`, `arr_push`, `arr_pop`, `make_range` |
| Maps | `new_obj`, `set_field`, `get_field` |
| Strings | `str_contains`, `str_split`, `str_replace`, `str_join` |
| Control flow | `break`, `continue` |
| Concurrency | `spawn`, `yield`, `await` |
| Polyglot | `exec_lang` |
| Capabilities | `cap_call` (stubbed, pops args and returns Null) |
| Exceptions | `enter_try`, `exit_try`, `throw` |

## Priority 1: Layer 1 (Walkers) — What Source Languages Can Come In

### Python walker

11 expression types bail, 14 statement types bail. **Cannot handle:**

- Classes, `with`, `match`/`case`, exceptions (`try`/`except`/`raise`)
- List/dict/set comprehensions, generators, lambda, decorators
- Slicing (`arr[:mid]`), f-string interpolation, `is`/`in` operators
- `global`, `nonlocal`, `del`, `assert`, type annotations
- Async iterators/context managers

**What works:** arithmetic, variables, if/while/for loops, function defs,
function calls, print, comparisons, boolean logic, array/list literals,
dict literals, basic indexing, augmentation (`+=`).

### JavaScript walker (SWC)

**Essentially complete for expression lowering.** No structural bail-outs.
The only operator gap is `Math.*` builtins (no polyfill layer).

For statements: `with` silently skipped, `debugger`/`empty` no-op'd.
Import/export handled through module system. Switch, try/catch, for-in,
for-of, classes — all lowered.

**JavaScript is the most complete walker by far.**

### Rust walker

Only basic functions, assignments, arithmetic, and if/while/for. **Cannot handle:**

- Structs, enums, impls, traits, modules, macros (except `println!`)
- Closures, match expressions, patterns, array/tuple/struct literals
- Async/await, `?` operator, unsafe, `let` chains, loops as expressions

### Other walkers

| Walker | Verdict |
|--------|---------|
| Bash | Good shell coverage, no `trap`, `select`, here-docs |
| Zsh | Better than Bash — `repeat`, `try`/`always`, arrays |
| C | Functions + control flow, no structs/enums/switch/goto/casts |
| Go | Minimal: only functions, vars, expressions, if, return |
| WASM/Zig/Nepali | Stubs |

## Priority 2: Layer 2 (Compiler) — CAST → CASM

The compiler handles nearly all CAST constructs. The gaps are:

| CAST construct | Compiler status |
|----------------|----------------|
| `Match` expression | ✅ Full lowering (compiled to if-else chain on match arms) |
| `AI` statements/expressions | ✅ Compiles to CASM — but CASM's `to_opcode()` rejects all 16 AI opcode strings with `UnknownOpcode` |
| `DomMutate` / `DomEventListener` / `DomQuery` | ✅ Compiles to CASM — same `UnknownOpcode` rejection |
| `Pipeline` | ✅ Handled for Bash/Zsh pipe desugaring |
| `Spawn` | ✅ Handled — desugars `async fn` to inner function + spawn wrapper |

**The compiler emits valid CASM JSON for all constructs.** The problem is
that `Instruction::to_opcode()` in `casm/src/lib.rs` rejects 19 opcode
strings (16 AI + 3 DOM) with `UnknownOpcode`. These get serialized as
raw string opcodes in the CASM JSON, and the VMs' lowering passes handle
them as generic `cap_call` fallthroughs — but they never resolve because
the capabilities aren't registered.

## Priority 3: Layer 2.5 (CASM → OPCODE mapping)

The CASM crate's `Instruction::to_opcode()` has a two-tier problem:

1. **19 opcode strings have no matching arm** → `CasmError::UnknownOpcode`
2. These 19 are compiler-emitted but never make it to the VM because
   the opcode enum doesn't recognize them

The 19 broken opcode strings: `dom_mutate`, `dom_event_listener`, `dom_query`,
`ai_query`, `ai_tool_chain`, `ai_agent_delegation`, `ai_learning_loop`,
`ai_context_aware`, `ai_semantic_match`, `ai_synthesize`, `ai_goal_decl`,
`ai_progress_update`, `ai_knowledge_share`, `ai_capability_discovery`,
`ai_adaptation_request`, `ai_semantic_switch`.

## What Would Make Crush Support Everything

### Tier 1 (lowest effort, highest impact)

| Fix | Lines | Unlocks |
|-----|-------|---------|
| Add `ArrSet` to FastOp + implement in FastVM | ~50 | Array writes in FastVM |
| Add AI+DOM opcode strings to `to_opcode()` match | ~30 | 19 compiler-emitted ops stop failing at CASM parsing |
| Register AI+DOM caps as stubs in CVM1 portable caps | ~40 | These ops run (as NOPs) instead of `UnknownCap` |
| Add `cap_call` implementation to Rust/C codegen | ~100 | AOT codegen handles walked Python/JS with caps |
| Add `range()` lowering to Python walker | ~30 | For loops with `range()` work in walked Python |
| Add `//` division to Python walker `lower_expr` | ~10 | Floor division in walked Python |
| Add slice lowering (`arr[:mid]`) to Python walker | ~40 | Slicing in walked Python |

### Tier 2 (medium effort, unlocks real programs)

| Fix | Lines | Unlocks |
|-----|-------|---------|
| Add array ops to Rust/C codegen (arr_get, arr_set, arr_push, arr_pop, make_range) | ~200 | Algorithms work in AOT |
| Add `try/except/raise` to Python walker | ~100 | Exception handling in walked Python |
| Add comprehensions to Python walker | ~150 | List/dict/set comprehensions |
| Add lambda to Python walker | ~50 | Anonymous functions |
| Add `Math.*` polyfill in JS walker → `cap_call("math.floor", ...)` | ~50 | Math builtins in walked JS |
| Add struct/enum lowering to Rust walker | ~200 | Real Rust programs |

### Tier 3 (large effort, platform capabilities)

| Fix | Lines | Unlocks |
|-----|-------|---------|
| Complete AOT codegen (all 46 opcodes) | ~800 | Full CASM → native compilation |
| JIT expand to cover arrays and caps | ~500 | JIT for real workloads |
| CrossLangCall implementation | ~300 | True polyglot cell execution |
| WASM walker completion | ~500 | WASM → CAST → native |
| Self-hosting: crush-frontend in AOT | ~2000 | Crush compiles itself |

## The Critical Path

If you can only fix one thing today: **add `ArrSet` to FastVM and add array ops to the AOT codegen**. That's ~300 lines and unlocks every algorithm benchmark. Right now arrays work in CVM1 but not in FastVM or AOT. With those fixes, N-Queens, Sieve, and Merge Sort would run end-to-end through the full pipeline at AOT speed.

If you can fix three things: arrays + AI/DOM opcode registration + JS walker Math polyfills. That makes the JS walker genuinely useful and the entire pipeline operational for real programs.
