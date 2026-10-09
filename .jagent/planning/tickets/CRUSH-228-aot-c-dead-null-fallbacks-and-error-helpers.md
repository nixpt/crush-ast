# CRUSH-228 — AOT C: dead `mk_null()` fallbacks after allocation; one OOM helper; error helper naming

| Field | Value |
|-------|-------|
| **ID** | CRUSH-228 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-223 |
| **Estimated effort** | XS |
| **Filed by** | claude — PR #126 review follow-up, 2026-10-09 |

## Problem

Since CRUSH-223, `_alloc_array`/`_alloc_object` exit on failure and never return -1,
but callers still branch on `>= 0` / `< 0` with `mk_null()` fallbacks (`mk_ai_stub`,
`mk_dom_stub`, `mat_mul`, `vec_add`, both `make_range` copies, `new_array`,
`new_object`). They suggest a null result is still possible — the contract CRUSH-223
removed. The out-of-memory exit is written out in `_str_new`, `_stack_grow` and
`crush_run`, and `_crush_arith_error` is used for non-arithmetic errors (pool
exhaustion, `mat_mul` shapes, too many fields). Review:
https://github.com/nixpt/crush-ast/pull/126#discussion_r4226980148

## Success criteria

- [ ] No `_alloc_*` result is checked against -1; no unreachable `mk_null()` branch.
- [ ] One `_crush_oom()`; a general `_crush_runtime_error(msg)` for non-arithmetic errors.
