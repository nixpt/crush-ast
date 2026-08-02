# CRUSH-71 — Design audit + client survey + top-win implementation campaign

| Field | Value |
|-------|-------|
| **ID** | CRUSH-71 |
| **Priority** | P1 |
| **Status** | In Progress (panini persona-session, s412 2026-08-02) |
| **Phase** | Correctness spine / M10-prep (feeds BACKLOG-INDEX ranking) |
| **Branch** | `agent/panini-crush/CRUSH-71` |

## Why this ticket file exists

The campaign was dispatched directly (captain directive s412: "explore and
improve the design by 3000x, check crush clients as well") before a ticket
file existed; triage flagged the ID as live-but-unfiled. This file makes the
ID's allocation durable. The dispatch prompt is at
`/home/nixp/WORKSPACE/workspace-meta/prompts/2026-08-02-CRUSH-71.txt`.

History note: the branch's first commits were briefly mislabeled CRUSH-21
(ID collision with the java-kotlin family ticket, caught mid-launch);
`ecf51c1` renamed the audit doc, `a5a24c8` preserved the first run's nine
dejavue captures (since minted as tickets CRUSH-79..86).

## Scope (as dispatched)

1. Measured bench baseline + design walk of parser → CAST → CASM → PortableVm
   → JIT; hunt design-level (not micro-opt) wins.
2. Read-only client survey across the 11 consumer repos (exosphere/crush-symbols,
   razor, bro-cli, exo-light, mycelium-node, squeeze, crush-notebook,
   crush-visuals, polydex, crush-lsp, bozo): consumed API surface × ripple risk.
3. Implement the top-ranked win with before/after bench numbers; capture the
   rest via `dejavue plan`.

## Definition of done

Per the dispatch prompt: `docs/design/CRUSH-71-design-audit.md` committed with
baseline numbers + ranked opportunities + client matrix; top win implemented
with quoted before/after benches; `cargo test --workspace` not regressed;
branch pushed with incremental commits.

## Post-close action (foreman)

Fold the audit's ranking into `.jagent/planning/BACKLOG-INDEX.md` (open
questions listed at its bottom) and mint any tickets the client matrix adds.
