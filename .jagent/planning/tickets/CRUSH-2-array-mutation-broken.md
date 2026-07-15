# CRUSH-2 — Array mutation is effectively unusable

| Field | Value |
|-------|-------|
| **ID** | CRUSH-2 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |

## Problem

Found while porting `examples/crush/snake.crush` (a self-playing Snake
simulation) against the real `crushc`/`crush-run` toolchain. Every array
"mutation" path is either unsupported at the parser level or broken at
runtime, which forced the port to avoid arrays for game state entirely and
use fixed-arity recursive function arguments instead:

1. **Index assignment is a parse error.** `xs[0] = 9;` fails with
   `[E-PP01] Unexpected token in expression: Assign` — the parser's
   assignment-statement grammar accepts `Identifier = expr` and
   `Statement::SetField` (`obj.field = expr`) as valid targets, but not
   `Expression::Index` as an lvalue. No `arr_set(xs, 0, 9)` builtin-function
   form exists either (`Undefined function: arr_set`); `xs.set(0, 9)`
   compiles but is misrouted to capability dispatch at runtime
   (`unknown capability: set`).
2. **`.push()`/`.append()` work once, but chaining is broken.**
   `let a2 = a1.push(2);` works. `let a3 = a2.push(3);` (pushing onto a
   value that was itself produced by a prior `.push()`) fails at runtime
   with `stack underflow` — confirmed with fresh `let` bindings (not
   reassignment), so it's not a reassignment issue, and confirmed with two
   *independent* first-generation pushes on different base arrays working
   fine, so it's specifically chained/nested push that's broken. This
   makes any accumulator-in-a-loop pattern (`acc = acc.push(x)` inside a
   `while`) unusable — `let acc = []; while i < 5 { acc = acc.push(i); ... }`
   fails on the second iteration.
3. **Nested array-literal indexing is broken.** `let snake = [[5,5],[5,6]];`
   compiles, but `snake[0]` (a single level of indexing into an
   array-of-arrays) fails at runtime with `type error: expected array, got
   int` even just doing `let head = snake[0]; len(head)` — before any
   second-level indexing is attempted.
4. **No slicing.** `xs[1:]` / `xs[1:3]` are parse errors; `.slice(1)`
   compiles but is misrouted to capability dispatch at runtime
   (`unknown capability: slice`), same failure mode as `.set()`.

## Impact

Any Crush program needing to build up or maintain array state across a
loop or a sequence of operations — which is close to a hard requirement for
most non-trivial programs — currently can't use arrays for it at all.
`examples/crush/arrays_and_loops.crush` and `examples/crush/snake.crush`'s
comment header document the workarounds found; see also `CRUSH-3` for two
existing example files that fail for related/adjacent reasons.

## Reproduction

```bash
# 1. Index assignment: parse error
echo 'let xs = [5, 5, 5]; xs[0] = 9;' | crushc /dev/stdin -o /dev/null
# [E-PP01] Unexpected token in expression: Assign

# 2. Chained push: runtime stack underflow
cat > /tmp/t.crush <<'EOF'
let a1 = [1];
let a2 = a1.push(2);
let a3 = a2.push(3);
EOF
crushc /tmp/t.crush -o /tmp/t.cvm1 && crush-run run /tmp/t.cvm1
# [runtime] stack underflow

# 3. Nested array indexing: runtime type error
cat > /tmp/t2.crush <<'EOF'
let snake = [[5, 5], [5, 6]];
let head = snake[0];
print(len(head));
EOF
crushc /tmp/t2.crush -o /tmp/t2.cvm1 && crush-run run /tmp/t2.cvm1
# [runtime] type error: expected array, got int
```

## Technical approach

Not investigated in depth (found via black-box language testing while
porting an example, not via source-diving crush-frontend/crush-vm). Likely
starting points for whoever picks this up:

- `crates/crush-frontend/src/parser/` — assignment-statement grammar,
  to add `Expression::Index` as a valid lvalue (or make the `[E-PP01]`
  error message name what *is* valid, since right now it just says
  "Unexpected token")
- `.push()`/`.append()`/`.set()`/`.slice()` dispatch — the fact that
  `.push()`/`.append()` compile and partially work while `.set()`/`.slice()`
  compile but hit capability dispatch suggests there's a hardcoded
  allowlist of recognized array methods somewhere in the compiler
  (`crush-frontend/src/compiler.rs`?) that's incomplete, plus a separate
  runtime bug in whatever `.push()` returns that breaks a second `.push()`
  chained onto it (`crush-vm/src/portable_vm.rs`'s array opcode handling,
  or the `ARR_PUSH` lowering in `crush-lang-sdk/src/compile.rs`)
- Nested-array indexing — likely in the `ARR_GET`/array-literal codegen
  path not correctly preserving element type tags for nested composite
  values

## Files to modify

- `crates/crush-frontend/src/parser/` (index-as-lvalue grammar)
- `crates/crush-frontend/src/compiler.rs` (method-call dispatch allowlist)
- `crates/crush-vm/src/portable_vm.rs` / `crates/crush-vm/src/scheduler.rs` (array opcode runtime behavior)

## Non-goals

- Full slice syntax with step (`xs[a:b:c]`) — just get single-level
  index-assign and non-chained-safe push/slice working first
