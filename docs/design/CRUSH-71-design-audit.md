# CRUSH-71 — crush-ast design audit

Branch: `agent/panini-crush/CRUSH-71` · Author: panini · Date: 2026-08-02

Captain's directive: explore and improve the design by ~3000x (ambition level,
not a literal gate) — hunt design-level wins across the pipeline
(parser → CAST → CASM compile → portable VM → JIT), grounded in a measured
baseline and a survey of what clients actually consume.

## 1. Baseline

### 1.1 Compile pipeline (`cargo bench -p crush-frontend --bench cast_compile`)

Full raw output: `docs/design/CRUSH-71/baseline-cast-compile-bench.txt`.
20 fixtures, two paths each: `text` (source → lex → parse → semantic →
optimize → compile) and `cast` (CAST JSON → serde decode → semantic →
optimize → compile). Representative rows (p50/p95 in µs, peak heap in bytes):

```
fixture,path,p50_us,p95_us,peak_heap_bytes
09,text,14,18,36729
09,cast,17,18,36417
13,text,16,17,26058
13,cast,25,26,26058
16,text,38,61,48625
16,cast,48,50,48625
19,text,39,41,46704
19,cast,53,56,46704
20,text,69,118,83880
20,cast,88,95,83880
20,breakdown,lex_p50_us=7,parse_p50_us=6,semantic_p50_us=11,optimize_p50_us=3,compile_p50_us=18
```

Two immediate observations:

1. **The CAST-JSON path is ~30–40% slower than parsing raw source.** Decoding
   the serialized AST via serde_json costs more than the entire lexer+parser.
   Any pipeline stage or client that ships CAST as JSON (rather than passing
   the in-memory `crush_cast::Program`) is paying more than a full re-parse.
2. Within the text path, `compile` (CAST→CASM) is the largest phase (~18µs of
   ~69µs on fixture 20), then `semantic` (11µs); lex+parse together are ~13µs.

Compile latency is microseconds on these fixtures — the compile side is not
where order-of-magnitude end-to-end wins live. The execution tier is (§1.2).

### 1.2 Runtime baseline

_(pending — release `crush-run`/`crushc` build in flight; fib + tight-loop
wall-clock numbers land here)_

## 2. Client survey (read-only)

All 11 client repos exist; all deps are `path` deps (no crates.io consumers
among surveyed clients). Two consume the tree-sitter grammar only; one declares
a dep it never uses.

Framing note: `crush-vm` exposes **two** value types — `crush_vm::vm::Value`
(stack VM / HostCap ABI; razor, bro-agent via sdk) and `crush_vm::RuntimeValue`
(FastVM + `Arena`/`Object`; bozo). "Changing the Value enum" hits disjoint
client sets depending on which one is meant.

### Matrix: client × consumed surface × ripple risk

Ripple-risk columns: **casm** = casm instruction set; **cast** =
`crush_cast::Program`/`Function` shape; **value** = crush-vm value enums;
**frontend** = Parser/Compiler API.

| Client | crush crates | APIs actually called | casm | cast | value | frontend |
|---|---|---|---|---|---|---|
| exosphere/crush-symbols | `tree-sitter-crush` only | `tree_sitter_crush::LANGUAGE` (src/lang.rs:37). Its `crush_lang::parse_source` comes from **exosphere's own fork** (own casm + crush-cast) | — | — | — | — |
| razor | `crush-vm` | `HostCaps`, `HostCap`, `HostCapSpec`, `value_to_text`, `vm::Value` (src/tools.rs:15-20, src/toolkit.rs) | — | — | **HIGH** (`vm::Value`) | — |
| bro-cli/bro-agent | `crush-lang-sdk` 0.3.0, `crush-vm` (its `crush-symbols` dep is aliased to polydex) | `Runtime::new/run`, `compile::compile_crush_source`, `program.manifest.permissions`, `to_blob()` (src/tools/crush.rs:41-56) | MED (.cvm1 blob) | — | LOW | MED |
| openko/exo-light | `crush-vm` (signature-only) | `HostCaps`, `Box<dyn HostCap>`; execution is **subprocess**: probes `crush-run`/`crush`/`crush-vm`/`cvm1`, writes `.cvm1` temp + `CVM1_BYTECODE` env (src/fabric_executor.rs:414-438) | **HIGH** (.cvm1 + CLI contract) | — | LOW | — |
| mycelium-node | `crush-lang-sdk` (test-only, feature `fabric`) | `ProgramBuilder` with **raw casm mnemonics as string literals** (`PUSH_STR`, `CAP_CALL`; src/compute_task.rs:529-545) | **HIGH** (hardcoded mnemonics) | — | — | — |
| squeeze | `crush-pkg` 0.3.0, `crush-vm` 0.3.0 | `crush_pkg::{Manifest, PackageBuilder, runners, manifest}`; **`crush-vm` dep is dead** (zero usage) | — | — | — | — |
| crush-notebook | `crush-frontend`, `crush-vm`, `casm`, `crush-jit` | `compile_crush_source`; `assemble`, `PortableVm`, `Quotas`; `fastvm::{lower_program, FastYield}`; `JitEngine`; **hand-written `casm_to_assembly` mapping ~35 string opcodes** (kernel/src/main.rs:403-478) | **CRITICAL** | — | HIGH (fastvm) | HIGH |
| crush-visuals | `crush-frontend`, `crush-cast` (source-bridge); `crush-debugger` (+ dead `crush-vm` dep) (debug-bridge) | `crush_cast::{Program, Function, Statement, Expression}` + `parse_source`; **exhaustive match over ~17 Statement/Expression variants** (source-bridge/src/lib.rs:202-240) | — | **CRITICAL** | LOW | HIGH |
| polydex | `tree-sitter-crush` only | `tree_sitter_crush::LANGUAGE` (src/lang.rs:45) | — | — | — | — |
| crush-lsp | `crush-frontend` 0.3.0 | `check_source` + `diagnostics::DiagnosticSeverity` (src/lib.rs:185-200) | — | — | — | HIGH (narrow: 1 fn + 1 enum) |
| bozo (+ bozo-wasm) | `casm`, `crush-walker-core`, `crush-frontend`, `crush-vm`, `crush-aot`, `crush-lang-sdk`, 11 `crush-lang-*` adapters | `casm::Program`; `compile_crush_source` + `compiler::Compiler::new`; `run_fastvm_with_caps`; `RuntimeValue`, `Arena`, `Object`, `fastvm::{Capability, Hal, FastYield}`; `crush_aot::{AotCompiler, Module::load, gen_rust_source}`; `AdapterRegistry` | **HIGH** | LOW | **CRITICAL** (`RuntimeValue`+`Arena`) | **CRITICAL** |

### Subprocess vs library

- **exo-light** is the only true subprocess client (binary probe + `.cvm1`
  file + `CVM1_BYTECODE` env). Trap: if no binary is found, its `None` arm
  degrades to a **fake `exit_code: 0` success** — renaming `crush-run` breaks
  it silently, not loudly.
- **bro-agent** is hybrid: compiles in-process via the sdk, executes via
  exo-light's subprocess path (transitively exposed).
- No client invokes `crushc` / `crush-aotc` as binaries; no `run_casm_json`
  callers anywhere.

### Blast-radius ranking per change category

1. **casm instruction set** — worst: crush-notebook's `casm_to_assembly`
   (35-arm string match, unmatched ops silently become `NOP` → wrong programs,
   not compile errors); then mycelium-node (hardcoded mnemonic strings), bozo,
   exo-light (.cvm1 container).
2. **crush_cast Program/Function shape** — crush-visuals-source-bridge only,
   but hard (exhaustive matches: adding a variant is a compile error there).
   Also the nimbus contract (out of band of this survey).
3. **crush-vm value enums** — split: `vm::Value` → razor (+ sdk re-export);
   `RuntimeValue` → bozo (src + 4 test files).
4. **crush-frontend API** — widest reach (bozo, crush-notebook, bro-agent,
   crush-visuals, crush-lsp) but each client touches only 1–2 free functions;
   cheap to keep stable via the facade in `crates/crush-frontend/src/lib.rs`.

### Cleanup notes (captured as dejavue plans)

- squeeze declares `crush-vm` (Cargo.toml:32) with zero usage — dead dep.
- crush-visuals-debug-bridge declares `crush-vm` but references only
  `crush_debugger::*` — dead direct dep.
- crush-notebook's silent-NOP fallback in `casm_to_assembly` deserves a
  hard error arm.

## 3. Design findings (ranked)

### 3.1 Compile pipeline (parser → CAST → semantic → optimizer → compiler → casm)

Ranked by estimated impact on end-to-end compile latency.

1. **Instructions are built out of `serde_json::Value` — 4–6 heap allocations
   per emitted instruction.** `compiler.rs:2271-2288` (`create_instr`) +
   `casm/src/lib.rs:236-244`. 201 `create_instr` call sites, 254 `json!`
   literals. Per instruction: a `String` opcode, a `serde_json::Map` for args
   (+ `String` key per field), a *second* map for `meta` (empty essentially
   always), optionally a `lang` String. A 10k-instruction program does 40–60k
   allocations in codegen alone — and consumers pay again via `to_opcode()`
   re-parsing JSON at load. Better: emit the typed `casm::OpCode` enum (which
   already exists, `casm/src/lib.rs:66-222`) directly into `Vec<OpCode>`;
   keep JSON only as a serialization view.
2. **Every CAST node carries a `HashMap<String, serde_json::Value>` `meta`
   that is always empty.** All `Statement`/`Expression` variants
   (`crush-cast/src/lib.rs:80-405`). Parser writes `meta: HashMap::new()` at
   54 sites; only ONE site ever inserts anything (`parser/mod.rs:2183`).
   Costs 48 bytes inline per node (~2× node size), inflates every clone in
   the pipeline, and forces serde_json into the core type graph. Better: a
   packed `Span { lo: u32, hi: u32 }` per node + side table for the rare real
   metadata. Highest-leverage structural change in crush-cast — but it IS the
   nimbus/client contract shape (crush-visuals matches on these variants), so
   it needs a coordinated change.
3. **SemanticAnalyzer runs 4–14 full type-inference walks per program.**
   `semantics.rs:98-137`: seed pass + authoritative pass + fixed-point loop
   (≤10 iterations) + final `check_function` walk — the multi-pass structure
   exists only because `program.functions` is a HashMap with unordered
   iteration (comment at :92-97 says so). Better: call graph → Tarjan SCC →
   reverse-topological inference; one pass for non-recursive code, O(N+E).
4. **Per-node `Type` deep clones inside those passes.** `resolve_var` clones
   the full recursive `Type` on every variable reference
   (`semantics.rs:516-523`); every call expression clones `(Vec<Type>, Type)`
   (`semantics.rs:382`). Multiplied by the 4–14 passes of #3. Better: `&Type`
   /`Cow`, or intern types to `TypeId(u32)`.
5. **`compile_cast` deep-clones the entire program per compile.**
   `lib.rs:65-72` — full AST clone (2× oversized due to #2) purely so the
   optimizer can mutate; on the critical path of `compile_crush_source` used
   by sdk/aot/lang-c/js/python/dart. Better: by-value entry point +
   `compile_cast_ref` for callers that retain the original.
6. **Optimizer clones the const-propagation map 1–3× per nested block.**
   `optimizer.rs:115-124` (If ×3), While (:154), For (:172), TryCatch
   (:207-209) — O(C·N), quadratic in function length when constants
   accumulate. Values are full `Expression`s (with their meta maps) instead
   of a small `ConstVal` enum. Better: scoped-shadowing delta stack, O(delta)
   per block. Also: `While` bodies get an extra full pre-walk
   (`collect_mutated_vars`) — loop bodies traversed twice.
7. **Lexer: `Vec<char>` whole-source copy (4 B/char) + fresh `String` per
   token, comments materialized then discarded.** `lexer.rs:252,293-488`.
   `Token`'s largest variant makes `Vec<Token>` ~72 B/token. Better: byte-
   offset spans (`{kind, lo, hi}` = 12 B), interner for identifiers, skip
   comments at lex time.
8. **No compilation cache and no incremental unit anywhere.** Only cache in
   the front-end is import-resolution (`polyglot_imports.rs`). Every entry
   point (`crush-aot`, sdk, lang-c/js/python/dart) recompiles from source
   text every call. Better: content-hash-keyed `casm::Program` cache +
   per-function memoization (the compiler already emits functions
   independently).
9. **`mutation_check` is O(F²·C²).** `mutation_check.rs:21-86` — every
   caller × every other function × linear rescans per annotation match; runs
   on the `check_source` path. Better: pre-built name→indices maps, O(F+ΣC).
10. **No constant pool, no local slots, garbage debug info.**
    `casm::Function` has no constant table (literals inlined per use, paid at
    compile AND load); locals are name-strings at runtime (`locals` hardcoded
    `vec![]` at all 4 construction sites — the VM does a string-hash lookup
    per variable access even though the compiler tracks `declared_vars` and
    throws the numbering away; `crush-lang-sdk/src/compile.rs:11-20`
    re-derives slot numbering at a LATER layer from the strings);
    `record_debug_info_for_function` re-walks every instruction to produce
    `line 1 col 1` for everything (meta is always empty) with 2 allocations
    per instruction. Better: `consts: Vec<ConstValue>` + `PushConst(u16)`,
    slot-numbered `Load/Store(u16)`, RLE source map emitted inline.

Pass-structure summary: between `check_source` and `compile` there are 5+
independent full walks of every function body (enrich, mutation-check,
semantic ×4-14, optimizer, compiler pre-passes + emit + `extract_type_hints`
+ `ensure_return`'s per-function string scan) that could be fused into far
fewer visitors.

Dead/unwired code found: `casm::Program::to_cached`/`CachedProgram`
(`casm/src/lib.rs:246-610`) — the abstraction whose doc-comment promises
"10-100x faster execution" by eliminating string dispatch — is entirely
unreferenced repo-wide (and is itself O(F²) as written + clones the whole
program). `casm::ecasm` (1,039 lines, ~49% of the casm lib) has zero external
references. `crush-ptx/src/compiler.rs:111-113` calls `to_opcode()` three
times per instruction (each re-parses JSON). Adjacent correctness bug:
`DebugInfo.source_map` mixes per-function pc into one flat vector —
`source_location_for_pc` returns another function's location once a program
has >1 function.

### 3.2 Execution tier (VM/JIT)

_(pending — VM audit in flight)_

## 4. Implemented improvements

### 4.1 SCC-ordered return-type inference (finding #3) — `11f7a1c`

Replaced `SemanticAnalyzer`'s whole-program multi-pass inference (tolerant
seed pass + authoritative pass + ≤10-iteration global fixed point — up to 12
full body walks, all to compensate for `HashMap` iteration order) with call
graph → Tarjan SCC → reverse-topological inference (`semantics.rs`).
Non-recursive functions now get exactly **one** authoritative walk; only
genuinely recursive SCCs iterate to a fixed point, scoped to their members.

This also fixes a latent **correctness bug**: the old fixed point capped at
10 iterations, so a call chain deeper than ~12 functions could
nondeterministically fail to converge (depending on `HashMap` order), leaving
placeholder `Null` return types — silently wrong programs. Pinned by
`deep_call_chain_return_types_resolve` (40-deep chain) and
`mutual_recursion_return_types_resolve` in `tests/type_check_tests.rs`.

**Scaling measurement** (`tests/semantic_scaling.rs`, run with
`cargo test -p crush-frontend --test semantic_scaling --release -- --ignored --nocapture`;
median of 30 runs of `SemanticAnalyzer::check`):

```
functions,call_shape,median_check_us   OLD (multi-pass)   NEW (SCC)   speedup
25,chain_forward                        62                 16          3.9x
100,chain_forward                       231                65          3.6x
300,chain_forward                       694                205         3.4x
25,chain_arith                          22                 17          1.3x
300,chain_arith                         276                213         1.3x
```

`chain_forward` (`return callee(x)`) is the honest worst case: each old
whole-program pass propagated types exactly one chain level, so the old
numbers above are also **wrong results** past depth ~12 (types left `Null` at
the cap) — the 3.4x is the cost of not even converging. `chain_arith`
(`return callee(x) + 1`) converges in one pass under the old code too
(lenient `Null + Int → Int` coalescing), so it isolates the constant-factor
win (~1.3x).

**Standard bench after** (`cargo bench -p crush-frontend --bench cast_compile`,
same fixtures as §1.1; these have shallow call graphs so the win is the
constant factor only):

```
fixture,path,p50_us,p95_us,peak_heap_bytes      baseline p50 → after p50
09,text,12,13,36729                              14 → 12
13,text,13,15,26058                              16 → 13
16,text,33,40,48625                              38 → 33
19,text,36,60,46704                              39 → 36
20,text,59,67,83880                              69 → 59
20,breakdown,lex=8,parse=7,semantic=9,optimize=4,compile=19   (semantic 11 → 9)
```

~10–15% end-to-end compile improvement on the small fixtures; the
asymptotic behavior is the real payoff (semantic analysis is now O(N+E) walks
instead of O(13·N) worst case, and deterministic).

### 4.2 Clone-free compile entry point (finding #5) — `11f7a1c`

`compile_cast_owned(Program)` added; `compile_crush_source` (the hot path
used by crush-lang-sdk / crush-aot / lang-c/js/python/dart) now consumes the
freshly-parsed program instead of deep-cloning the entire AST (the clone was
2× oversized due to the always-empty `meta` maps, finding #2).
`compile_cast(&Program)` keeps its signature — existing callers (bash walker
tests, type-check tests) are untouched.

### 4.3 Not landed here (needs coordination — flagged per halt criteria)

Findings **#1** (typed `OpCode` emission — casm instruction-stream shape is
the `.cvm1`/crush-notebook/exo-light/mycelium contract) and **#2** (CAST
`meta` → packed spans — the `crush_cast::Program` shape is the
nimbus/crush-visuals contract) are the two largest wins but both change
cross-repo contract shapes. Per the dispatch's halt criteria they are
flagged for foreman sign-off rather than landed unilaterally; both are
captured as dejavue plan entries with design sketches in §3.1.
