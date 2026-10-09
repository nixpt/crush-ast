# CRUSH-207 — tree-sitter-crush is out of date: ERROR nodes in 43/43 examples

| Field | Value |
|-------|-------|
| **ID** | CRUSH-207 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M3 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

Parsing every `examples/crush/*.crush` with the generated parser (gcc harness,
tree-sitter 0.25 runtime) gives ERROR nodes in **43/43** files, and in all 6 of the
grammar's own `test_*.crush` fixtures. With `//` comments stripped, 21/43 still fail.
Editors and crush-lsp consume this grammar.

Rejected by the grammar, accepted by `crushc --check`:
- `// comment` (grammar only knows `#`, `grammar.js:~84`)
- plain reassignment `x = 2`
- return types `fn fib(n: Int) -> Int {…}`
- `if s == "a" { s = "b" }`
- `@decision "a" { … }`

Accepted by the grammar, rejected by crushc: a call split across lines
`f(1,\n 2)` ("unexpected token in expression: newline").

Tracked only for lambdas (CRUSH-75 / #78).

## Success criteria

- [ ] CI job: parse every `examples/crush/*.crush` with tree-sitter and fail on ERROR nodes.
- [ ] Grammar updated to the frontend's current syntax; differential test against
      `crushc --check` over the corpus.
