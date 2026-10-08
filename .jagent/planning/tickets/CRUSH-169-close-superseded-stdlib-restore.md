# CRUSH-169 — Close the archive-zip stdlib restoration tickets as superseded

| Field | Value |
|-------|-------|
| **ID** | CRUSH-169 |
| **Priority** | P4 |
| **Status** | Done (PR pending foreman approval) |
| **Phase** | M9 |
| **Assignee** | panini-e |
| **Dependencies** | foreman approval |
| **Estimated effort** | XS (~10 turns) |
| **Relay lane** | E2 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

CRUSH-56 (tracker), CRUSH-57 (mock rewrites), CRUSH-88..97 (ten identical shard templates, no cap lists) and CRUSH-108 (source reconcile) plan a restore from `exosphere-1.0.zip`. CRUSH-122 already restored the clean families from exosphere's live tree; the 46 mock-tainted caps are the families MIGRATION-INVENTORY §2.2 classifies dead/out; the real remainder is CRUSH-151..155.

## Success criteria

- [x] each ticket marked Superseded with a pointer to CRUSH-122 + MIGRATION-INVENTORY §2.5
- [x] TASKS / BACKLOG-INDEX / ROADMAP M9 rows updated ("103/46 caps" wording removed)
- [x] a `dejavue decision` recording why

## Technical approach

- Planning-only; needs foreman's ok before closing.

## Files to modify

- `.jagent/planning/tickets/CRUSH-{56,57,88..97,108}*.md`
- `.jagent/planning/{TASKS,BACKLOG-INDEX,ROADMAP}.md`

## Non-goals

- deleting ticket files

## Source (reference only — re-implement, don't copy)

- MIGRATION-INVENTORY §2.5

## Evidence (2026-10-07, panini-e)

Status changes (each ticket gets a `## Resolution` section; files kept, not deleted):

| Ticket | New status | Why |
|---|---|---|
| CRUSH-56, CRUSH-57 | Superseded | CRUSH-122 + MIGRATION-INVENTORY §2.5 |
| CRUSH-88 … CRUSH-97 (10 shards) | Superseded | same; real remainder CRUSH-151..155 |
| CRUSH-108 (`stdlib-reconcile-source-and-dedupe`) | Superseded | same (its question — zip vs live tree — is answered in §2.3/§2.5) |
| CRUSH-162 | Declined | captain C-2: no Lua unless someone asks |
| CRUSH-163 | Deferred | captain C-3: decide with CRUSH-77 |
| CRUSH-174 | Superseded | crushlang.org/playground (HTTP 200, 2026-10-07) + CRUSH-118 (#93) |
| CRUSH-154 | (not edited here) | declined under C-6 by lane A, crush-ast#102 |

Planning rows: TASKS (162/163/174/169 rows, the "STDLIB RESTORATION MAP" backlog line, the M9 summary),
BACKLOG-INDEX (108 row, 121+ note, M9 table), ROADMAP (M9 table row, M9 checklist, proposed-ticket list,
the "103 + 46 = 149 caps" risk note). Decision recorded with `dejavue decision`.

Found on the way: the ID **CRUSH-108 is used twice** (`CRUSH-108-jit-nan-eq-test-failing-main.md` too).
Only the stdlib one is closed here; captured with `dejavue plan`.
