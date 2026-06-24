# Handoff

Read `.dejavue/state.md`, `.dejavue/decisions.md`, and `.dejavue/timeline.jsonl` before making changes.

## Current State

Crush is in good shape. 414+ tests, 0 warnings. Three branches worth of work have converged:
- `agent/opencode/types` — VM type expansion (Bool, Map, Error, Bytes + 11 new opcodes)
- `agent/opencode/polyglot` — EXEC_LANG, polyglot stdlib, rust_walker macro handling
- `agent/opencode/rustpython` — Native parser migration, Frontend trait, Python + Rust frontends

All three branches have been merged into `agent/opencode/rustpython` (the active branch).

## Next Steps

### High Priority
1. **Publish core crates** (crush-errors, crush-cast, casm) to crates.io so external dependents (openko, crush-symbols, mycelium-mobile, arniko) can version-dep instead of path-dep.
2. **Merge `agent/opencode/rustpython` to `main`** — currently living on a feature branch, needs to land.

### Medium Priority
3. **JS/TS walker** — implement `crush-lang-js` with `boa_parser` (plan at `crates/crush-lang-js/PLAN.md`). No TypeScript support until boa adds it.
4. **Bash walker** — implement `crush-lang-bash` with `brush-parser`. API mismatch issues last attempted.
5. **Fix `--all-features` build** — `db` and `stdlib` features currently fail with Value::Bool type errors.
6. **Portable VM parity** — portable_vm.rs lacks EXEC_LANG, ENTER_TRY/EXIT_TRY/THROW, NEW_OBJ/SET_FIELD/GET_FIELD, ARR_PUSH/POP, PUSH_BOOL.

### Low Priority
7. **Lambda and Match compilation** — currently bail at compile time.
8. **Async/await** — parsed but not executable.
9. **Doc warnings** — 12 doc warnings across casm and SDK crates.
10. **More polyglot examples** — test variable wiring across Python/Bash/JS blocks.

## Recent backfills (2026-06-22)

- `agent/buffy/CRUSHDEJAVUE-1` retro-registered the crush-pkg fedpath byte-exact NDJSON contract (commit `2f2b2f5`) into STATE.md + TASKS.md + `.dejavue/decisions.md` + `.dejavue/timeline.jsonl`. Full mirror entry at `.dejavue/decisions.md` 2026-06-22 [REGISTRATION] section.
- 3 runner-subsystem gaps catalogued in `TICKETS/CRUSHRUNNERS-1.md` (each sized S, addressable independently): `ContainerRunner` stub-bail (`runners.rs:122-128`); `ScriptRuntime` 4-variant cap (`manifest.rs`); `PayloadFormat::Unknown` → `CrushRunner` silent fallback (`runners.rs:175`).
