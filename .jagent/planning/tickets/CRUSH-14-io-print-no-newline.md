# CRUSH-14 — `io.print` does not emit a trailing newline

| Field | Value |
|-------|-------|
| **ID** | CRUSH-14 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

`io.print`'s output has no trailing newline, so successive prints run
together on one line: `crush-website/example.crush`'s output currently
reads `🚀 Crush...Python: 5^3 = 125.0Back in Crush...` instead of one line
per print. The archived pre-extraction stdlib had `io.print` AND `io.echo`
as distinct capabilities, suggesting one was meant to be line-oriented and
one raw.

## Impact

Cosmetic but visible in the first thing anyone runs — the website demo,
and by extension any multi-line example — looks broken even when the
underlying logic is completely correct.

## Reproduction

```bash
cat > /tmp/twolines.crush <<'EOF'
fn main() {
    print("line one");
    print("line two");
}
EOF
crushc /tmp/twolines.crush -o /tmp/twolines.cvm1 && crush-run run /tmp/twolines.cvm1
# expected: "line one\nline two\n"
# actual:   "line onelinetwo" (no newline between them)
```

## Technical approach

Either (a) add a trailing `\n` to `io.print`'s output unconditionally, or
(b) restore the `io.echo`/`io.print` split (one line-oriented, one raw) —
check which the guide and existing examples actually expect before
picking. Whichever direction, verify against all 3 execution backends
(scheduler, portable_vm, fastvm) plus both AOT backends' `cap_call
io.print` codegen for consistency (see CRUSH-13 — arithmetic isn't the
only place these backends can silently diverge on capability semantics).

## Files to modify

- `crates/crush-vm/src/scheduler.rs` / `portable_vm.rs` (io.print dispatch)
- `crates/crush-aot/src/codegen.rs` / `codegen_c.rs` (`cap_call io.print`
  codegen — must match whatever the interpreter path decides)

## Non-goals

- Restoring the full archived stdlib's io namespace (that's the larger
  "STDLIB RESTORATION MAP" opportunity in TASKS.md, separate effort)
