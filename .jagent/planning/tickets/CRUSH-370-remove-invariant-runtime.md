# CRUSH-370 — Remove `crushc --invariant-runtime` (it called a capability that does not exist)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-370 |
| **Priority** | P3 |
| **Status** | In review |
| **Assignee** | nova |
| **Dependencies** | — |

## Problem

`crushc --invariant-runtime` prepended `CAP_CALL "invariant.evaluate" 0` (no
arguments) to each function an `@invariant` with a `check_source` applies to.
No host registers `invariant.evaluate`, and the call carries neither the
expression nor the function's arguments, so a program built with the flag
failed at run time (`unknown capability`) — the opposite of a runtime check.

## Decision (the smaller correct option)

Implementing it means compiling an arbitrary check expression in the context
of each function's parameters, running it in-VM, and failing with the
invariant's name — a feature, not a fix. Instead:

- `crushc --invariant-runtime` is an error naming the reason (flag kept, hidden,
  so the message is better than clap's "unexpected argument").
- `Compiler::with_invariant_runtime` is `#[deprecated]` with the same note; its
  emission tests are kept (and `allow(deprecated)`) so the behaviour stays pinned
  until the API is dropped.
- `@invariant` blocks remain documentation + `codebase.invariants(name)`
  queries; nothing evaluates `check_source`. If in-VM evaluation is wanted, it
  needs its own design (expression compile, argument access, failure type).

## Success criteria

- [x] `crushc --invariant-runtime` exits 1 with a message naming `invariant.evaluate`.
- [x] No artifact written. Test: `annotation_warnings_cli::invariant_runtime_flag_is_a_clear_error`.
- [x] `docs/design/compile-pipeline.md` no longer lists the flag.
