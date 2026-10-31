# CRUSH-257 — AOT C: sound local type inference, then re-enable unboxed int64/double locals

| Field | Value |
|-------|-------|
| **ID** | CRUSH-257 |
| **Priority** | P1 — the largest single lever on AOT compute speed |
| **Status** | In Progress — sound inference + typed locals landed in PR #165 (2026-10-12); the 1.5x-of-Node benchmark criterion is not met (no speed change; needs CRUSH-258) |
| **Phase** | M2 |
| **Assignee** | unassigned |
| **Dependencies** | none (CRUSH-260 for JS number semantics) |
| **Estimated effort** | M |
| **Filed by** | claude (vega) — 2026-10-12 JS-vs-Node benchmark (CRUSH-261) |

## Problem

The C backend can keep a local as a native C variable — `LocalType::{I64, F64}` (`crates/crush-aot/src/codegen_c.rs`
~827-856, load/store at ~1083/1095) — but every local is forced to `Value` since `f65777c` (2026-07-14):

```rust
// TODO: type inference currently produces wrong results for array/varargs ops.
// Force Value type until inference handles all opcodes correctly.
// "Float" | "F64" | "F32" => LocalType::F64,
// "Int" => LocalType::I64,
_ => LocalType::Value,
```

So even `s = (s + i*i) % 1000003` with `s`, `i` provably Int runs as boxed `Value`s through `_add/_mul/_mod` type
dispatch. Measured (CRUSH-261, Node v26.11.1): the 20M-iteration loop takes 747 ms on AOT-clang vs 321 ms on Node.

## Success criteria

- [ ] `infer_types` (~886) is sound: a forward dataflow over the CFG (jumps, not one linear pass), joining at
      merge points; any opcode it doesn't model (arrays, varargs, calls, cap_calls, polyglot) yields `Any` for
      what it produces, and a local is I64/F64 only if **every** store to it is that type.
- [ ] `Int`/`Float` re-enabled; overflow semantics preserved (checked i64 ops → the same runtime error the VM gives).
- [ ] `examples_parity` (incl. `CRUSH_GC_STRESS=1`) and the differential harness stay green; a test pins that a
      local assigned an array somewhere stays `Value`.
- [ ] CRUSH-261 benchmark: loop/fib/sieve on AOT-C within 1.5x of Node or better (report before/after).

## Survey 2026-10-31 (status)

PR #165 implements criteria 1–3. Its own table reports no speed change (fib 63→62 ms, loop 990→978 ms), so the benchmark criterion is not met. Previous status: `Backlog`.
