# CRUSH-175 — `crush doctor` — polyglot runtime health check

| Field | Value |
|-------|-------|
| **ID** | CRUSH-175 |
| **Priority** | P4 |
| **Status** | Done (PR pending review) |
| **Phase** | M7 |
| **Assignee** | panini-e |
| **Dependencies** | none |
| **Estimated effort** | S (~25 turns) |
| **Relay lane** | E5 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

Polyglot needs host `python3`/`node`/`bash` (and `bwrap`/buckets for `sandboxed-polyglot`); nothing tells a user what is missing until a block fails. The old `crush-cli` had a `doctor` command (its joker/federation checks are out of scope).

## Success criteria

- [x] `crush doctor` reports presence + version of python3, node, bash, bwrap, buckets, and which polyglot features this build has
- [x] exit code non-zero when a granted-by-default runtime is missing; `--json` output
- [x] test with a fake PATH

## Technical approach

- Subcommand in the `crush` umbrella (`crush-lang-sdk/src/bin/crush.rs`), reusing `resolve_lang_binary`.

## Files to modify

- `crates/crush-lang-sdk/src/bin/crush.rs` (+ a module)

## Non-goals

- installing anything

## Source (reference only — re-implement, don't copy)

- `nixpt/crush` `tools/crush-cli` (`doctor`)

## Evidence (2026-10-07, panini-e)

- `crates/crush-lang-sdk/src/doctor.rs` (`Report::collect`, text + JSON), `crush doctor [--json]` in
  `src/bin/crush.rs`. Interpreter names come from `crush_vm::resolve_lang_binary` (now `pub`, was
  `pub(crate)`) so the check can't drift from what `EXEC_LANG` spawns; `crush_vm::SANDBOXED_POLYGLOT`
  reports the sandbox feature.
- "Granted-by-default" = what `crush run --polyglot` grants (python/javascript/bash): any missing → exit
  1. `bwrap` is required only when the build has `sandboxed-polyglot`; `buckets` (CLI) is informational.
  Usage errors exit 2. `--version` probes are killed after 5 s.
- Tests: 4 unit tests in `doctor.rs` (PATH search skips non-executables, stderr version fallback,
  missing required → not ok, optional sandbox tools don't fail); 3 integration tests in
  `tests/crush_test.rs` running the real `crush` binary with `PATH` = a temp dir of fake tools (all
  found → exit 0 + versions in JSON; `node` missing → exit 1 + `MISSING`; bad arg → exit 2).
  `cargo test -p crush-lang-sdk --test crush_test`: 8/8; `--lib doctor`: 4/4.
- Live on the dev box: python3/node/bash/bwrap/buckets all found with versions, exit 0.
- Not a capability: it runs on the host CLI before any program, grants nothing, so no cap-refusal test
  applies.
