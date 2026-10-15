# CRUSH-zorro-2 — Python parity follow-ups: tuples, `in`/`is`, whitespace split, C string ops

| Field | Value |
|-------|-------|
| **ID** | CRUSH-zorro-2 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M6 (frontends) |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-251 (closures) for the deferred item |
| **Estimated effort** | M |
| **Filed by** | claude (vega) — 2026-10-15, Python e2e parity push |

## Problem

Python e2e parity (branch `agent/vega/python-parity`) covers a 19-program CPython corpus
(`crates/crush-lang-python/tests/cpython_corpus`): the VM matches CPython on all of it, AOT Rust is
exact, AOT C is exact or loud. These Python features are outside that corpus and still differ:

- **Tuples print as lists.** The walker lowers tuples to arrays (`print((1, 2))` → `[1, 2]`). The VM
  has `Value::Tuple`/`NEW_TUPLE`, but neither AOT backend implements `new_tuple`, and Crush source has
  no tuple syntax. Emitting real tuples needs AOT support first, or AOT would go from working to failing.
- **`x in seq`, `x is None`.** `__crush_contains__` / `__crush_is__` have no implementation; since #180
  they fail with `unknown capability` instead of printing `None`. (`in` is very common in Python.)
- **`s.split()` without a separator** (whitespace-run semantics) fails by name.
- **AOT C string ops:** `str_split` / `str_replace` / `str_join` aren't in the C runtime (compile error;
  `LOUD_ON_C` in `crates/crush-aot/tests/python_parity.rs`).
- **Closures / lambdas:** deferred to CRUSH-251 (crush can't call a function value yet). The owner noted
  higher-order functions are still needed, so CRUSH-251 is the next language-level item.

## Success criteria

- [ ] Each item above either matches CPython on every engine or is a documented, loud non-goal.
- [ ] New corpus programs for each (tuples, `in`/`is`, `split()`, C string ops) added to `cpython_corpus`;
      `LOUD_ON_C` shrinks accordingly.

## Technical approach

- `seq.contains` / identity-`is` as shared capabilities, like `seq.slice` and `math.power`: one VM
  implementation, mirrored in the AOT Rust and C runtimes.
- Tuples: implement `new_tuple` and tuple index/len/iterate/print on both AOT backends, then lower
  Python tuples to `TupleLiteral`.
- C runtime: `str_split` / `str_replace` / `str_join` over the existing string arena and array pool.

## Non-goals

- An embedded Python VM (CRUSH-47, declined 2026-10-15): parser-only frontends, crush-vm executes.
