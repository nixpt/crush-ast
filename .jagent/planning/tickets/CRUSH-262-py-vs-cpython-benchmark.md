# CRUSH-262 — Python → AOT vs CPython benchmark (progress gate)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-262 |
| **Priority** | P2 |
| **Status** | Done (harness + 2026-10-12 baseline in README) |
| **Phase** | M2 / M6 |
| **Assignee** | nixp |
| **Dependencies** | CRUSH-200 (Python `/` `//` `%` + `py.print`); twin of CRUSH-261 |
| **Estimated effort** | S |

Harness: `docs/benchmarks/py_vs_cpython/` (`README.md`, `pyaot` = Python → rustpython walker →
CAST → CASM → AOT `.so`, `run.py` = output check vs CPython + median-of-7 wall time, peak RSS,
artifact size).

## Done when

- [x] Harness committed (fib / loop / sieve / strings).
- [x] Baseline numbers recorded in `README.md` from a real `perf` run (2026-10-12).
- [ ] Re-run and append results after AOT compute-gap work (CRUSH-257/258) and further Python
      walker fixes.
