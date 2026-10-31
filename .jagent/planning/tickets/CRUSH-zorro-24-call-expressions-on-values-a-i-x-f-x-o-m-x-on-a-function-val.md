# CRUSH-zorro-24 — Call expressions on values: a[i](x), f()(x), o.m(x) on a function value

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-24 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M6 (frontends) |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-251 step 2 (#194, function values) |
| **Estimated effort** | S |
| **Filed by** | claude (vega) — 2026-10-15, from the foreman's review of #194 |

## Problem

Since CRUSH-251 step 2 (#194) function values exist (`MAKE_CLOSURE` / `CALL_VALUE`), but Crush
source can only call a **name**: CAST `Expression::Call { function: String, .. }`. Calling any other
expression is a parse error ("Cannot call non-function"):

```crush
let fs = [inc, sq]
fs[1](4)        // parse error — today: `let g = fs[1]; g(4)`
make_adder()(3) // parse error
obj.handler(x)  // parses as a method call / capability, not a call of the field's function value
```

## Fix

- CAST: a call whose callee is an expression (e.g. `Expression::CallValue { callee: Box<Expression>,
  args }`), or `Call` with an optional callee expression.
- Parser: postfix call after any primary/postfix expression (`a[i](x)`, `f()(x)`, `(expr)(x)`).
- Compiler: args last-first, then the callee expression, then `call_value argc` — the same shape #194
  emits for a variable.
- `o.m(x)`: decide the rule (a map field holding a function → call it; otherwise today's method /
  capability path) and keep it unambiguous.
- Walkers: JS `arr[i](x)` / `obj.fn(x)` and Python `fs[i](x)` lower to the new form.

## Done when

`fs[1](4)`, `make_adder()(3)` and the chosen `o.m(x)` rule run on scheduler + portable VM (a
`run_both`-style test), and AOT/FastVM still refuse loudly.
