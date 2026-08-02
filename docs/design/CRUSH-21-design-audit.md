# CRUSH-21 — crush-ast design audit

Branch: `agent/panini-crush/CRUSH-21` · Author: panini · Date: 2026-08-02

Captain's directive: explore and improve the design by ~3000x (ambition level,
not a literal gate) — hunt design-level wins across the pipeline
(parser → CAST → CASM compile → portable VM → JIT), grounded in a measured
baseline and a survey of what clients actually consume.

## 1. Baseline

### 1.1 Compile pipeline (`cargo bench -p crush-frontend --bench cast_compile`)

Full raw output: `docs/design/CRUSH-21/baseline-cast-compile-bench.txt`.
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

_(pending — pipeline + VM audit sections land here)_

## 4. Implemented improvements

_(pending)_
