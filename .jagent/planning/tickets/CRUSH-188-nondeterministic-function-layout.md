# CRUSH-188 — Codegen function order is nondeterministic (`Program.functions` is a `HashMap`)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-188 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

Compiling the same file six times produced two different CASM outputs. Function
order follows `HashMap` iteration (`crates/crush-cast/src/lib.rs:38`). This breaks
reproducible builds and caching, and makes layout-dependent bugs (CRUSH-187)
change symptom from run to run.

## Success criteria

- [ ] `crushc --emit casm` is byte-identical across runs (use `BTreeMap`/`IndexMap`
      or sort by source order before emission).
- [ ] Test compiling a multi-function program twice and comparing bytes.
