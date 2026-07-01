# State

Updated: 2026-07-01T00:38:09-05:00

## v0.2.0 Workspace — 27 crates (+1 design doc)

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

### Design Docs (new)
- `docs/design/crush-jit-backend.md` — Cranelift JIT architecture, 7-phase roadmap, nan-boxing, GC strategy

### Test Status
- All workspace-wide unit, integration, and doctests pass cleanly (430+ green), 0 warnings.

## Known External Dependents
openko/fabric, crush-symbols, mycelium-mobile, arniko, sona — all dependent on crates in this repo.
