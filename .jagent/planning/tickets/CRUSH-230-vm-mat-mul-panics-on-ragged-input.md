# CRUSH-230 — VM `MAT_MUL` panics on ragged or mismatched matrices; no Crush syntax emits it

| Field | Value |
|-------|-------|
| **ID** | CRUSH-230 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | XS |
| **Filed by** | claude — PR #126 review follow-up, 2026-10-09 |

## Problem

`crates/crush-vm/src/scheduler.rs` `MAT_MUL` indexes `r.borrow()[k]` / `[j]` without
checking row lengths (it takes the column count from row 0), so a ragged or mismatched
input — e.g. `[[1,2,3]]` × `[[1],[]]` — panics the VM (index out of bounds). PR #126
made the C AOT backend reject these with `mat_mul: shape mismatch`. `mat_mul` is only
reachable from hand-written CASM: no Crush syntax or builtin emits it.

## Success criteria

- [ ] The VM returns a runtime error for mismatched shapes, same text as the C backend.
- [ ] Decide whether `mat_mul`/`vec_add`/`vec_dot` should be reachable from Crush
      (builtins) or removed from the backends.
