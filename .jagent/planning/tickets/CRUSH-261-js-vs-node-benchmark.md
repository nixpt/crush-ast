# CRUSH-261 — JS → AOT vs Node benchmark (progress gate for CRUSH-257..260)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-261 |
| **Priority** | P2 |
| **Status** | In this PR (harness + baseline) |
| **Phase** | M2 |
| **Assignee** | vega |
| **Dependencies** | none |
| **Estimated effort** | S |

Harness and 2026-10-12 baseline: `docs/benchmarks/js_vs_node/` (`README.md`, `jsaot` = JS → swc walker → CAST →
CASM → AOT `.so`, `run.py` = output check vs Node + median-of-7 wall time, peak RSS, artifact size).

## Done when

- [x] Harness + baseline committed.
- [ ] Re-run and append results after each of CRUSH-257/258/259/260 lands.
