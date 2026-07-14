# Crush as a JavaScript Runtime

> Why Mallika doesn't need Node.js, Deno, or Bun — Crush compiles her JS to native,
> and V8 handles the dynamic parts when she needs them.

---

## The Problem: Mallika's Stack Is a Frankenstein

Mallika's game engine today:

```
┌─────────────────────────────────────────────────────────┐
│  Node.js process                                         │
│                                                          │
│  game-server.js     ← Express HTTP server                │
│  physics.js          ← 18ms/frame, her hot path          │
│  ai-inference.py     ← subprocess, 3ms IPC overhead      │
│  websocket-relay.js  �� real-time player sync             │
└───────��─────────────────────────────────────────────────┘
                          │
           ┌──────────────┼──────────────┐
           ▼              ▼              ▼
     Python subprocess   Redis pub/sub   SQLite worker
     (serialize/deserialize per call)

Total runtime: 21ms/frame. Breakdown:
  18ms physics (JS, single-threaded)
   3ms IPC overhead (Node <-> Python subprocess)
   + Redis latency, SQLite blocking, event loop stalls
```

Mallika's core complaint isn't just "my JS is slow." It's "I have four
different runtimes duct-taped together and the tape costs 3ms per frame."

## The Crush Answer

Crush doesn't replace Node.js by being a faster Node.js. It replaces the
entire multi-runtime architecture with **one compilation unit**:

```
┌─────────────────────────────────────────────────────────┐
│  ONE native binary (libgame.so)                          │
│                                                          │
│  fn_handle_request()    ← was game-server.js (C syntax)  │
│  fn_update_physics()    ← was physics.js (JS syntax)     │
│  fn_predict_action()    ← was ai-inference.py (Python)   │
│  fn_relay_ws()          ← was websocket-relay.js (JS)    │
│                                                          │
│  All four in the same CASM. All four in the same .so.    │
│  Calling fn_predict from fn_update is PUSH + CALL + PC.  │
│                                                          │
│  6ms total per frame. No subprocess. No IPC. Zero tape.  │
└─────────────────────────────────────────────────────────┘
```

---

## Architecture: Three Paths for JS

### Path A: Compiled JS (Walker → AOT C) — PRIMARY

For **application code** — your business logic, hot loops, data processing.
Code that you control, that doesn't change at runtime, that benefits from
ahead-of-time optimization.

```
.js source ──→ SWC parser ──→ AST
                                      │
                                      ▼
                               lower_swc.rs (1459 lines)
                                      │
                                      ▼
                               crush_cast::Program (CAST)
                                      │
                                      ▼
                               crush-frontend::Compiler
                                      │
                                      ▼
                               CASM (bytecode)
                                      │
                    ┌─────────────────┼─────────────────┐
                    ▼                 ▼                  ▼
                 CVM1 (dev)      AOT C (prod)      AOT Rust
                 interpreter     gcc -O3 -flto     rustc LTO
                 20ms/sieve      native .so        native .so
```

### Path B: V8 Fallback (for dynamic JS the walker can't lower)

For **unsupported JS constructs** — `eval()`, `Proxy`, prototype
chains, `with`, generators, async iterators. Opt-in via `crush-v8-fallback`
feature flag.

```
JS source ──→ SWC parser ──→ Try lower to CAST
                      │
                      │ (Failed: "eval() not supported")
                      │ (Failed: "Proxy not supported")
                      │
                      ▼ (if --feature crush-v8-fallback)
               V8 Isolate (one-time creation via snapshot)
                      │
                      ▼
               Direct eval or module load
                      │
                      ▼
               Result → unified error model
               (same capability gates as compiled path)
```

### Path C: Surfer/Boa DOM Bridge (for web content JS)

For **web `<script>` tags** — DOM manipulation, event handling, browser APIs.
Already mature in surfer-browser (70 tests, SURF-H..R shipped).

```
<script> in HTML page ──→ Boa parser + interpreter ──→ DOM bridge
```

---

## Why V8 Instead of Boa for the Fallback?

Our earlier design proposed Boa as the fallback engine. Surveying rusty_v8
(Deno's V8 binding) and exosphere's multi-engine architecture changed this:

| Concern | Boa | V8 (via rusty_v8) |
|---------|-----|-------------------|
| Spec conformance | ~90% ES2023 | Full ES2024 |
| Speed | Interpreter only | TurboFan + Maglev JIT |
| GC | Experimental tracing | Generational, production-hardened |
| Binary cost | 0MB (pure Rust) | ~20MB (prebuilt static lib, feature-gated) |
| Build complexity | Cargo dependency | Downloads prebuilt binaries from GitHub |
| Snapshot support | No | Yes (sub-millisecond isolate creation) |
| Inspector (DevTools) | No | Full Chrome DevTools |
| Node.js compat | None | V8 + Node API = drop-in compat |
| TypeScript support | No | No (swc handles TS at the parse layer) |

Boa's interpreter-only design means if Mallika falls through to the fallback
path, her user mods run 3-5× slower than Node. V8's JIT closes that gap —
the fallback is as fast as running in Node.js directly.

---

## How Crush Would Use rusty_v8

| Component | rusty_v8 feature | Crush integration |
|-----------|-----------------|-------------------|
| Isolate management | `v8::Isolate::new()` | One isolate per crush runtime |
| Op bindings | `FunctionTemplate` / `ObjectTemplate` | Adapt crush's native function registry |
| Fast calls | `CFunction` + `CFunctionInfo` | Route AOT functions through V8 Fast API |
| Snapshot | `SnapshotCreator` | Pre-bake AOT functions + stdlib at build time |
| Module loading | `Module::instantiate_module()` | Hybrid: AOT as SyntheticModule, dynamic via V8 |
| Error handling | `TryCatch` | Unified error model between AOT and JIT |
| Inspector | `V8Inspector` | Optional: Chrome DevTools for fallback code |
| Memory | `Weak<T>` + finalizers | Cleanup native resources when JS objects GC'd |

**The snapshot advantage:** At build time, crush pre-compiles AOT functions
into a V8 snapshot. At runtime, loading the snapshot creates an isolate in
~1ms (vs ~50ms for a cold Node start). Dynamic scripts run alongside pre-baked
AOT functions in the same isolate — same heap, no IPC.

**Feature gate:** `crush-v8-fallback` is opt-in. Pure AOT crush stays lean
(~2MB binary). Add the feature for V8 (+20MB). Mallika's server deployment
can afford 20MB; her WASM/embedded targets use pure AOT.

---

## What We Learned from Exosphere

Exosphere (sibling project in `/workspace/projects/exosphere/`) took the
opposite approach — embed 4+ full JS engines (Boa, quick-js, V8, SpiderMonkey)
plus Bun/Node/Deno subprocess fallbacks. Their internal docs recommend
**consolidating to crush-ast's parser-only pattern**:

From `crush_ast_peer_architecture.md` (exosphere internal, 2026-06-16):

> The crush-ast peer repo proved a parser-only approach: all frontend
> languages lower into CAST, and a single CrushVM owns execution.
> Benefits over exosphere's embedded-VM approach:
> - One sandbox — one quota system, one capability gate, one scheduler
> - Smaller dependency graph — no 137K-line boa_engine
> - Consistent replay — all execution is CrushVM bytecode
> - Capabilities at the language level — FeatureReport blocks dangerous imports

From `polyglot-runtime-consolidation.md` (exosphere internal, 2026-06-25):

> Exosphere currently embeds 4+ full VM runtimes each with its own GC,
> object model, sandbox, and dependency tree. Recommend migrating to the
> crush-ast parser-only pattern.

**Key patterns we should borrow from exosphere:**

1. **`SandboxPolicy → Deno permissions` mapping** — a clean 1:1 translation
   from capability policies to runtime permissions. Crush's `casm::Manifest {
   permissions }` can mirror this for uniform security across both compiled
   and fallback paths.

2. **`Capsule.toml` manifest** — a declarative TOML specifying language, entry
   point, required/optional capabilities, and resource limits. Cleaner than
   command-line flags for complex polyglot projects.

3. **`PayloadFormat` auto-detection** — extension-based dispatch to the right
   walker/runner. Crush already has this via `WalkerRegistry`.

4. **The `ScriptRunner` subprocess model** — clean encapsulation of the
   subprocess lifecycle (command building, env isolation, timeout, capabilities
   as env vars). Useful for when native system runtimes are needed.

**What we should NOT borrow:**

- Don't embed `boa_engine` as the default — 137K lines, ICU conflicts
- Don't use `quick-js` for production — lightweight but incomplete
- Don't maintain 4+ JS backends — exosphere's own docs say this is a burden

---

## The Compilation Pipeline (Today)

The `crush-lang-js` crate implements two parsing backends producing the same CAST:

| Backend | Parser | Feature Flag | Handles |
|---------|--------|-------------|---------|
| **swc** (default) | `swc_ecma_parser` v41 | none | JS, JSX, TS, TSX, MTS |
| **boa** (optional) | `boa_parser` v0.21 | `boa-backend` | JS only |

### What Each Lowerer Handles

| Construct | SWC | Boa | Notes |
|-----------|:---:|:---:|-------|
| `function` declarations | ✅ | ✅ | Both generate `FunctionDef` / `Function` |
| `var`/`let`/`const` | ✅ | ✅ | Both generate `VarDecl` |
| `if`/`else` | ✅ | ✅ | Standard `If` branching |
| `for`/`while` loops | ✅ | ✅ | Desugared as needed |
| `switch` | ✅ | — | SWC: lowered to `If` chains |
| `try`/`catch`/`finally` | ��� | — | SWC: `TryCatch` with handler |
| `throw`/`break`/`continue` | �� | — | SWC: `Throw`/`Break`/`Continue` |
| `class` | ✅ | ✅ | SWC: lowered to object + method lambdas |
| arrow functions | ✅ | ✅ | Both: `Lambda` |
| template literals | ✅ | ✅ | Both: concatenated `StringLiteral` |
| JSX | ✅ | ❌ | SWC only: lowered to `createElement` calls |
| TypeScript types | ✅ | ❌ | SWC only: stripped silently |
| `import`/`export` | ✅ | ��� | SWC: `ImportStatement` / `Export` |
| `console.log` | — | ✅ | Boa: mapped to `io.print` capability |
| `await`/`yield` | ✅ | ✅ | Both: `Await`/`Yield` expressions |
| generators | ✅ | ✅ | Both: generator function marker |
| compound assignment (`+=`) | ✅ | ✅ | Both: desugared to binary + store |

---

## What Mallika Gets vs Node.js/Deno/Bun

| Concern | Node.js | Deno | Bun | Crush |
|---------|---------|------|-----|-------|
| **Startup time** | ~50ms | ~30ms | ~10ms | **~1ms** (native .so, no VM init) |
| **Runtime overhead** | V8 JIT + GC | V8 JIT + GC | JSC JIT + GC | **None** (AOT compiled) |
| **Memory (baseline)** | ~20MB | ~15MB | ~10MB | **~2MB** (no JIT code cache) |
| **Cross-language calls** | IPC/subprocess | IPC/subprocess | IPC/subprocess | **Same stack frame** |
| **Suitable for** | Servers, scripts | Servers, CLI tools | CLI tools, bundling | **Hot loops, game engines, embedded** |
| **Package ecosystem** | npm (2M+) | npm + JSR | npm | **None needed** (compiles from source) |
| **JS spec conformance** | Full ES2024 | Full ES2024 | Full ES2024 | **~80% walker subset + V8 fallback** |
| **TypeScript** | Via ts-node/swc | Native | Native | **SWC backend strips TS types** |
| **JSX** | Via Babel/swc | Via swc | Via swc | **SWC backend lowers JSX** |
| **DOM APIs** | jsdom | Web APIs | Native | **Boa + surfer DOM bridge** |
| **Debugger** | --inspect | --inspect | --inspect | **V8Inspector (fallback path)** |

---

## The Roadmap

| Phase | Status |
|-------|--------|
| JS → AOT C (single file) | ✅ Works today |
| Multi-file polyglot merge (JS + Python + C → one .so) | 🚧 CAST merge exists, needs CLI |
| `crush-walk-run` JS support (add to capability registry) | ⬜ Same pattern as Python path |
| V8 fallback (feature-gated, snapshot-based) | 📋 Design complete |
| V8 Inspector integration (DevTools for fallback code) | 🔮 Future |
| Node.js API shim (`require('http')` → `CAP_CALL`) | 🔮 Design needed |
| Boa/V8 → CASM JIT bridge | 🔮 Future research |
| Full ES2024 via walker (no fallback needed) | ❌ Walker subset only |

## Exosphere ↔ Crush Synergy

| Exosphere component | Crush equivalent | Complement |
|---------------------|-----------------|------------|
| Boa as embed JS | Parser-only SWC/boa | Crush avoids embedding a second VM |
| `quick-js` in NanoVM `@js {}` | CAST IR CASM | Crush's IR is simpler, more AOT-friendly |
| V8 in bliss-core (feature-gated) | Proposed V8 fallback | Same pattern: opt-in heavy engine |
| `Capsule.toml` | `casm::Manifest` | Crush's manifest is simpler, could adopt Capsule.toml |
| `ScriptRunner` subprocess | `crush-aotc --extern` | Exosphere's model is cleaner for native runtimes |
| Deno sandbox flags | Capability manifest | Same 1:1 mapping pattern |
| `PayloadFormat` auto-detect | `WalkerRegistry` | Both extension-based, complementary |
