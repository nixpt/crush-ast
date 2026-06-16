---
name: crush-ast
purpose: Standalone Crush language toolchain — CAST IR, tree-sitter grammar, polyglot walkers, compiler frontend, VM runtime, package manager, installer. Extracted from exosphere on 2026-06-12.
dcp: DCP/1.0
---

# Context

## Operating Rules

- All internal crate deps MUST use `workspace = true` — never raw `path = "../"` in member Cargo.toml files.
- Workspace deps MUST have both `path` + `version` fields so crates can be individually published.
- Dependency DAG is acyclic: `crush-frontend` → `crush-cast` → `casm` → `crush-errors`. Never add a back-edge.
- No path deps to exosphere or other external repos — crush-ast is fully standalone.
- `publish = false` at `[workspace.package]` blocks all crates unconditionally. Remove it and let individual crates opt in.
- When adding a new language walker, register it as a workspace member AND add it to `[workspace.dependencies]`.

## Key Architecture

Four crate layers:
1. **Core IR**: `crush-errors` → `crush-cast` → `casm`
2. **Grammar**: `tree-sitter-crush` (no internal deps)
3. **Walkers**: `walker-core` (trait) → language walkers + `walker` CLI
4. **Runtime & Tools**: `crush-vm` → `crush-frontend` → `crush-lang-sdk` → `crush-pkg` + `crush-installer`

External dependents that path-dep on this repo:
- `openko/fabric` → `crush-lang-sdk`
- `crush-symbols` → `crush-cast`, `tree-sitter-crush`
- `mycelium-mobile` → `crush-lang-sdk`
- `arniko` → `crush-lang-sdk`

## Build / Test

```bash
cargo build
cargo test --workspace --exclude crush-cast --exclude walker-core
```

`crush-cast` has a pre-existing fixture issue (missing `examples/cast/` fixtures). `walker-core` has a pre-existing doc-test issue (references non-existent `tree_sitter_mylang` crate). Neither is caused by this workspace.
