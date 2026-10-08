# CRUSH-173 — Design notes recovered from the ancestors

| Field | Value |
|-------|-------|
| **ID** | CRUSH-173 |
| **Priority** | P4 |
| **Status** | Done (PR pending review) |
| **Phase** | M0 |
| **Assignee** | panini-e |
| **Dependencies** | none |
| **Estimated effort** | S (~40 turns) |
| **Relay lane** | E4 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

A few ancestor documents describe crush-ast-relevant design that exists nowhere in this repo: the import system (`nixpt/crush` `docs/architecture/import-system.md`), the compile pipeline and walker-authoring chapters (crushed-book `reference/advanced/`), the SBL/corecap layering and CASM-utilities ideas, and a WIT/component-model RFC seed.

## Success criteria

- [x] `docs/design/import-system.md` — written against `crates/crush-frontend/src/import_system.rs` and CRUSH-110 (import is a no-op): what is specified vs implemented
- [x] `docs/design/compile-pipeline.md` — the real `crushc` / `crush-run` pipeline (note CRUSH-141's divergence)
- [x] `docs/design/walker-authoring.md` — `Frontend` / `LanguageAdapter` traits, not the old `walker-core`
- [x] a short `docs/design/sbl.md` (what `system.*` is, why it runs as Crush)
- [x] no overclaims: every statement checked against code; no copied text (sources are unlicensed)

## Technical approach

- Rewrite from the code; use the ancestors only for structure.

## Files to modify

- `docs/design/*.md`

## Non-goals

- the language guide (separate repo)

## Source (reference only — re-implement, don't copy)

- MIGRATION-INVENTORY §1b, §1c

## Evidence (2026-10-07, panini-e)

Four notes under `docs/design/`, written from the code (file:line research per topic, then key claims
re-read by hand); none of the unlicensed ancestor sources (crushed-book, walker, joker-coordinator,
incubator) were opened — structure only from MIGRATION-INVENTORY's summaries.

- `import-system.md` — five parsed forms → `ImportStatement`; compiler lowers each to an unregistered
  cap (`module.load`, `external.load`, …); semantics ignores imports; `import_system.rs` never called;
  CRUSH-110 confirmed. Live: `import helpers` + `crush-run` → `[runtime] unknown capability:
  module.load`, exit 1. Direction section for CRUSH-110 (compile-time merge, one resolver).
- `compile-pipeline.md` — stages, the three "CASM" forms (incl. the `.casm` JSON-vs-text clash), every
  binary's real path and engine, the capability checks, and CRUSH-141's divergence table (plus
  `crush-diff` skipping `prepare_polyglot_blocks`).
- `walker-authoring.md` — `Frontend`/`LanguageAdapter`/legacy `Walker`, the three invocation paths (only
  `AdapterRegistry` ships), lowering conventions, crate checklist; flags the dotted-extension trap.
- `sbl.md` — `system.*`, per-call fresh PortableVm on the pure stdlib, why Crush; the capability-layer
  table; built-in names shadow host caps; WIT note (nothing implemented; prerequisites).

Found and captured (`dejavue plan`): top-level `stdlib/README.md` claims `import` loads modules at
compile time; `crush-walker-core/README.md` example doesn't compile; extension tables duplicated 4x.
