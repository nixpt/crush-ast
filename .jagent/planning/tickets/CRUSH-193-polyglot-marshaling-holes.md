# CRUSH-193 — Polyglot marshaling: Python read-modify-write not injected; blocks inside `try` get no marshaling

| Field | Value |
|-------|-------|
| **ID** | CRUSH-193 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

Both in `crates/crush-lang-sdk/src/compile.rs` `prepare_stmts`:

- `let n = 5` then `@python {\nn = n * 2\n}` → `NameError: name 'n' is not defined`.
  The Python analyzer excludes names that are also bound from `reads`. The JS
  equivalent works.
- `let n = 5` + `try { @python { m = n*2 } print(m) } catch e {}` →
  `Undefined variable: m`; the same code inside `if` works. There is no `TryCatch` arm (~line 170).
- `@bash`/`@python` output loses its trailing newline (`bash 5` and the next print merge).

## Success criteria

- [ ] Read-modify-write names are both injected and extracted.
- [ ] `prepare_stmts` recurses into try/catch (and any other block-bearing statement).
- [ ] Polyglot output newlines preserved.
