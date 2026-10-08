# CRUSH-172 — Example capsule: `squad-bridge-peek` (first pure-Crush capsule)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-172 |
| **Priority** | P5 |
| **Status** | Done (PR pending review) |
| **Phase** | M4 |
| **Assignee** | panini |
| **Dependencies** | CRUSH-151 |
| **Estimated effort** | XS (~15 turns) |
| **Relay lane** | E3 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

crush-capsules' `squad-bridge-peek` is the only pure-Crush capsule outside crush-ast and matches crush-pkg's manifest shape. It needs `fs.cat` (CRUSH-151) and hard-codes a personal absolute path.

## Success criteria

- [x] `examples/crush/capsules/squad-bridge-peek/` with `capsule.toml` + `main.crush`, path taken from an argument/env grant, no personal paths
- [x] `crush-pkg build` + `run` work on it with `--fs` scoped to a fixture file; CI or conformance covers it

## Technical approach

- Re-create, don't copy verbatim.

## Files to modify

- `examples/crush/capsules/squad-bridge-peek/`

## Non-goals

- the other crush-capsules (dead)

## Source (reference only — re-implement, don't copy)

- `nixpt/crush-capsules` `squad-bridge-peek/`

## Note (2026-10-07, panini-e, lane E3) — skipped, not started

Needs `fs.cat` from **CRUSH-151**, which is in review (crush-ast#98) but not merged on `main`, so this
ticket was skipped in the phase-2 relay per the dispatch rule. Pick it up once #98 lands; nothing was
built for it yet.

## Outcome (panini, 2026-10-08)

- `examples/crush/capsules/squad-bridge-peek/`: `capsule.toml` (`category = "example"`,
  native `platforms` only — it needs `fs`/`env`, which the browser doesn't grant), `main.crush`
  (last 5 non-empty lines of the log), `fixtures/bridge.md`. The file is `$BRIDGE_PEEK_FILE`
  (via `--env`), else `bridge.md`, resolved inside `--fs-root`; no built-in path.
- `crush-pkg` had no way to grant anything, so `fs.cat` could never run under it. `run` and
  bare `crush-pkg` now take `--fs` / `--fs-root` / `--env` / `--time` (same meaning as
  `crush run`) and register the pure stdlib. Documented in `crates/crush-pkg/MANIFEST.md`.
- `crates/crush-pkg/tests/test_example_capsules.rs`: build + run + bare under the grants,
  refusal without each grant, and no reads outside `--fs-root` (`../`, absolute). No CI job ran
  crush-pkg tests before (`Test (workspace)` is `--no-run`); `Test (sdk)` now runs them.
