# CRUSH-290 — JS `===` / `!==` lower to an operator the checker rejects

| Field | Value |
|-------|-------|
| **ID** | CRUSH-290 |
| **Priority** | P1 |
| **Status** | Open |
| **Phase** | COSM K3 follow-up (boa walker) |
| **Assignee** | unassigned |
| **Filed by** | muse — 2026-10-21, found while landing CRUSH-289 |

## Problem

Both JS walkers lower `a === b` to a CAST `BinaryOp "==="`
(boa: `lower/expr.rs` RelationalOp::StrictEqual; swc:
`lower_swc.rs`), and `crush-frontend`'s semantics checker
rejects it: `Unknown operator: ===`. So any JS using strict
equality — maya uses it pervasively — *walks* (the sweep only
lowers) but cannot compile or run through the checked path:

```js
console.log(1 === 1);        // compile: Unknown operator: ===
function f(x) { return x === null; }
```

## Suggested shape

JS strict equality is its own operation (no coercion; the one
number type means `1 === 1.0` is true): a `js.strict_eq`
capability beside `js.loose_eq` in `crush-vm`'s `js_core`,
mirrored in the AOT runtimes (Rust codegen delegates to
`js_core`; the C runtime needs its own arm), with `===`
lowering to it and `!==` to its negation, in both walkers.
Corpus: add a node_corpus program exercising `===` across
types (numbers, strings, bools, null, objects by identity).

## Evidence

Reproduced on the CRUSH-289 branch (probe, 2026-10-21):
`1 === 1` and `x === null` in a function both fail
`compile_cast` with `Unknown operator: ===`; `1 == 1` runs.
