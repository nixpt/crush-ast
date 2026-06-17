# CrushVM × RustPython: Python Frontend Design Archive

> **Status: archival.** This document consolidates a design-conversation series
> (`crushpython.md` … `crushpython9.md`, scratch, triaged into the repo s298)
> exploring how to bring Python into the Crush VM / crush-ast toolchain. It
> preserves the durable rationale and the decisions that shaped the shipped
> design. See **Status (as of 2026-06)** at the end for what is realized vs.
> aspirational.

---

## 1. The Core Idea

**Goal.** Let Crush execute Python under Crush's authority model — Python source
and Python AST run *inside* Crush's capability, scheduling, and replay machinery,
never against the raw host.

The opening spec framed this as embedding **RustPython as a guest language
runtime** ("language capsule, not the root VM"):

```
Crush Program → CrushVM → GuestRuntime ABI → RustPython Runtime
              → Capability Bridge → ScopedHal / VFS / Net / Env
```

Python would reach the host only through `exo.*` built-in modules
(`exo.fs`, `exo.net`, `exo.env`, `exo.log`, `exo.clock`, `exo.cap`), each gated by
a declared capability set. Raw `open()`, `import os`, `import socket`, `eval`,
`exec`, `__import__` are denied unless explicitly granted, enforced by **AST
filtering before execution**.

The core question driving the whole series:

> Can CrushVM host Python safely while remaining the single authority layer?

The conversation's answer evolved significantly from the original embed-a-VM
framing toward a **parser-frontend** framing (Section 5). The non-negotiable
invariant — Python runs only through Crush capabilities — survived intact.

---

## 2. The Three-Lane Python Strategy

RustPython does not replace existing Python paths; it becomes a **third lane**.
Each lane targets a different point on the safety ↔ compatibility spectrum:

| Lane | Pipeline | Use for | Trade-off |
|------|----------|---------|-----------|
| **1. Transpile (CAST)** | `Python → parser → CAST → CASM → CrushVM` | Static, simple Python (`x = 1; y = x + 2; print(y)`) | Safest, capability-native, debuggable through CASM, portable — but limited Python compatibility |
| **2. Embedded (RustPython)** | `Python AST/source → RustPython VM in-process` | Dynamic pure Python that needs no CPython packages (classes, dynamic dispatch) | In-process, Rust-native, easy value bridge — but not CPython, no C extensions |
| **3. Subprocess (CPython)** | `Python → sandboxed subprocess → JSON/IPC bridge` | Real CPython ecosystem (`numpy`, `pandas`, `torch`) | Full compatibility — but heavier, IPC overhead, harder security, weaker object bridge |

The insight: RustPython is the **middle lane** — "not simple enough for CASM, not
heavy enough for subprocess." A layered Python story instead of forcing one
backend to do everything.

### Routing

A router (`crush-python-router`) selects the backend by static analysis:

```
Can lower safely to CAST?      → CAST lane
Else can RustPython support it? → RustPython lane
Else needs CPython/native?      → subprocess lane
```

Users may also force a lane explicitly:
`python backend="native" | "embedded" | "cpython" { … }`.

The structure mirrors modern JS engines: fast path → optimized path → fallback.

---

## 3. Embedded Lane: "RustPython-minimal, not full RustPython"

**Decision:** if the embedded lane is built, use a *stripped* RustPython, not the
whole thing. The guiding rule:

> Embedded RustPython should not pretend to be normal Python.
> It is **"Python syntax inside Crush authority"** — Python-shaped scripting, not
> Python support.

**Minimal pieces to keep:**
- `rustpython-parser` — parse source / AST
- `rustpython-compiler` — AST → RustPython bytecode
- `rustpython-vm` — execute bytecode (core only)
- tiny builtins — `print`, `len`, `range`, `str`, `int`, `bool`, `list`, `dict`
- `exo.*` modules — capability-bridged host access

**Aggressively stripped / disabled:** full stdlib, `importlib`, `socket`/`os`/
process modules, native-extension assumptions, filesystem-backed imports,
threading, multiprocessing, `ctypes`/FFI escape hatches.

**Execution profiles** layer capability surface:

```
crushpy-core     = tiny Python (exprs, funcs, classes, basic builtins, no host imports)
crushpy-exo      = core + exo.fs / exo.log / exo.env + capability bridge
crushpy-compat   = broader RustPython stdlib, still capability-gated
cpython-external = subprocess lane
```

Supporting mechanisms the embedded lane needs:

1. **Import firewall** — no normal imports by default; resolve only from
   `/capsule/lib`, `/exo/modules`, `/manifest/allowed_imports`.
2. **Frozen builtins** — `print → ctx.stdout`; `open`, `eval`, `exec`, `compile`
   denied; `__import__` capability-gated.
3. **Fuel / step budget** — `max_steps`, `max_memory`, `max_time_ms`,
   `max_recursion` (kills `while True: pass`).
4. **Object boundary** — primitives bridge 1:1 (None↔null, dict↔map, etc.);
   everything else becomes `OpaquePyObject(handle)`.
5. **Deterministic mode** — `time.now()` → `exo.clock.now()`, `random` seeded via
   `exo.random`, `uuid` deterministic unless entropy cap granted.
6. **Error mapping** — `SyntaxError → CrushCompileError`,
   `NameError → CrushRuntimeError`, `PermissionError → CapabilityDenied`,
   `RecursionError → LimitExceeded`, timeout → `FuelExhausted`.
7. **Snapshot / replay** — capture stdout, stderr, return value, capability calls,
   fuel used, imports used (valuable for Exosphere / Joker agents).

A Python capsule manifest ties these together:

```toml
[python]
profile = "crushpy-exo"
backend = "embedded-rustpython"

[python.imports]
allow = ["exo.fs", "exo.log", "exo.json"]

[caps]
fs.read = ["/data/input.txt"]
fs.write = ["/out/*"]

[limits]
max_steps = 500000
max_memory = "32MB"
max_time_ms = 250
```

---

## 4. The "One Real VM / One Object Model" Principle

The pivotal realization of the series: **Crush should keep exactly one VM.**

```
source languages → language parsers → CAST → CASM → CrushVM
```

In this model **RustPython becomes a front-end parser/AST provider, not a second
runtime.** The preferred Python path collapses to:

```
Python source → rustpython-parser → RustPython AST
              → py_ast_to_cast → CAST → CASM → CrushVM
```

…with the CPython subprocess remaining only as an escape hatch for native
packages.

**Why this matters — what it avoids:** two garbage collectors, two schedulers,
two debuggers, two object models, two capability systems, two VMs fighting for
authority.

The clean philosophical split:

> RustPython gives Crush **Python syntax**.
> CrushVM gives Python **authority, execution, caps, VFS, scheduling, replay.**

This generalizes. If CAST is the universal IR, then *every* frontend language
lowers into it — Python, Rust, JS, Go, Zig — making CAST Crush's capability-aware
equivalent of LLVM IR / WASM / JVM bytecode. The "walker" concept is renamed to a
**Frontend**:

```rust
trait Frontend {
    fn parse(source) -> Ast;
    fn analyze(ast) -> Features;
    fn lower(ast) -> CastModule;
}
```

…with `crush-lang-python`, `crush-lang-rust`, `crush-lang-js`, `crush-lang-bash`
as implementations. A polyglot `python { … }` block stops being "embedded foreign
syntax" and becomes a Python frontend producing a CAST fragment the Crush compiler
can inspect (symbols, functions, imports), capability-analyze, diagnose, and
optimize *before* CASM generation.

The recommended missing layer is a **Feature Analyzer** between parse and lower:

```rust
struct FeatureReport {
    uses_async: bool, uses_classes: bool, uses_generics: bool,
    uses_exceptions: bool, uses_ffi: bool, uses_imports: Vec<String>,
}
```

so each block routes: `Can Lower? → Yes: CAST / No: reject or fallback`.

---

## 5. GC Policy

Because all frontends lower into one object model, the GC belongs to the
**CAST/CASM runtime + CrushVM heap + capsule object graph** — *not* to any
language frontend.

> Python/Rust/JS/Bash **syntax** must not drag in Python/Rust/JS **memory
> models.** They all lower into Crush values, collected by one Crush GC.

A Python `x = [1,2,3]` and a JS `let x = [1,2,3]` both become a Crush `ListRef`
and are reclaimed by the same collector — no RustPython GC, no JS GC.

**Core model:** arena allocation + tracing GC + capability roots. The unified
value set: `Null, Bool, Int, Float, StringRef, ListRef, MapRef, StructRef,
FunctionRef, ClosureRef, FiberRef, CapsuleRef, OpaqueHandle`. Roots traced: VM
stack, call frames, globals, closures, fibers, capsule state, host handles,
capability table, pending messages. Every heap object implements
`trait Trace { fn trace(&self, tracer: &mut Tracer); }`.

**Phased rollout:** (1) bump arena per execution/fiber, freed at capsule end →
(2) mark-sweep → (3) generational → (4) incremental/concurrent if needed. The
load-bearing part is the **root model**, not the collector sophistication.

**Capsule-aware GC.** Each capsule owns a heap region
(`young / old / extern_handles / roots / limits`), making `heap`, `max_objects`,
`gc_pause_ms` enforceable. Exceeding memory yields a **capability/limit error, not
a host OOM**. Opaque handles to host/subprocess resources are never blindly freed;
the GC enqueues **finalizers** for orderly resource release.

> **Big rule:** do not let frontend languages own memory. Then CrushVM has one GC,
> one heap policy, one replay story.

### GC: "policy brain, not the collector itself"

A later idea explored ML for GC. The decision:

> ML is a **GC policy brain, not the collector.** GC *correctness* must stay
> deterministic: roots → trace reachable → reclaim unreachable. ML must **never**
> decide "this object is unreachable."

ML's legitimate role is *scheduling/tuning* — predicting when to trigger GC, which
capsule will spike, which arena to compact, likely-short-lived objects, minor vs.
major collection, heap-grant sizing. Architecture:

```
Capsule heaps → telemetry → GC Observer → features
→ ML Policy Model → recommendation → GC Controller
→ minor / major / compact / promote / throttle
```

ML output is **advisory only** (`enum GcAdvice { CollectMinor, CollectMajor,
Compact, Delay, ThrottleCapsule, ResizeYoungHeap }`); a deterministic controller
validates against limits before applying (`if advice.respects_limits() { apply }`).
The result is a **GC autopilot, not a GC judge** — a potential differentiator:
capsule-aware adaptive GC.

---

## 6. Validation

One conversation reviewed a screenshot of the actual crush-ast frontend table and
found it **validating the direction**:

```
Python     : tree-sitter-python → rustpython-parser
Rust       : tree-sitter-rust   → syn
JavaScript : tree-sitter-js     → swc_ecma_parser (next)
Bash       : regex+shlex        → brush-parser (planned)
```

This is the shift from `Language → tree-sitter → Walker → CAST` (syntax
translation) to `Language → native parser → real AST → lowering → CAST`
(**frontend compiler architecture**). Native parsers yield *semantic* structure
(`syn`'s `ItemStruct { ident, generics, attrs, fields, visibility }` vs. a flat
tree-sitter node), making lowering dramatically easier. The takeaway: Crush is
"accidentally converging" on an LLVM-like multi-frontend → single-IR design, but
targeting capability-aware CAST/CASM with Exosphere semantics rather than LLVM IR.

---

## 7. Architectural Decisions (summary)

- **One VM, one object model, one GC.** Frontends provide syntax only; CrushVM
  owns authority, execution, caps, VFS, scheduling, replay.
- **RustPython's parser/AST is the high-value piece; its VM is optional.**
  Priority: `rustpython-parser` ✓ / `rustpython-ast` ✓ / Python→CAST lowering ✓
  (high); tiny embedded VM (medium); full RustPython integration (low/avoid).
- **Three lanes, routed by static analysis** with explicit user override.
- **Embedded lane is minimal and capability-gated** if built at all — import
  firewall, frozen builtins, fuel budgets, deterministic mode, error mapping.
- **Capability-first, always.** Host access only via `exo.*` → capability check →
  ScopedHal; AST filtering rejects dangerous constructs before execution.
- **A Feature Analyzer sits between parse and lower** to drive the routing /
  reject / fallback decision.
- **Shipped crate name** settled as `crush-lang-python` (parser + lowering),
  superseding the spec's `crush-python` / `crush-python-bridge` naming.

## 8. Open Questions

- Exactly which Python feature set is CAST-lowerable vs. must fall back
  (classes, exceptions, comprehensions, decorators, `async`/`await` → Crush
  fibers, pattern matching)?
- Is the embedded RustPython lane worth building at all, or do CAST + subprocess
  cover the space? (The series trended toward "parser-only; VM as last-resort
  fallback.")
- IPC/object-bridge design and sandboxing for the subprocess lane.
- Concrete `FeatureReport` thresholds that decide lane selection.
- GC maturity needed before capsule heap limits are enforceable end-to-end; if/
  when the ML policy layer is worth building.

---

## Status (as of 2026-06)

The **parser-frontend direction (Sections 4–6) has shipped.** A
`crush-lang-python` crate exists in crush-ast (`crates/crush-lang-python`,
v0.2.0) and implements exactly the "Python syntax → CAST" model:

- **Realized:** Python frontend backed by `rustpython-parser` / `rustpython-ast`
  (the spec's Mode B parser stage, repurposed as a lowering frontend);
  `parser.rs`, `lower_expr.rs`, `lower_stmt.rs`, `analyzer.rs`; a `python_walker`
  binary integrating with `walker-core` / `crush-frontend`. This is the
  Transpile / CAST lane — the "high-value parser, not the VM" decision, made real.
  The "Frontend" abstraction and the multi-language convergence (Rust via `syn`,
  etc.) are the active architecture.

- **Aspirational / not yet shipped:**
  - **Embedded RustPython lane** (Section 3) — no `rustpython-vm`, compiler, or
    `crushpy-*` profiles in the crate; dependencies are parser/AST only. The
    "RustPython-minimal" engine remains a design, consistent with the series'
    conclusion to favor parser-only with the VM as a possible later fallback.
  - **Subprocess / CPython lane** and the three-way **router** (Section 2).
  - **`exo.*` capability modules, import firewall, fuel budgets, deterministic
    mode, snapshot/replay** as a Python-runtime surface (Section 3) — these
    attach to an execution lane that hasn't been built.
  - **Unified capsule-aware GC and the ML "GC policy brain"** (Section 5) — VM /
    runtime concerns beyond the lowering frontend.

In short: the conversation's central pivot — *use RustPython as a parser frontend
into one CAST/CASM VM, not as a second embedded runtime* — is the part that
shipped. The embedded-VM, multi-lane routing, capability-module, and GC-policy
ideas remain durable design intent for later phases.
