# CRUSH-xps-13 — Support Rust's mapped standard capabilities on both AOT backends

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-13 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M2 / Rust AOT parity |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-xps-3, CRUSH-xps-5 |

## Problem

Rust's mapped methods now use resolved named host capabilities to avoid local
bindings such as `str` or `math` changing call resolution. The VM matches rustc,
but both AOT backends refuse `str.to_lower`, `str.contains`, and `math.pow`.
Their older intrinsic spellings are not equivalent to accepting the named SDK
capability contract.

Found by the CRUSH-xps-5 audit on `0140399`. The existing fixtures
`capability_namespace_bindings`, `int_arith`, and `vecmut` reject with
`the <Rust|C> AOT backend cannot compile: cap_call '…'`.

## Success criteria

- [ ] Both AOT backends implement these named capabilities with the same
      argument, result, error, and text/number semantics as the SDK contract.
- [ ] Audit the other resolved method names in the Rust walker and either
      implement them identically or retain explicit compile-time refusal.
- [ ] Tests include Unicode case conversion and string search, numeric power
      typing/edge cases, and local bindings named `str`/`math`.
- [ ] Reuse shared implementations/contracts; do not add independent semantics
      solely to satisfy ASCII fixtures.
- [ ] Remove the three AOT-only exceptions from the shared Rust conformance
      policy after VM, Rust AOT, and C AOT outputs match real rustc.
- [ ] Run C cases under `CRUSH_GC_STRESS=1` as well as normal collection.

## Non-goals

Rust ownership, integer widths, object methods, and arbitrary Rust crate APIs.
