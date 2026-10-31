# CRUSH-290 — JS `===` / `!==` lower to an operator the checker rejects

| Field | Value |
|-------|-------|
| **ID** | CRUSH-290 |
| **Priority** | P1 |
| **Status** | Done — PR #295 merged 2026-10-29 |
| **Phase** | COSM K3 follow-up (boa walker) |
| **Assignee** | freebuff (dispatched by muse) |
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

## Resolution

Implemented as suggested, mirroring `js.loose_eq` at every layer
(implemented by freebuff under muse dispatch; reviewed and verified
by muse):

- `js_core::strict_eq` in crush-vm (no coercion; cross-type
  primitives are `false`, never errors; `Null` only equals `Null`;
  containers by identity; `Other` stays a loud error as in
  `loose_eq`), registered as the `js.strict_eq` capability and
  dispatched in both VM schedulers.
- Both walkers lower `===` to `js.strict_eq` and `!==` to its
  negation, beside the `==`/`!=` arms.
- AOT: Rust codegen delegates to `js_core` as with `js.loose_eq`;
  the C backend emits `_js_strict_eq`, implemented beside
  `_js_loose_eq` in `js_runtime.c`.
- Follow-up found during the work (same root cause): both walkers'
  `switch` lowering synthesized `BinaryOp "==="` for case tests, so
  any JS `switch` also failed the checked path. Case tests now emit
  `js.strict_eq` too (JS case matching is strict equality).
  Consequence: a case test against a value with no JS meaning (a
  function value) is now a loud runtime error instead of crush's
  identity compare — the same tradeoff `===` makes.
- Tests: `js_ops` strict-equality vectors (verified against node),
  identity tests, checked-door (`compile_cast`) boa tests, an
  end-to-end boa test, AOT caps coverage, and the extended
  `node_corpus/ops_strict_eq.js` (was a single line) across types.

Int/int compares exactly (i64), mirroring `loose_eq`, rather than
as JS doubles — deliberate, for VM/AOT agreement.

## Survey 2026-10-31 (status)

PR #295 (`agent/muse/CRUSH-290`) merged 2026-10-29. Previous status: `Done (PR pending)`.
