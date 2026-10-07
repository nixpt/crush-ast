# CRUSH-169 — Close the archive-zip stdlib restoration tickets as superseded

| Field | Value |
|-------|-------|
| **ID** | CRUSH-169 |
| **Priority** | P4 |
| **Status** | Backlog |
| **Phase** | M9 |
| **Assignee** | unassigned |
| **Dependencies** | foreman approval |
| **Estimated effort** | XS (~10 turns) |
| **Relay lane** | E2 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

CRUSH-56 (tracker), CRUSH-57 (mock rewrites), CRUSH-88..97 (ten identical shard templates, no cap lists) and CRUSH-108 (source reconcile) plan a restore from `exosphere-1.0.zip`. CRUSH-122 already restored the clean families from exosphere's live tree; the 46 mock-tainted caps are the families MIGRATION-INVENTORY §2.2 classifies dead/out; the real remainder is CRUSH-151..155.

## Success criteria

- [ ] each ticket marked Superseded with a pointer to CRUSH-122 + MIGRATION-INVENTORY §2.5
- [ ] TASKS / BACKLOG-INDEX / ROADMAP M9 rows updated ("103/46 caps" wording removed)
- [ ] a `dejavue decision` recording why

## Technical approach

- Planning-only; needs foreman's ok before closing.

## Files to modify

- `.jagent/planning/tickets/CRUSH-{56,57,88..97,108}*.md`
- `.jagent/planning/{TASKS,BACKLOG-INDEX,ROADMAP}.md`

## Non-goals

- deleting ticket files

## Source (reference only — re-implement, don't copy)

- MIGRATION-INVENTORY §2.5
