# CRUSH-8 — Two shipped `examples/crush/*.crush` files don't actually run

| Field | Value |
|-------|-------|
| **ID** | CRUSH-8 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

Found while porting `examples/crush/snake.crush` — needed real working
examples to confirm syntax against, and two of the existing "real, verified"
example files fail outright against the current `crushc`/`crush-run`:

1. **`examples/crush/fibonacci.crush`** fails to *compile*:
   `[type] type error: Invalid binary op + for types null and null`. Uses
   typed params/return (`fn fib(n: Int) -> Int`). A minimal untyped
   recursive function (`fn count_down(n) { ... return count_down(n - 1); }`)
   compiles and runs fine, so the failure is plausibly specific to typed
   parameter/return-type annotations on a recursive function, not
   recursion itself — not confirmed further, this needs someone to
   actually debug it.
2. **`examples/crush/arrays_and_loops.crush`** compiles, but fails at
   *runtime*: `[runtime] stack quota exceeded (4096)`. Plausibly related to
   `print("Array created: " + arr)` (concatenating a whole array directly
   into a string) — not confirmed, needs debugging.

Neither of these has anything to do with `CRUSH-7` (array
mutation) — `fibonacci.crush` doesn't touch arrays at all, and
`arrays_and_loops.crush`'s failure is a stack quota during what should be
straightforward printing, not an array-mutation operation.

## Impact

These are the two most basic/canonical examples in the directory (a
recursive-function demo and an arrays-and-loops demo) — if someone new to
the language runs them expecting a working reference and hits a compile
error or a runtime stack-quota crash, that's a bad first impression, and it
means `examples/README.md`'s claim that these are drawn from "the
exosphere test suite" and presumably once passed is now stale.

## Reproduction

```bash
crushc examples/crush/fibonacci.crush -o /tmp/fib.cvm1
# [type] type error: Invalid binary op + for types null and null

crushc examples/crush/arrays_and_loops.crush -o /tmp/aal.cvm1   # succeeds
crush-run run /tmp/aal.cvm1
# [runtime] stack quota exceeded (4096)
```

## Technical approach

Not investigated — found via black-box testing while porting a new
example, not via source-diving. Whoever picks this up should:

1. Bisect `fibonacci.crush`: try `fn fib(n: Int) -> Int` vs `fn fib(n)`
   (no annotations) vs `fn fib(n: Int)` (typed param, no return type) to
   isolate which annotation triggers the type error.
2. Bisect `arrays_and_loops.crush`: try `print("x: " + arr)` (string +
   array) in isolation vs the rest of the file, to confirm/deny that's the
   stack-quota trigger.
3. Once root-caused, either fix the compiler bug or fix the example file
   to avoid the broken pattern (whichever is correct depends on whether
   the pattern *should* be supported).

## Files to modify

- `examples/crush/fibonacci.crush` and/or `crates/crush-frontend/` (typed
  recursive function type-checking)
- `examples/crush/arrays_and_loops.crush` and/or `crates/crush-vm/` (string
  + array concatenation, or whatever `+` does when the RHS is an array)

## Non-goals

- Auditing every other file in `examples/crush/` for similar rot — this
  ticket is just the two found incidentally; a full sweep is separate work
  if wanted
