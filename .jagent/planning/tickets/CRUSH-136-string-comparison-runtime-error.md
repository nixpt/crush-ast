# CRUSH-136 — `<` / `>` on strings type-check but fail at run time

| Field | Value |
|-------|-------|
| **ID** | CRUSH-136 |
| **Priority** | P2 |
| **Status** | Done (2026-10-05, branch `claude/crush-136-string-compare`) |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **GitHub** | [#76](https://github.com/nixpt/crush-ast/issues/76) (filed 2026-10-04 by pranix) |

## Problem

`"a" < "b"` passes the checker and fails in `compare_values` (`portable_vm.rs` ~l.563) with "expected numeric, got str".

## Reproduction

From [#76](https://github.com/nixpt/crush-ast/issues/76) (verbatim):

#### Summary

`"a" < "b"` compiles cleanly but fails at runtime with `[runtime] type error: expected numeric, got str`. `==` and `!=` work fine on strings.

#### Repro

```crush
fn main() {
  if "a" < "b" { print("lt") }
}
```

#### Expected

Either support lexicographic comparison for `<`/`>`/`<=`/`>=` on strings, or reject it at compile time. A runtime type error for code that type-checks is the worst option.

## Success criteria

- [x] either lexicographic comparison on all backends, or a compile-time error — no run-time type error for code that type-checks

## Technical approach

- Extend `compare_values` to `Str × Str` (byte-wise/lexicographic) and match on FastVM/JIT/AOT.

## Files to modify

- `crates/crush-vm/src/arithmetic.rs`
- backend equivalents

## Decision (owner interview, 2026-10-05)

Lexicographic by Unicode code point (= UTF-8 byte order) on every backend; string vs number stays a type error.

## Resolution

Reproduced on the branch base first. Each backend gained a two-string branch:
- **CVM1:** `crush_vm::arithmetic::compare_values`, shared by the scheduler and PortableVm.
- **FastVM:** a new `compare_ordered`, which reads arena-held strings.
- **JIT:** the `OP_CMP_ORDERED` helper.
- **AOT Rust:** `bin_cmp_ordered`.
- **AOT C:** `_cmp` plus the type guard in front of it.

The two existing "string ordering is rejected" tests now pin the new behaviour. New tests (each fails without the fix):
- `crush-lang-sdk/tests/gh_issue_76_string_compare.rs` (the issue's repro now prints `lt`);
- `aot_ordered_comparison_of_strings_is_lexicographic`, which checks every backend strictly, including the JIT;
- string-vs-number rejection on every backend.

Writing the all-backend test exposed CRUSH-143, a pre-existing optimizer bug, fixed in the same PR.
