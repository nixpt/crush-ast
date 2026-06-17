---
name: crush-ast
purpose: Standalone Crush language toolchain — CAST IR, tree-sitter grammar, polyglot walkers, compiler frontend, VM runtime, package manager, installer. Extracted from exosphere on 2026-06-12.
dcp: DCP/1.0
---

# Context

## Operating Rules

- All internal crate deps use `workspace = true` (never raw `path = "../"`).
- Workspace deps have both `path` + `version` fields.
- Dependency DAG is acyclic: `crush-frontend` → `crush-cast` → `casm` → `crush-errors`. No back-edges.
- No path deps to exosphere — crush-ast is fully standalone.
- When adding a new language frontend, implement the `Frontend` trait in `walker_core`.

## Architecture

Four crate layers:
1. **Core IR**: `crush-errors` → `crush-cast` → `casm`
2. **Grammar**: `tree-sitter-crush`
3. **Walkers/Frontends**: `walker-core` (Frontend trait) → language crates (`crush-lang-python`, `crush-lang-rust`, etc.)
4. **Runtime & Tools**: `crush-vm` → `crush-frontend` → `crush-lang-sdk` → `crush-pkg` + `crush-installer`

Language walkers migrated from tree-sitter to native parsers:
- Python: `rustpython-parser` (pure Rust)
- Rust: `syn` (canonical Rust parser)
- JavaScript/TS: `boa_parser` (planned)
- Bash: `brush-parser` (planned)

## Type System (VM Value enum)
- Null, Bool, Int, Float, Str, Array, Map, Error, Bytes
- 35+ opcodes including EXEC_LANG, ENTER_TRY/EXIT_TRY/THROW, NEW_OBJ/SET_FIELD/GET_FIELD
- Polyglot blocks via `@python { }` → `EXEC_LANG` → subprocess dispatch with variable wiring

## Key Decisions

- No embedded RustPython VM — parser-only approach (crushpython4.md)
- GuestRuntime ABI removed — keep one VM, not two
- Frontend trait replaces Walker for native-parser languages

## External Dependents

openko/fabric → `crush-lang-sdk`; crush-symbols → `crush-cast`, `tree-sitter-crush`; mycelium-mobile → `crush-lang-sdk`; arniko → `crush-lang-sdk`
