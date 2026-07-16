# CRUSH-13 — Five independent `add`/arithmetic implementations disagree (bugarium flagship target)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-13 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | loosely related to CRUSH-10/CRUSH-11 (same backends), but this is about semantic divergence, not compile/output bugs |
| **Estimated effort** | L |

## Problem

Five separate places implement arithmetic (`+`, `/`, `%`, etc.) and they
give **different answers for the same program**:

1. `crush-vm/scheduler.rs` — the interpreter `crush-run` actually uses
2. `crush-vm/portable_vm.rs` — `PortableVm`
3. `crush-vm/fastvm/execution.rs` — `FastOp::Add` via `binary_op`, numeric-only; `crush-python` calls `run_fastvm`
4. `crush-aot/codegen.rs` — emits Rust via `bin_arith`, numeric-only
5. `crush-aot/codegen_c.rs` — emits C via `_add()`

A prior session (s385) fixed string-concat + a loud `TypeError` in (1) and
(2) only — which **widened** the gap, since (3)/(4)/(5) didn't get the same
fix. **Proven divergence, independent of that fix**: `1 / 0` raises
`DivisionByZero` loudly in the scheduler interpreter, but
`crush-aot/codegen.rs` emits `if b != 0 then a/b else 0` and **silently
returns 0**. Same program, different answers, no error from either path.
Same class of bug very likely applies to `%` (mod-by-zero) and possibly
other operators — not fully audited.

## Impact

This is the exact scenario the `crush-diff` differential harness
(`crates/crush-lang-sdk/src/differential.rs`, already built and already
caught one real divergence on its first run — see `dejavue`) exists to
catch systematically, but `crush-diff` currently only compares
interpreter/portable/fastvm (backends 1-3) — the AOT backends (4/5) "do not
link" was the old blocker (now resolved, see CRUSH-16) and were never
wired into the comparison. `crush-aot` is not in the runtime dependency
graph (zero reverse deps — no source file imports it), but it IS a
workspace member that builds and ships two real binaries (`crush-aotc`,
`crush-walk-run`), so a user genuinely can invoke it and get silently
different semantics than every other execution path.

## Reproduction

```bash
cat > /tmp/divzero.crush <<'EOF'
fn main() { print(1 / 0); }
EOF
crushc /tmp/divzero.crush -o /tmp/divzero.cvm1
crush-run run /tmp/divzero.cvm1
# [runtime] DivisionByZero  (loud, correct)

crush-aotc compile /tmp/divzero.crush --emit so
# .so compiles; calling crush_run() silently returns 0, no error
```

## Technical approach

1. Audit all 5 arithmetic implementations for `+`, `-`, `*`, `/`, `%`
   specifically for zero-division and type-mismatch behavior; catalog
   every divergence found (not just the one already proven).
2. Decide the canonical semantics once (loud error on div/mod-by-zero,
   matching the scheduler/portable_vm precedent already set for
   string-concat type errors) and propagate to all 5 backends.
3. Wire the AOT backends (crush-aot) into `crush-diff` as backends D/E now
   that they link (CRUSH-16) — this was explicitly deferred pending the
   link fix and should no longer be blocked.
4. Add corpus test cases specifically for div/mod-by-zero and any other
   divergence found, so `crush-diff` catches regressions here going
   forward — this whole bug class is exactly what the harness exists for.

## Files to modify

- `crates/crush-vm/src/scheduler.rs`, `portable_vm.rs`, `fastvm/execution.rs`
- `crates/crush-aot/src/codegen.rs`, `codegen_c.rs`
- `crates/crush-lang-sdk/src/differential.rs` (wire in AOT backends)

## Non-goals

- Fixing CRUSH-10/CRUSH-11 themselves (those are separate, already-tracked
  compile/output bugs in the same two files this ticket also touches)
