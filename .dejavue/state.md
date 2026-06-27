# State

Updated: 2026-06-22T15:28:21-05:00

## v0.2.0 Workspace — 23 crates

### Core IR (v0.2.0)
- `crush-errors`, `crush-cast`, `casm`

### Grammar
- `tree-sitter-crush` (v0.1.0)

### Walkers / Language Frontends
- `walker-core` (v0.1.0) — Frontend trait, FeatureReport, BaseWalker
- `cli` (v0.1.0) — walker dispatcher
- `python_walker` via `crush-lang-python` — rustpython-parser (replaced tree-sitter)
- `rust_walker` via `crush-lang-rust` — syn (replaced tree-sitter)
- Remaining tree-sitter: js, go, c, zig, bash, wasm
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
- 414+ tests pass (workspace), 0 warnings
- Python frontend: 6 FeatureReport tests + 3 pipeline tests
- All 31 crush-vm tests pass including new types

## Known External Dependents
openko/fabric, crush-symbols, mycelium-mobile, arniko — all path-dep on crates in this repo.

## Recent registrations (post-hoc, 2026-06-22)

- **crush-pkg fedpath byte-exact NDJSON contract** (commit `2f2b2f5`): retro-registered via STATE.md `Test hardening (CRUSHPKG-1)` paragraph + TASKS.md `Done log` entry. Surface tests: `handle_lint_with_byte_exact_three_rule_fedpath` (byte-exact NDJSON across `ObsoleteKey` + `PlaceholderValue` + `UnreferencedDependency`); `handle_lint_with_referenced_dep_suppresses_finding_end_to_end` (full entry-aware cross-ref pin: `Manifest::from_str` → `parent().join(&entry)` → `scan_entry_file_references` → `lint_capsule_toml_with_entry`); `scan_entry_file_references` URL-fragment fix at `builder.rs:998-1007`. Closes the 2-day registration gap between the squash-merge on 2026-06-20 and the CRUSHPKG-1 retro-pass on 2026-06-22. Sister gap catalogue: `TICKETS/CRUSHRUNNERS-1.md` (3 runner-subsystem gaps).
