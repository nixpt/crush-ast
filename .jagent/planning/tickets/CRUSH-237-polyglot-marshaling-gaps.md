# CRUSH-237 — Polyglot marshaling: comprehension inputs not passed; analyzer failure is silent

| Field | Value |
|-------|-------|
| **ID** | CRUSH-237 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M4 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-130 |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 use-case review (agents + polyglot), on `main` `2d71b96` |

## Problem

- `@python { ys = [x * 2 for x in xs] }` fails with `NameError: xs`: the free-variable
  analyzer skips comprehensions (`crates/crush-lang-python/src/analyzer.rs:~322-324`).
- When the analyzer can't parse a block, the block runs with no variables passed in and no
  warning (`crates/crush-lang-sdk/src/compile.rs:~58-60`).
- Bash blocks get text only (no JSON, no write-back).

## Success criteria

- [ ] Comprehension, lambda and generator free variables are marshaled; test per form.
- [ ] Analyzer failure is a compile error (or at least a warning with the block location).
- [ ] Bash limitation documented in the language guide.
