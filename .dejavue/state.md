# State

Updated: 2026-06-30T03:45:00-05:00

## v0.2.0 Workspace — 27 crates

### Core IR (v0.2.0)
- `crush-errors`, `crush-cast`, `casm`

### Grammar
- `tree-sitter-crush` (v0.1.0)

### Walkers / Language Frontends
- `walker-core` (v0.1.0) — Frontend trait, FeatureReport, BaseWalker
- `cli` (v0.1.0) — walker dispatcher
- `python_walker` via `crush-lang-python` — rustpython-parser (replaced tree-sitter)
- `rust_walker` via `crush-lang-rust` — syn (replaced tree-sitter)
- `crush-lang-custom` — Meta-Frontend using CSON for dynamic grammar mapping
- `crush-lang-sona` — Extracted/deleted from workspace. Moved to standalone private repository `nixpt/sona`.
- Matured tree-sitter-based frontends: `crush-lang-c` (C/C++), `wasm_walker` (WebAssembly with WASI lowering), `go_walker` (Go), `zig_walker` (Zig).
- Old tree-sitter crates: `python_walker/` and `rust_walker/` deleted

### Runtime & Tools (v0.2.0)
- `crush-vm` — CVM1 bytecode with 35+ opcodes, Value::{Bool,Map,Error,Bytes}
- `crush-frontend` — compiler frontend (parser, sema, optimizer, compiler)
- `crush-lang-sdk` — SDK + binaries (crushc, crush-run, crush-compile, crush-repl)
- `crush-pkg` — package manager
- `crush-installer` — toolchain installer
- `crush-python` — PyO3 bindings for crush-cast

### Polyglot
- `EXEC_LANG` opcode (0x70) — subprocess dispatch for `@python { }` blocks
- Variable wiring across polyglot blocks via env vars + stdout capture
- Three-lane Python: CAST transpile / (RustPython planned) / subprocess

### Test Status
- All workspace-wide unit, integration, and doctests pass cleanly (430+ green), 0 warnings.
- Wasm walker verified with integration test suite compiling `.wat` sources containing WASI calls.

## Known External Dependents
openko/fabric, crush-symbols, mycelium-mobile, arniko, sona — all dependent on crates in this repo.

## Recent registrations (post-hoc, 2026-06-30)

- **Sona & Wasm walker maturation & CRUSHRUNNERS-1 Gap 3** (commit `dd9bca5` & `75b38c6`):
  - Extracted Sona compiler and runtime to private repository `nixpt/sona`.
  - Registered `.sn` and `.sno` extensions in `walker-core` and `crush-pkg`.
  - Modified `CrushRunner` to support in-process execution of compiled `.sno` (JSON-serialized `casm::Program`) payloads, converted to VM programs via `casm_to_vm`.
  - Added support for `arr_get` opcode in `casm_to_vm`.
  - Added a test case `test_sno_execution` verifying Sona payload execution.
  - Implemented strict-mode bail on unknown formats in `crush-pkg` run subcommand.
  - Matured `wasm_walker` with an integration test suite translating WASI calls into `io.print` capability calls.
  - Matured and checked off `c_walker`, `go_walker`, and `zig_walker` on the active roadmap.

