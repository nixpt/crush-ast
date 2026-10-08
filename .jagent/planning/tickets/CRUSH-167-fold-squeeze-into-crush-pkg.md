# CRUSH-167 — Fold squeeze's build-then-run flow into crush-pkg (decision-gated)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-167 |
| **Priority** | P3 |
| **Status** | Done (PR open) |
| **Phase** | M0 |
| **Assignee** | panini-d |
| **Dependencies** | CRUSH-161, decision C-4 |
| **Estimated effort** | S (~40 turns) |
| **Relay lane** | D2 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

squeeze (`nixpt/squeeze`, 227 L, unpublished, no tests) is ~60 lines of composition over crush-pkg: default check → build → run, and a guard refusing non-Crush capsules for `build`/`check`. It cannot ship before crush-pkg, and SQUEEZE-5 would re-wrap commands crush-pkg already has.

## Success criteria

- [x] decision C-4 recorded (captain s474: fold; dejavue decision CRUSH-167)
- [x] if fold: `crush-pkg` gains the default build-then-run command (with args pass-through and the existing `--message-format`) and the non-Crush guard; tests for both
- [n/a] if keep-thin: squeeze becomes a binary that calls into crush-pkg's CLI entry point (crush-pkg exposes one), its dead `crush-vm` dep removed (SQUEEZE-7), requirement raised to `0.3.9`
- [ ] squeeze's README states the outcome (recommendation in the PR body; not edited from here) (separate PR in `nixpt/squeeze`, owner's call)

## Technical approach

- Keep crush-pkg's existing subcommands unchanged.

## Files to modify

- `crates/crush-pkg/src/main.rs` (or `cli.rs`)
- `crates/crush-pkg/tests/`

## Non-goals

- squeeze's M3–M6 roadmap

## Source (reference only — re-implement, don't copy)

- `nixpt/squeeze` `src/main.rs`; MIGRATION-INVENTORY §4

## Evidence (panini-d, 2026-10-07, commit `132ddd4`)

**Delivered**
- Bare `crush-pkg` (previously a clap error, so this is additive): build →
  write `target/<name>.cvm` + `.casm.json` → run the built program.
  `crush-pkg -- ARGS` passes args through. `--message-format` works on both
  sides of a subcommand, as before.
- Script/Native capsules skip the build and go through `handle_run`'s runner
  dispatch, args included.
- `crush-pkg build`/`check` refuse Script/Native capsules with
  ``crush-pkg build` only applies to Crush-source capsules (manifest declares
  language = "python"); use `crush-pkg run` …`` under `E-BUILDER` (text and
  NDJSON).
- Library: `crush_pkg::flow::{buildability, Buildability,
  require_crush_buildable}`, `CrushRunner::run_program`. Additive; crush-pkg
  has never been published.

**Departures from squeeze (both are fixes)**
1. squeeze ran by re-dispatching the entry through the runner, which
   recompiles the entry alone. Any package whose entry calls a path-dep
   function fails that way (`Undefined function: greet`). The bare flow runs
   the `Program` that `build` produced.
2. There's no separate `check` pass: `PackageBuilder::check` compiles each file
   alone and fails on the same packages (captured in TASKS.md). `build`
   compiles everything, so it's the stronger check.

**Not done / out of scope**
- No thin `squeeze` binary. It isn't free: a second name on every
  `cargo install crush-pkg` user's PATH, plus a second CLI to keep in sync.
- Crush programs still can't see args: `CrushRunner` ignores them, and no
  argv capability exists. Same as `crush-pkg run -- ARGS` today; captured as
  a gap.

**Verification**
- Live: `crush-pkg new app` + bare → `hello from Crush`, `target/` written.
  For the app + `util` path dep: bare → `hi from util`, while `run` →
  `Undefined function: greet`. For a python capsule: `build`/`check` refused,
  and bare `-- a b` → `py args: ['a', 'b']` (via buckets) with no `target/`.
- `tests/test_default_flow.rs`, 5 tests against the real binary:
  - the built program runs, with a `run` control that fails;
  - a compile error gives `E-BUILDER` and writes no `target/`;
  - `build`/`check` are refused (text and JSON);
  - a native `/bin/echo` capsule receives `-- alpha beta` and isn't built;
  - stray args after `build` are rejected.
- Plus 4 `flow` unit tests and a clap parse test.
- `cargo test -p crush-pkg` green. Clippy: nothing in new code.
- The existing `cli_lint_subcommand_parses…` test caught one regression from
  `args_conflicts_with_subcommands` (it broke `--message-format=json lint`).
  That setting was dropped.
