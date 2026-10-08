# CRUSH-170 — Capability inference: diff used vs declared caps (`crush-pkg check`)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-170 |
| **Priority** | P3 |
| **Status** | Done (PR pending review) |
| **Phase** | M5 |
| **Assignee** | panini |
| **Dependencies** | CRUSH-151, CRUSH-153 (final cap list) |
| **Estimated effort** | M (~60 turns) |
| **Relay lane** | D3 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

A package declares `[capabilities] required` in `capsule.toml`, but nothing checks that the program's CAST actually uses only those (or that every declared cap is used). A parked incubator prototype inferred required caps from a CAST program's imports/calls and diffed them against the declared set.

## Success criteria

- [x] a pass over CAST collecting every `CapabilityCall` / builtin that lowers to `CAP_CALL`, plus `@lang` blocks → `polyglot.<lang>`
- [x] `crush-pkg check` reports undeclared-but-used (error) and declared-but-unused (warning), NDJSON via `crush-diagnostics`
- [x] tests on fixture packages
- [x] documents the known blind spots (dynamic cap names, if any)

## Technical approach

- Re-implement against today's `crush-cast` API; nothing from the prototype is copied (private, old API).

## Files to modify

- `crates/crush-pkg/src/` (check)
- possibly a helper in `crates/crush-frontend`

## Non-goals

- auto-editing `capsule.toml`

## Source (reference only — re-implement, don't copy)

- incubator `parked/crates/ai/services/shadow-capsule` (private, idea only)

## Lane D note (panini-d, 2026-10-07)

Not started. The dispatch said to take this only if lane A's caps (CRUSH-151–153) were already merged on `main`. They aren't: `origin/main` is still `9abf05b`, and #97/#98/#99/#100 are open. The final capability list isn't settled yet, so this waits.

Two things for whoever picks this up, found while doing CRUSH-167/171:
- `PackageBuilder::check` compiles each file on its own and fails on any package whose entry calls a path-dep function (captured in TASKS.md). Fix that first, or do the inference over the combined program `build` compiles, or every package with deps will report spurious errors.
- CRUSH-171 added `[capsule] platforms`. A natural extension here is a warning when a `web` capsule uses a capability the browser doesn't grant (crush-web grants `io.print`, `str.*`, `conv.chr/ord` only).

## Outcome (panini, 2026-10-08)

- **Inference reads CVM1 bytecode, not CAST:** `crush_vm::capabilities_used(&Program)`
  (`crates/crush-vm/src/cap_inference.rs`). That's after every lowering, so "builtins that lower
  to `CAP_CALL`" come for free instead of from a copy of the lowering table, and the names are
  exactly what the VM gates on: `CAP_CALL` consts, `EXEC_LANG` → `polyglot.<lang>` (the same
  `polyglot_gate_name` both backends now share), AI/DOM opcodes → `ai_native.*`/`dom_native.*`.
  Only code reachable from the entry counts (CALL edges, plus any function named by a string
  constant, because SPAWN pops its target's name), so a path dep's uncalled functions add nothing.
- **Classification reuses one table:** `crush_lang_sdk::effects::catalog()` is `crush-run caps
  --json`'s table moved into the SDK; ambient = VM built-in / always / stdlib.
- **`PackageBuilder::check` fix (lane D note 1):** it now calls `build()` (compile the combined
  program, no `target/` write) and runs the inference on that. No design change was needed.
- **Web extension (lane D note 2):** done. The browser set is `crush_vm::capabilities()` because
  crush-web calls `crush_vm::run` with no host registry (`crates/crush-web/src/lib.rs`).
- **Blind spots** (documented in `crates/crush-pkg/MANIFEST.md`): a spawn target whose name is
  computed at run time is missed; analysis is per program, not per path (a cap behind a dead
  `if` counts); scoped entries' scopes aren't checked. No dynamic capability names exist in CVM1.
- The CRUSH-172 capsule checks clean: used = `env.get, fs.cat, io.print, push, str.len,
  str.split`, required = exactly `env.get, fs.cat, io.print`.
