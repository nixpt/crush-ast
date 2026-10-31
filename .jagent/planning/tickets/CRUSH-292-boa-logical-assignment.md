# CRUSH-292 — boa walker: logical assignment (`&&=`, `||=`, `??=`) refuses

| Field | Value |
|-------|-------|
| **ID** | CRUSH-292 |
| **Priority** | P2 |
| **Status** | Done — PR #297 merged 2026-10-29 |
| **Phase** | COSM K3 follow-up (boa walker) |
| **Assignee** | freebuff (dispatched by muse) |
| **Filed by** | muse — 2026-10-22, from the boa refusal inventory |

## Problem

The boa walker refuses logical assignment at walk time
(`lower/expr.rs`, `AssignOp::BoolAnd | BoolOr | Coalesce`):
`x ||= y`, `x &&= y`, `x ??= y` are common JS (config/default
plumbing; maya uses them) and currently cannot lower at all.

## Suggested shape

Desugar in the assignment lowering, reusing the machinery already
there (target resolution, the computed-target side-effect guard,
`__crush_assign__` / `__crush_setindex__`):

- `x &&= y` → `x` kept when falsy, else assign `y`
- `x ||= y` → `x` kept when truthy, else assign `y`
- `x ??= y` → `x` kept unless null, else assign `y`

with the condition semantics exactly those of the binary
`&&` / `||` / `??` lowering in the same file (`js.truthy`,
`js.loose_eq` against null), and the expression's value the kept
or assigned value, as in JS. The existing instance-tracking rule
already treats non-plain assignment as non-proving.

## Evidence

`loud()` on `let x = 0; x ||= 5;` → "logical assignment
(`&&=`, `||=`, `??=`) is not supported by the JS walker yet".

## Resolution

Implemented in the boa assignment lowering (freebuff under muse
dispatch; reviewed and verified by muse): the logical arm builds
the keep condition from the binary operator's own semantics
(`js.truthy`, negated for `&&=`; `!js.loose_eq(x, null)` for `??=`),
and the assign branch writes through the same
`__crush_assign__`/`__crush_setindex__` call as plain assignment,
then re-reads the target (`__crush_assign__` leaves null, so the
write rides the inner ternary's condition). The rhs runs only on
the assign branch; the expression's value is the kept or assigned
value. The computed-target side-effect guard still refuses
`a[f()] ||= 1` (the slot would be read twice). An existing
instance-tracking test moved `c ||= new B()` from loud to
runs-and-forgets, as intended.

Tests: end-to-end boa tests (all three operators, member/index
targets, value-as-expression, rhs laziness), the loud
computed-target test, and `node_corpus/ops_logical_assign.js`
(.out from node), allow-listed as swc-loud/boa-fixed in the corpus
and AOT parity harnesses (swc is frozen and still refuses).

## Survey 2026-10-31 (status)

PR #297 (`agent/muse/CRUSH-292`) merged 2026-10-29. Previous status: `Done (PR pending)`.
