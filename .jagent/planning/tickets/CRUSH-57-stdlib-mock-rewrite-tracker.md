# CRUSH-57 — STDLIB mock-rewrite tracker (46 mock-tainted caps; per-cap tickets on demand)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-57 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M9 |

## Problem

46 archived capabilities are mock-tainted: their archived implementations
contain stub/mock behavior that would restore as silent corruption. Each must
be REWRITTEN from spec, not copied. Rewrites touch behavior → each needs its
own ticket with spec provenance (where the behavioral contract comes from:
docs, call sites, sibling caps) recorded via `dejavue decision`.

## Approach

This ticket = the process + the list, NOT 46 pre-minted tickets (deliberate:
pre-minting without per-cap spec provenance would produce 46 empty shells).
1. From CRUSH-56's map: the 46-cap list with per-cap taint reason.
2. Rewrite-ticket template: spec source, behavioral contract, @covers test,
   provenance line.
3. Mint per-cap tickets (CRUSH-106+) in demand order (what M9 consumers
   actually call first — CRUSH-71's client matrix ranks this).

## Definition of done

- [ ] 46-cap list + taint reasons committed
- [ ] Rewrite template committed
- [ ] First 5 per-cap tickets minted in ranked order

## Files in scope

- This tracker + template; per-cap tickets as minted

## Gates

CRUSH-56 (the map).
