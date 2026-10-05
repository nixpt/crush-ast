# CRUSH-141 — `crushc` and `crush-run x.crush` still compile differently

| Field | Value |
|-------|-------|
| **ID** | CRUSH-141 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-130 |
| **Estimated effort** | S |

## Problem

CRUSH-130 made `crushc` run the polyglot pass, but its pipeline (`crates/crush-lang-sdk/src/bin/crushc.rs`) still differs from `compile_crush_source` (what `crush-run run x.crush` uses):

- it calls `Parser::parse` directly, so `cast_enrich::enrich_cast` (fills `Program.exhaustive_sites`) never runs;
- the optimizer runs only with `--optimize`, while `compile_cast_owned` always optimizes — so optimizer bugs (e.g. CRUSH-131) show up under `crush-run` but not under a plain `crushc` build, and vice versa.

## Success criteria

- [ ] decide: one shared pipeline function both tools call (crushc keeping its staged diagnostics), or document the differences as intended
- [ ] a test that the same program produces identical bytecode via both paths (or the documented difference)

## Files to modify

- `crates/crush-lang-sdk/src/bin/crushc.rs`, `crates/crush-lang-sdk/src/compile.rs`
