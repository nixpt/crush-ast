# CRUSH-111 — type checker's builtin table is out of sync with the compiler's

| Field | Value |
|-------|-------|
| **ID** | CRUSH-111 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none (see CRUSH-65 for the shared-registry recommendation) |
| **Estimated effort** | M |

## Problem

`compiler.rs`'s `compile_call` handles ~25 builtin function names
(`str.contains`, `str.split`, `str.replace`, `str.join`, `make_range`,
`range`, `arr_get`, `arr_set`, `array.push`, `array.pop`, `math.pow`,
`math.sqrt`, …), but `semantics.rs` registers only `len` and `print` as
builtin functions — its `check()` inserts exactly those two names.

Because a non-dotted name like `arr_get(...)` or `make_range(...)` parses as
`Expression::Call` (not `CapabilityCall`), the **type checker** hits its
"Undefined function" bail before the compiler ever runs — so the compiler's
`compile_call` arms for those names are dead code from `.crush` source.

## Impact

The only array primitives callable from source are array literals, indexed
assignment (`xs[i] = v`), indexed read (`xs[i]`), and `str.split`. There is no
callable `make_range` / `append` / `push` / `arr_set` / `arr_get` *function*
form, so growing an array from source is effectively impossible — the root of
the integer-packing workarounds used across the `awesome-crush` games.

## Reproduction

```bash
printf 'fn main() { let r = make_range(1, 5); io.print(r); return 0; }\n' > /tmp/r.crush
crushc /tmp/r.crush -o /tmp/r.cvm1
# [type] type error: Undefined function: make_range

printf 'fn main() { let a = [1,2,3]; io.print(arr_get(a, 1)); return 0; }\n' > /tmp/g.crush
crushc /tmp/g.crush -o /tmp/g.cvm1
# [type] type error: Undefined function: arr_get
```

## Success criteria

- [ ] `make_range(a, b)`, `arr_get(a, i)`, `arr_set(a, i, v)` — and every other name `compile_call` already handles — type-check and run from `.crush` source.
- [ ] The builtin name table is defined in ONE place and consulted by both the type checker and the compiler (per CRUSH-65's "single shared builtin-name registry" recommendation).

## Technical approach

- Replace the two hardcoded `functions.insert("len"/"print")` calls in `semantics.rs::check()` with a shared builtin table (name → arg types → return type) that mirrors `compiler.rs`'s `compile_call` arms.
- Prefer the shared-registry direction from CRUSH-65 over a second hand-maintained list, so this class of drift can't recur.

## Files to modify

- `crates/crush-frontend/src/semantics.rs` — builtin registration.
- `crates/crush-frontend/src/compiler.rs` — source the same shared table.

## Non-goals

- Dotted `array.*`/`str.*`/`math.*` forms (those are CRUSH-112).
- Making `len()` accept strings (CRUSH-114).
