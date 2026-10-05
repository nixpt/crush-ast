# CRUSH-139 — JIT takes the wrong branch for `if inside && !outside`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-139 |
| **Priority** | P2 |
| **Status** | Done (2026-10-05, branch `claude/fastvm-args-execlang`) |
| **Phase** | M2 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

The JIT returns `20` where FastVM (and every other backend) returns `10`. Pre-existing on `main` `a8247af` (same result with the old eager `and`/`or` and with CRUSH-125's branch lowering); the simpler forms `if !outside` and `if !b` agree. The differential harness only warns on JIT divergence, so this never failed a test. Found while testing CRUSH-125.

## Reproduction

```crush
fn main() {
  let x = 4
  let inside = x > 0 && x < 5
  let outside = x < 0 || x > 9
  if inside && !outside { return 10 }
  return 20
}
```

FastVM → `10`; JIT (`jit_outcome_via_subprocess`) → `20`.

## Success criteria

- [x] JIT returns `10`
- [x] a differential test that fails on JIT divergence for this program

## Technical approach

- Bisect: `let y = !outside`, `inside && y`, bools loaded from locals vs. computed — find which JIT lowering (`Not` on a loaded bool, or a branch on a combined condition) is wrong.

## Files to modify

- `crates/crush-jit/src/compiler.rs`

## Resolution

Root cause: `JumpIfNot` and `Not` negated a 0/1 truth value with `bnot` (bitwise): 1 → 0xFE, 0 → 0xFF, both non-zero, so every unfolded `jmp_if_not` jumped and `!x` was always true. The original repro only *looked* like an `&&`/`!` problem; `let y = false; if y == false` failed too. `is_float` and the float-equality guard had the same pattern. New `lnot` helper (`icmp_imm eq 0`) used in all four places; `BitNot` keeps the integer `bnot`.

Test: `jit_branches_on_comparisons_and_negation` (strict on the JIT — the harness only warns on JIT divergence). While here, the harness's JIT warnings turned out to be mostly noise from a shared temp file between parallel tests (fixed); the two real remaining JIT gaps are CRUSH-142.
