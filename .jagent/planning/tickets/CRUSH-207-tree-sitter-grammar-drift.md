# CRUSH-207 — tree-sitter-crush is out of date: ERROR nodes in 43/43 examples

| Field | Value |
|-------|-------|
| **ID** | CRUSH-207 |
| **Priority** | P1 |
| **Status** | Partially done (grammar + CI check; differential test remains) |
| **Phase** | M3 |
| **Assignee** | nixp |
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

- [x] CI job: parse every `examples/crush/*.crush` with tree-sitter and fail on ERROR nodes.
- [ ] Grammar updated to the frontend's current syntax; differential test against
      `crushc --check` over the corpus.

## Resolution (2026-10-30, `agent/nixp/CRUSH-207`)

Measured with the crate's own parser over `examples/crush/*.crush`:

| | before | after |
|---|---|---|
| parse with no ERROR node | **1 / 51** | **49 / 51** |

Grammar fixes in `crates/tree-sitter-crush/grammar.js`, each traced to the
frontend (`crates/crush-frontend/src/parser/`):

- **Comments.** The rule was `/#.*/` — only hash comments — so almost every
  real file (which opens with `//`) died at line 1. Now `//`, `/* … */` and `#`
  (lexer.rs `read_comment`). *This alone: 1 → 22 files.*
- **Assignment.** `field_assignment` only accepted `a.b = c`; plain `x = 1`
  and `xs[0] = 9` were ERRORs. Replaced by `assignment` covering all three
  targets (parser/mod.rs `parse_expression_statement`). *22 → 32.*
- **Function return types** (`fn fib(n: Int) -> Int`), `pub`/`async` modifiers.
- **String escapes** — `"a\"b"` broke the old `/[^"]*/`. *32 → 39.*
- **`&&`, `||`, `!`/`not`, unary `-`** — the operator table had no logical ops.
- **Chained field access** (`r.origin.x`) and a distinct `@`-call alternative
  (the old `optional('@')` was ambiguous once binary ops followed).
- **Object literals** (`{"a": 1}`), **struct literals** (`Point { x: 1 }`,
  `new Point { x: 1 }`), `null`, `capability <name> <access>`.
- **AI-native annotations** (`@module`, `@invariant`, `@decision`, `@wip`,
  `@temporary`, `@errors`, …), keyed off the lexer's `IMPORT_KEYWORDS` list.
- **`semantic_switch`** and **named arguments** (`f(x, constraints=[…])`).
- **Polyglot bodies** now brace-balance, so `@javascript { const o = {a: 1}; }`
  ends at the outer `}`. *39 → 49.*

Object/struct literals are deliberately absent from `expression` (they live in a
new `_value_expression`, used after `=`/`return`/in arguments) — allowing them
as statements is what made `if x { … }` and `@module { … }` ambiguous.

### The CI check

`crates/tree-sitter-crush/tests/grammar_examples.rs` parses every
`examples/crush/*.crush` and the crate's own `test_*.crush` fixtures with the
bundled parser and fails on any ERROR/MISSING node. It runs under the existing
`test-rest` job (`cargo test --tests -p tree-sitter-crush`), so no new CI job
was needed. The two remaining files are in a `KNOWN_BAD` allowlist with reasons;
the list **only shrinks** — an entry that starts parsing cleanly fails the test
until it is deleted.

Verified RED: reverting just the comment rule fails the test with 32 files.

Also: `test/corpus/basic.txt` expectations refreshed (they had drifted from the
grammar itself — 5 of 6 failed), a new `test/corpus/modern.txt` pins the syntax
added here (9 cases), and the README no longer claims `crush-frontend` uses this
grammar (it has its own hand-written parser).

## Remaining

- **Differential test against `crushc --check`.** Criterion 2's second half: run
  the frontend over the corpus and assert the grammar and the frontend agree on
  accept/reject, rather than only "no ERROR nodes". That is what would catch the
  grammar being *more* permissive than the frontend (the current check cannot:
  `dashboard.crush` is rejected by both, but for different reasons, and the
  frontend rejects multi-line calls the grammar accepts).
- **The two allowlisted files.** `dashboard.crush` is an `// expect-error:`
  example (the frontend rejects it at line 30 for a multi-line call) and stops
  the grammar at its `catch (e)`, which is not valid Crush either.
  `polyglot_braces.crush` case 4 puts a bare `{` inside a Python triple-quoted
  string, which no context-free grammar can see.
- **Regenerating `parser.c` needs tree-sitter CLI ≥ 0.25** (the committed
  parser.c is ABI 15; 0.24 refuses it). Noted in the crate README.
