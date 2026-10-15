# CRUSH-xps-8 — Preserve Rust integer widths, casts and overflow behavior

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-8 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M2 / Rust parity |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-xps-3, CRUSH-xps-5, CRUSH-xps-6, CRUSH-xps-7 |
| **Estimated effort** | L; deliver in 8a/8b/8c slices |
| **Spec** | [Rust parity completion](../designs/rust-parity-xps-8-13.md#xps-8-numeric-facts-and-the-bounded-integer-profile) |

## Problem and current evidence

`Expr::Cast` currently lowers only its operand, literal suffixes disappear,
and xps-7's ownership `Kind::Copy` collapses numeric widths. Generic Crush
integers are checked signed i64 values; that is not Rust's narrow-integer
or release-overflow contract.

The shared corpus pins two silent failures:

| Fixture | rustc stdout | Current pinned Crush stdout |
|---|---|---|
| `audit_integer_cast`: `300i64 as u8` | `44` | `300` |
| `audit_integer_overflow`: `fn bump(n: u8) -> u8 { n + 10 }; bump(250)` | `4` | `260` |

A larger signed cast such as `3_000_000_000i64 as i32` must become
`-1294967296`. Normalizing a result after its intermediate has already
overflowed i64 is not sufficient.

## Contract and scope

- Source profile: Rust 2021, `rustc -O`, runtime overflow checks disabled for
  wrapping fixtures. Record rustc version, target and options in proof. Debug
  overflow panics and const-evaluation failures are different cases.
- Preserve signedness/width through suffixes, annotations, casts, locals,
  assignments, parameters/returns, closure parameters, and branch joins.
  Unknown facts must not imply i64. Unsuffixed fallback is i32 when Rust's
  supported inference constraints require it.
- Initial representable integer domain: i8/i16/i32/i64 and u8/u16/u32.
  Store canonical mathematical values in existing `Int`, retaining their
  Rust source type separately. Do not use negative `Int` as a hidden u64.
- Integer conversions truncate and interpret the target sign bit correctly;
  widening preserves source signedness. Evaluate each operand once.
- Narrow arithmetic/compound assignment/wrapping operations normalize at
  each typed operation only where the intermediate is proven safe. Refuse
  unsupported operations, including unrestricted u32 multiplication or i64
  overflow, until a reviewed shared implementation provides exact behavior.
- Explicit pointer-width arithmetic/casts require a declared matching source
  target or refusal. Bounded `len`/index facts can be preserved when their
  use is independent of pointer width.
- Unsupported large widths, aliases/generics, float/char/pointer casts and
  f32 arithmetic fail at the Rust boundary; casts must not be erased.
- Numeric methods must produce a compatible runtime result as well as retain
  source facts: reject integer `.pow` mapped to a Float-returning capability
  until a compatible width-aware operation is selected. xps-13 owns its AOT
  capability expansion, not permission to leave the VM result mistyped.
- Keep generic Crush and JavaScript arithmetic policies independent of Rust.

## Implementation slices

### 8a — Preserve facts and close acceptance holes

Introduce a numeric fact pass beside binding/value metadata. Keep annotations,
suffixes and casts before generic CAST loses them; consume unsupported numeric
forms with line:column diagnostics. Reuse xps-6 binding identities, and do not
weaken xps-7 ownership checks. Attach facts to returned/mapped method values
so xps-13 can distinguish integer `.pow`, float `.powi`, and float `.powf`.

### 8b — Exact fixed-width casts

Use the bounded normalization algorithm in the spec. A compiler-owned cast
intrinsic with literal width/signedness operands is an acceptable implementation
if semantic analysis validates it, optimization preserves it, and codegen
consumes it with a single evaluation of its value argument. Prefer existing
bit/comparison/control-flow operations where their contracts suffice. A new
shared numeric kernel requires an explicit backend support/refusal table.

### 8c — Typed operations and wrapping

Implement only operations with exact intermediate/width semantics. Cover unary
negation, signed minimum parsing, bitwise/shift distinctions and compound
assignment as well as binary arithmetic. Preserve assignment's unit result.
Keep unimplemented `wrapping_*`, shifts, division edge cases and width-sensitive
methods explicitly loud. A checked-overflow VM error is not proof of matching
Rust's unchecked release wrapping.

## Proposed fixture matrix

All runnable cases must compile with rustc; runtime arguments avoid accidental
constant-overflow diagnostics. These are proposed cases, not new passing claims.

| Case | Required result or boundary |
|---|---|
| Existing two audit fixtures | VM exact or source-located rejection; AOT exact or compile-time rejection; remove both silent policies |
| `300i64 as u8`; `-1i16 as u8`; `255u8 as i8` | `44`; `255`; `-1` when supported |
| `3_000_000_000i64 as i32`; widening negative i8 | `-1294967296`; preserve the negative value |
| Signed min/max and unsigned max for each supported width | Correct values, including parsing the signed minimum |
| Parameter/return/local suffix inference and shadowing | Width follows binding identity, including closure parameters |
| Narrow add/sub/mul and compound operations | Exact wrap at each supported step, not just on final assignment |
| `wrapping_add/sub/mul`, large u32 multiply, i64 overflow | Exact implementation or explicit frontend refusal |
| Division/remainder zero and `MIN / -1`; large shifts | Dedicated supported/error contract or source refusal, never a host panic/UB |
| u64/i128/u128, usize/isize target dependence, erased noninteger casts | Representability/target proof or source refusal |
| Calls used as cast operands | Exactly one source evaluation |
| Integer power used in later division | Integer result or source refusal; metadata must not relabel a float |

## Acceptance

- [ ] Numeric facts survive every supported source/optimization boundary;
      unknown facts cannot authorize a width-dependent operation.
- [ ] Supported casts and operations match rustc, including return type and
      downstream arithmetic; unsupported cases report source line:column.
- [ ] Existing exact fixtures remain exact, or any newly restricted boundary
      is justified, documented and given a narrow ticket-linked policy.
- [ ] No accepted narrow operation silently widens and no accepted conversion
      disappears. Every supported cast evaluates its operand once.
- [ ] Both xps-8 silent entries are removed only after SDK VM and both AOT
      gates pass; expected refusal has the correct phase and diagnostic.
- [ ] Live SDK/rustc proof and supported native AOT artifacts are recorded,
      along with compiler profile, docs and release-note updates.

## Dispatch and validation

After the SDK stack is integrated, refresh main and inspect
`crush-lang-rust/src/{lower_expr,lower_stmt,bindings,values}`, the compiler
intrinsic contract, generic arithmetic and the shared corpus policy. Reproduce
the two audit cases first in an isolated xps-8 worktree with a dedicated target.
Deliver one numeric slice per reviewable PR; keep this ticket open until its
acceptance criteria are met.

Planned checks: SDK/Rust library and integration suite, frontend helper tests,
SDK rustc freshness audit, and Rust/C AOT parity. Run real source through the
public SDK with explicit stdlib registration and compare with a rustc binary.

## Non-goals

A Rust type checker, arbitrary precision, a new generic integer ABI, debug
panic equivalence, Rust object/COSM production, or CLI/Cargo dispatch.
