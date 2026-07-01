# State

Updated: 2026-07-01T15:34:00-05:00

## v0.2.0 Workspace — 30+ crates (+ design docs)

### Core IR
- `crush-errors`, `crush-cast`, `casm`

### Grammar
- `tree-sitter-crush` (v0.1.0)

### Walkers / Language Frontends
- `walker-core` (v0.1.0) — Frontend trait, FeatureReport, BaseWalker
- `cli` (v0.1.0) — walker dispatcher
- `python_walker` via `crush-lang-python` — rustpython-parser
- `rust_walker` via `crush-lang-rust` — syn
- `crush-lang-c` (C/C++ with tree-sitter, renamed from c_walker)
- `crush-lang-js` (JS/TS via swc + boa)
- `crush-lang-bash`, `crush-lang-zsh` (tree-sitter shell)
- `crush-lang-custom` — Meta-Frontend using CSON for dynamic grammar
- `wasm_walker` (WASM with WASI lowering, lib crate + bin)
- `go_walker`, `zig_walker` (tree-sitter-based)

### Runtime & Tools
- `crush-vm` — CVM1 bytecode + FastVM + PortableVM + scheduler + Arena GC + polyglot executors + FFI + AI optimizer
- `crush-frontend` — compiler (parser, sema, optimizer, compiler, cson desugar, render)
- `crush-lang-sdk` — SDK + binaries (crushc, crush-run, crush-compile, crush-repl)
- `crush-pkg` — package manager with manifest + runners + builder
- `crush-installer` — toolchain installer
- `crush-python` — PyO3 bindings for crush-cast
- `crush-index` — cross-module index
- `crush-debugger` — interactive runtime debugger with breakpoints, step, inspect
- `crush-lint` — linting
- `crush-net` — networking with mesh-proto support
- `crush-cson` — CSON parser

### Polyglot
- `EXEC_LANG` opcode + polyglot executor registry
- Three-lane Python: CAST transpile / (RustPython planned) / subprocess
- C FFI plugin support (`crush-ffi`, `crush-plugin-example`)

### Design Docs
- `docs/design/crush-jit-backend.md` — Cranelift JIT architecture (7-phase roadmap)
- `docs/design/cson_proposal.md` — CSON specification

### Test Status
- Core crates (crush-vm, crush-cast, casm, crush-frontend, crush-lang-sdk, crush-debugger) pass clean.
- Pre-existing: test_ffi_gateway_cap needs /tmp/example_c_plugin.so built separately.

### Recent Merge (2026-07-01)
- `agent/buffy/CRUSHSDK-1` — ticket file only
- `agent/buffy/debugger-initial-scaffold` — crush-debugger crate + breakpoint support in portable_vm
- `feat/p2-walkers-maturation` → `main` — all feature branches consolidated

## Known External Dependents
openko/fabric, crush-symbols, mycelium-mobile, arniko, sona.
