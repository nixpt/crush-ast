# CRUSH-215 — FastVM/JIT `==`/`!=` wrong for strings, bools and null (JIT tictactoe ends immediately)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-215 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |
| **Filed by** | claude — 2026-10-09 test-drive sweep, reproduced on `main` `554f077` (debug build) |

## Problem

`crates/crush-vm/src/fastvm/operations.rs:~66-90` (`compare_op`): strings fall through
to `_ => false` for both `Eq` and `Ne`; the Bool arm computes `x == y` even for `Ne`.

```crush
fn id(x) { return x }
print("ab" == "ab")            // FastVM: false
print(id(true) != id(false))   // FastVM: false
print(id(null) != id(1))       // FastVM: false
```

JIT: string `==` always false, `!=` always true — JIT tictactoe immediately prints
`game over: - wins!` (`w != "-"`); JIT snake output is wrong. CRUSH-136 covered string
*ordering*, not equality.

## Success criteria

- [ ] Equality for every value kind shared by interp, FastVM and JIT (one implementation).
- [ ] Differential test over all value-kind pairs for `==`/`!=`.
