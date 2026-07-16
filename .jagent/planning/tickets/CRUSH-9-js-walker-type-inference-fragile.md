# CRUSH-9 — JS-walked CAST hits severe, non-local type-inference bugs

| Field | Value |
|-------|-------|
| **ID** | CRUSH-9 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none (adjacent to CRUSH-7/CRUSH-8, distinct root cause) |
| **Estimated effort** | L |

## Problem

Found while porting `examples/js-walked/turtle_runner.js` — a real, working
Snake-adjacent self-playing game walked through `js_walker`
(`crush-lang-js`, swc backend) into CAST, then compiled via
`crush_frontend::compile_cast`. Getting a genuinely non-trivial multi-
function JS program through the compiler required extensive black-box
bisection because `compile_cast`'s type-checker has bugs whose triggers are
**non-local**: adding, removing, or merely *defining but never calling* an
unrelated function elsewhere in the same file flips whether an entirely
different function type-checks.

## Confirmed reproduction cases

All confirmed via `js_walker <file> | ` piped into a small helper that
calls `crush_frontend::compile_cast` directly (bypassing the interpreter,
so these are pure type-checker issues, not VM/runtime bugs).

**1. A function's own return-type unification breaks when one branch
returns a literal and another returns a parameter-derived/computed value:**

```js
function f(x) {
    if (x < 0) {
        return 23;      // literal
    }
    return x;            // parameter (or `x + 0`, or a local copy of x —
}                         // all three still trigger it)
```

Fails with `Conflicting return types: int and any` — **even when `f` is
never called anywhere in the program.** Renaming the literal branch to also
route through a local/expression (`const r = 23; return r;`) does not fix
it. The only reliable fix found: make the function have exactly one
unconditional `return <expr>;` (no `if`), reformulating branching logic as
arithmetic (e.g. a wraparound `((x - n) % m + m) % m` instead of an
`if (x < 0) return m; return x;` clamp). Multi-branch functions where
*every* branch returns a literal (no parameter/computed values at all) are
unaffected — see `game_over_flag`/`hits_obstacle_flag`-shaped "flag"
functions in the example, confirmed safe across ~15 variations.

**2. A boolean value returned by one function and passed as an argument to
a second function, then used directly in an `if` there, isn't recognized as
bool:**

```js
function danger(n) { if (n == 3) { return true; } return false; }
function g(flag) { if (flag) { return 1; } return 0; }   // flag: bool param
g(danger(3));
```

Fails with `If condition must be bool, found any` — the parameter's type
isn't propagated from the call site. Calling the bool-returning function
*directly* in the condition (`if (danger(n))`) works fine; only the
pass-through-a-parameter form breaks. Workaround used in the example:
represent booleans as `0`/`1` integers everywhere, compare with `== 1`
instead of using them as bare conditions.

**3. A completely unrelated, entirely uncalled function's mere presence
(or absence) in the file flips whether case 1 triggers**, and in one
observed instance **removing a single no-op `console.log("");` statement**
(present in the original file, textually identical before/after) flipped a
previously-working file back into `Conflicting return types: int and any`.
This is the most concerning part: the type-checker appears to carry some
form of shared/global state across function boundaries that a no-op
statement can perturb.

## Reproduction

```bash
# Case 1 — minimal, standalone, never-called function breaks itself:
cat > /tmp/t.js <<'JS'
function f(x) {
    if (x < 0) { return 23; }
    return x;
}
console.log(1);
JS
js_walker /tmp/t.js > /tmp/t.cast.json
# then compile_cast(&program) on the resulting CAST -> "Conflicting return types: int and any"

# Case 2:
cat > /tmp/t2.js <<'JS'
function danger(n) { if (n == 3) { return true; } return false; }
function g(flag) { if (flag) { return 1; } return 0; }
console.log(g(danger(3)));
JS
js_walker /tmp/t2.js  # then compile_cast -> "If condition must be bool, found any"
```

## Impact

Any non-trivial JS program walked through `crush-lang-js` is at real risk
of hitting one of these — not exotic edge cases, but ordinary patterns
(a clamp/guard-clause function, passing a boolean between two helper
functions) that show up constantly in real code. `examples/js-walked/
turtle_runner.js`'s header comment documents the specific safe subset found
by trial and error; that subset is small enough that most real JS won't fit
it without rewriting.

## Technical approach

Not investigated — found via black-box testing against the compiled
binary, not via source-diving `crush-frontend`. Whoever picks this up
should start in `crates/crush-frontend/src/semantics.rs` (return-type
unification, `check_stmt`/function-body type inference) — the "even when
never called" and "an unrelated no-op statement flips it" symptoms suggest
whatever tracks inferred types per-function is not properly scoped/reset
between functions, or a single shared type-inference table/counter is
being mutated in an order-dependent way across the whole compilation unit.

## Files to modify

- `crates/crush-frontend/src/semantics.rs` (type inference / return-type
  unification — primary suspect)
- Possibly `crates/crush-lang-js/src/lower_swc.rs` (if the JS walker is
  emitting CAST in a shape that happens to trigger this more than native
  Crush source does — not confirmed either way; CRUSH-8's native
  `fibonacci.crush` failure with typed params suggests the underlying bug
  is not walker-specific, but the walker's lowering may make it easier to
  hit by construction)

## Non-goals

- Fixing CRUSH-7 or CRUSH-8 (array mutation, stale examples) — separate,
  already-filed, unrelated root causes
