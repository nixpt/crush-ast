# CRUSH-208 — crush-lint is a heuristic mock: false positives on valid code, writes to `./.dejavue/timeline.jsonl`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-208 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M7 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

- Flags 14/43 valid examples. The "Missing semicolon" rule is
  `!line.contains(";") && line.contains("let ")` (`crates/crush-lint/src/main.rs:~44-52`);
  semicolons are optional in Crush (`fibonacci.crush:9` flagged; crushc accepts it).
- Every diagnostic is appended to `./.dejavue/timeline.jsonl` relative to the CWD
  (`lib.rs:~173`). Running it inside the repo dirties the tree (146 lines in one
  sweep). No flag to disable it; undocumented.

## Success criteria

- [ ] Lint rules run on the parsed AST (crush-frontend), not line substrings; zero
      findings on `examples/crush/` that crushc accepts (or the finding is real).
- [ ] Timeline writes are opt-in (`--record`), never implicit.
