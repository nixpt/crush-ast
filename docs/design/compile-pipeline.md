# Compile pipeline — from source to a running program

**Status:** design note, 2026-10-07 (CRUSH-173). Describes what the binaries actually call, not the
intended architecture. Paths under `sdk/` mean `crates/crush-lang-sdk/`. Known divergence:
**CRUSH-141** (crushc vs crush-run).

## The stages

```text
.crush source
  │ crush_frontend::parser::Parser::parse              → crush_cast::Program (CAST)
  │ crush_frontend::cast_enrich::enrich_cast            (fills match exhaustiveness sites)
  │ sdk::compile::prepare_polyglot_blocks               (@python/@javascript free variables → marshaling)
  │ crush_frontend::semantics::SemanticAnalyzer::check  (types, return-type inference)
  │ crush_frontend::optimizer::Optimizer::optimize      (constant folding etc.)
  │ crush_frontend::compiler::Compiler::compile         → casm::Program (JSON IR)
  │ sdk::compile::casm_to_vm                            → CVM1 text assembly → crush_vm::assemble
  ▼
crush_vm::Program (CVM1)  ──to_blob──▶  .cvm1 file
  │ crush_vm::run_with_caps (CVM1 green-thread scheduler) + HostCaps from the CLI grants
  ▼
output
```

The library entry points bundle these:

| Function | Does |
|---|---|
| `crush_frontend::parse_source` | parse + `enrich_cast` |
| `crush_frontend::compile_cast` / `compile_cast_owned` | semantics → optimizer (always) → `Compiler::new().compile` |
| `sdk::compile::compile_crush_to_casm` | `parse_source` → `prepare_polyglot_blocks` → `compile_cast_owned` |
| `sdk::compile::compile_crush_source` | `compile_crush_to_casm` → `casm_to_vm` |

## The three forms called "CASM"

1. **JSON IR** — `casm::Program` (`crates/casm`): functions of `{op, args}` instructions plus a
   manifest of permissions. This is what `Compiler::compile` produces. `casm::Format` names its
   files `.casm` (JSON) / `.casmb` (MessagePack).
2. **Text assembly** — `.func main`, labels, `PUSH_STR "x"`, `CAP_CALL "io.print" 1`, read by
   `crush_vm::assemble` (`crates/crush-vm/src/assembler.rs`). **`crush-run` treats `.casm` as this
   format**, not as the JSON IR — the same extension means different things to `casm::Format` and to
   the CLI.
3. **CVM1 bytecode** — `crush_vm::Program` (`crates/crush-vm/src/bytecode.rs`):
   `"CVM1" | version | manifest | constants | code`, serialized by `to_blob`/`from_blob` as `.cvm1`.

`casm_to_vm` is the only IR → CVM1 bridge. It renders the IR as text assembly and assembles that. It
lowers a subset: stack/arithmetic/logic, locals (slots per function), local `CALL` vs `CAP_CALL` (a
non-local call becomes a capability call and its name is added to the manifest), jumps, arrays,
strings, objects, try/throw, spawn/await, `EXEC_LANG`. Lossy: `new_array` ignores its size; `dom_*`,
`ai_*` and `export_var` become `NOP`. Anything else is an error (`Unsupported CVM1 opcode`).

## The binaries

| Binary | Input | Pipeline | Engine |
|---|---|---|---|
| `crush-run run F` | `.crush` | `compile_crush_source` (optimizer **always on**) | CVM1 scheduler |
| | `.casm` | text assembly, `--cap` lists the manifest permissions | CVM1 scheduler |
| | `.cvm1` | `Program::from_blob` | CVM1 scheduler |
| `crushc F` | Crush source (no extension check) | parse (**no** `enrich_cast`) → `prepare_polyglot_blocks` → semantics → optimizer **only with `-O`** → compile → `casm_to_vm` | none — writes `.cvm1` (or `--emit casm`/`ast`/`types`) |
| `crush build F` | — | runs `crushc` | — |
| `crush run F` | — | runs `crush-run run` | — |
| `crush-compile F` | text assembly only | `assemble` → `.cvm1` | none |
| `crush-repl` | Crush lines | sdk `repl.rs` | PortableVm |
| `crush-walk-run F` (crush-aot) | a walker language, by extension (`AdapterRegistry`: python, js/ts, rust, c, go, zig, wasm, bash, zsh, nepcode, dart) | walker → `Compiler::compile` (no semantics/optimizer) → `casm_to_vm` | CVM1 scheduler, stub host caps |
| `crush-aotc` (crush-aot) | Crush or a walker language | as above → `crush_aot` Rust/C codegen | native `.so` / C / Rust source |

`crushc --emit` details: `ast` renders CAST back to source before type checking; `types` renders the
same thing after the check passes (it is not type-annotated); `casm` prints CVM1 text assembly
recovered by `crush_vm::disassemble` — which does not write the manifest back, so feeding it to
`crush-run` needs `--cap` for each capability.

Other engines exist but no CLI path above reaches them: PortableVm (`crush-repl`, `crush-web`,
`crush-debugger`, `crush-diff`), FastVM (`run_fastvm`, the `crush!` macros, `crush-diff`), the JIT
(`jit-runner` on a lowered-program JSON, and tests), PTX (library only).

## Capabilities on the way through

Two independent checks gate every `CAP_CALL` at run time (`crates/crush-vm/src/scheduler.rs`,
`dispatch_cap`):

1. **Declared** — the name must be in the program's manifest. For `.crush` input the compiler and
   `casm_to_vm` collect it automatically; for `.casm` text it comes from `--cap`.
2. **Granted** — VM built-ins (`io.print`, `str.len`, …) are always there; everything else must be
   registered in the `HostCaps` the CLI builds from its flags (`--fs`, `--time`, `--env`, `--net`,
   `--polyglot`, `--stdlib`, …). Missing → `unknown capability: <name>`.

A `@python { }` block compiles to `EXEC_LANG` and additionally needs the `polyglot.python` grant
(`--polyglot`); see `docs/design/exec-lang-pluggable-executor.md`.

## Where the paths diverge (CRUSH-141)

The same `.crush` file is not compiled identically by every tool:

| | `crush-run` / `compile_crush_source` | `crushc` | `crush-diff` (`differential_run`) |
|---|---|---|---|
| `enrich_cast` | yes | **no** (calls `Parser::parse` directly) | yes |
| `prepare_polyglot_blocks` | yes | yes | **no** |
| optimizer | always | only `-O` | always |
| `--invariant-runtime` | no | opt-in | no |

`enrich_cast` only fills `exhaustive_sites`, which the exhaustiveness checker reads and the compiler
does not, so the bytecode difference between `crushc` and `crush-run` is the optimizer: an optimizer
bug (e.g. CRUSH-131) shows up under `crush-run` and not under a plain `crushc` build. Fixing CRUSH-141 means routing `crushc` through `compile_crush_to_casm` with an explicit
"no optimizer" switch, and adding a test that compares the two tools' bytecode.
