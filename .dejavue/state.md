# State

Updated: 2026-06-16T00:10:00Z

## Complete

- **Path deps migrated**: All 19 crates use `workspace = true` for internal deps. `publish = false` removed.
- **Repo docs created**: `README.md`, `CONTRIBUTING.md`, dejavue context/state/decisions/handoff.
- **Crate READMEs fixed**: 9 crate READMEs — stale cross-references to exosphere paths replaced with correct crush-ast references.
- **Crush Language Guide updated**: `src/README.md` now points to crush-ast as the implementation repo. Other stale exosphere refs fixed.
- **Compiler fixes**: `pub fn` parser support, `str.join` return type inference, string + non-string `+` auto-conversion, recursive function return type inference.
- **crushc binary**: rustc-style compiler binary with `--emit {vm,casm,ast,types}`, `--check`, `-O`, `--cap`, `-L`, `-v`. 5 integration tests.
- **crush-installer**: Full install/uninstall/status toolchain with 5 unit tests, end-to-end verified.
- **CAST fixtures copied**: 16 `examples/cast/*.cast.json` from exosphere. Fixes pre-existing `crush-cast` test failure (pack_tests path corrected).
- **CAST docs copied**: `docs/cast/cookbook.md`, `docs/cast/schema-reference.md`.

## Test Status

320 pass, 1 pre-existing failure (walker-core doc-test references non-existent `tree_sitter_mylang`).

## Known External Dependents

openko/fabric, crush-symbols, mycelium-mobile, arniko — all path-dep on crates via `../crush-ast/crates/`.
