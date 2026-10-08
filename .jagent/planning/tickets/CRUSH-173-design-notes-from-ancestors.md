# CRUSH-173 — Design notes recovered from the ancestors

| Field | Value |
|-------|-------|
| **ID** | CRUSH-173 |
| **Priority** | P4 |
| **Status** | Backlog |
| **Phase** | M0 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S (~40 turns) |
| **Relay lane** | E4 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

A few ancestor documents describe crush-ast-relevant design that exists nowhere in this repo: the import system (`nixpt/crush` `docs/architecture/import-system.md`), the compile pipeline and walker-authoring chapters (crushed-book `reference/advanced/`), the SBL/corecap layering and CASM-utilities ideas, and a WIT/component-model RFC seed.

## Success criteria

- [ ] `docs/design/import-system.md` — written against `crates/crush-frontend/src/import_system.rs` and CRUSH-110 (import is a no-op): what is specified vs implemented
- [ ] `docs/design/compile-pipeline.md` — the real `crushc` / `crush-run` pipeline (note CRUSH-141's divergence)
- [ ] `docs/design/walker-authoring.md` — `Frontend` / `LanguageAdapter` traits, not the old `walker-core`
- [ ] a short `docs/design/sbl.md` (what `system.*` is, why it runs as Crush)
- [ ] no overclaims: every statement checked against code; no copied text (sources are unlicensed)

## Technical approach

- Rewrite from the code; use the ancestors only for structure.

## Files to modify

- `docs/design/*.md`

## Non-goals

- the language guide (separate repo)

## Source (reference only — re-implement, don't copy)

- MIGRATION-INVENTORY §1b, §1c
