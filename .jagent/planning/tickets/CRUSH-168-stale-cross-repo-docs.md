# CRUSH-168 — Fix stale in-code docs pointing at exosphere/ecasm

| Field | Value |
|-------|-------|
| **ID** | CRUSH-168 |
| **Priority** | P4 |
| **Status** | Done (PR pending review) |
| **Phase** | M0 |
| **Assignee** | panini-e |
| **Dependencies** | none |
| **Estimated effort** | XS (~15 turns) |
| **Relay lane** | E1 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

Several crush-ast docs still describe things that moved or were deleted: `crates/casm/src/lib.rs` mentions `ecasm.rs` (removed by CRUSH-80); `crates/crush-cast/STATUS.md` points at exosphere paths (`../base/errors`, `crates/core/base/stdlib/`) that don't exist here; `crates/crush-lang-sdk/src/stdlib.rs` says stdcaps are "always available" (they need the `stdlib` feature — CRUSH-113).

## Success criteria

- [x] each stale reference fixed or removed
- [x] `crush-cast/STATUS.md` names crush-ast's own stdlib (`crush-lang-sdk/src/stdlib*`) and links MIGRATION-INVENTORY
- [x] no code changes

## Technical approach

- Docs-only sweep.

## Files to modify

- `crates/casm/src/lib.rs` (comment)
- `crates/crush-cast/STATUS.md`
- `crates/crush-lang-sdk/src/stdlib.rs` (header, if CRUSH-113 hasn't already)

## Non-goals

- rewriting the guide

## Source (reference only — re-implement, don't copy)

- MIGRATION-INVENTORY §1–2

## Evidence (2026-10-07, panini-e)

Sweep: `git grep -n 'EXO-205\|EXO-151\|ecasm\|CRUSH-55\|nanovm\|crates/core/'` outside `.jagent/`, `.dejavue/`
and the inventory itself.

- `crates/casm/src/lib.rs` + `crates/casm/tests/ver02_version_gate.rs`: dropped the "uncompilable `ecasm.rs`
  inline tests (EXO-151)" rationale — the file is gone (CRUSH-80).
- `crates/casm/README.md`: pipeline diagram ended at "NanoVM"; now names crush-vm / crush-jit / crush-aot.
- `crates/crush-cast/STATUS.md`: publication table (said unpublished, `../base/errors` path dep — crush-cast is
  on crates.io at 0.3.9 and uses `crush-errors.workspace = true`); `crush-lang`/`--from-cast` references
  (there is no such crate or flag here — `crush_frontend::compile_cast`); the whole "stdlib status" section
  (pointed at exosphere's `crates/core/base/stdlib` + corecaps) replaced by "Where the stdlib lives" naming
  `crush-lang-sdk/src/stdlib*`, `sbl/`, `host_caps.rs` and linking MIGRATION-INVENTORY §2; open questions
  dropped exosphere ticket/file references.
- `.jagent/planning/research/2026-09-25-CRUSH-55-delta-inventory.md`: header note pointing at
  MIGRATION-INVENTORY (and recording that exosphere's EXO-205 doc is history).
- `docs/tasks/vm-pipeline-gaps.md`: "wire ecasm.rs into pipeline" struck as obsolete.
- `crates/crush-lang-sdk/src/stdlib.rs` header: **left alone** — CRUSH-113 (#97, lane A) rewrites it.
- Left as-is on purpose: dated snapshots (`docs/design/readiness-matrix.md` 2026-07-14,
  `docs/design/CRUSH-71-design-audit.md`), CHANGELOG history, and `crates/crush-web/README.md`'s
  explanation of why the exosphere nanovm stack could not target wasm (accurate history).
