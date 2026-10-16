# CRUSH-111 — type checker's builtin table is out of sync with the compiler's

| Field | Value |
|-------|-------|
| **ID** | CRUSH-111 |
| **Priority** | P2 |
| **Status** | **In review** — implemented on `agent/buffy/CRUSH-111` (2026-10-16); see Resolution |
| **Phase** | M1 |
| **Assignee** | buffy |
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

## Resolution (buffy, 2026-10-16 — branch `agent/buffy/CRUSH-111`)

`walker_intrinsics` is now the single table: `BUILTINS` (name + arity + result type) for every
non-intrinsic name `compile_call` lowers inline, consulted by `semantics.rs`; the compiler keeps its
arms and a new unit test (`compiler_and_table_agree`) parses `compiler.rs` for
`function == "..."` arms and fails when the two lists diverge. The checker's own
`len`/`print`/`parallel_map` registrations are gone — the table is the only list now.

`crush_frontend::compile_cast` accepts the programs it used to refuse, with identical VM output to
the walker door:

| program (Python walker) | before | after |
|---|---|---|
| `for i in range(3): print(i)` | `Undefined function: make_range` | `0 1 2` on both doors |
| `xs = [i for i in range(3)]; print(xs[1])` | `Undefined function: make_range` | `1` on both doors |

### Two further divergences found while making the doors agree

1. **`make_range` arity.** The checker and the capability runtime both require 1–3 arguments
   (`crush-vm::make_range` errors `CapArity` on an empty call) but `compile_call` accepted 0 with a
   stale "handles 0..2 args" comment. The arm now checks 1–3 at compile time, so the arity contract
   is the same on both doors and a runtime `CapArity` becomes a compile error.
2. **Generated receivers.** Comprehensions desugar to `__comp_N.append(x)`
   (`crush-lang-python/src/lower_expr.rs:632`), which `compile_call` lowers through its generic
   `obj.method` split (`load recv` + `cap_call`) — but the checker refused the dotted name with
   `Undefined function: __comp_0.append`. Calls whose receiver is walker-generated
   (`__comp_*`, `__crush_*` — `walker_intrinsics::is_generated_receiver`) are now accepted
   after the receiver is checked to be bound. Calls on *user* receivers keep the old behaviour
   (refused by the checker), which is CRUSH-112's question, not this ticket's.

### Deliberate behaviour change

The checker no longer registers `len`/`print`/`parallel_map` in its private function map, so a bare
*reference* to one of those names (passing `print` as a value) no longer resolves to a function type.
No test in the workspace depends on it; it is called out here because it is the one behaviour the
single-table change removes.

### Evidence (`agent/buffy/CRUSH-111`)

- `cargo test -p crush-frontend` — 111 + 48 + 24 + 18 + 19 + 13 + 11 + 9 + 6 + 3 + 2 + 1 passed, 0 failed
  (includes the new `builtin_names_agree_across_both_doors`, `unknown_names_stay_refused_by_the_checker`,
  `user_functions_still_shadow_builtin_names`, `compiler_and_table_agree`).
- `cargo test -p crush-lang-python` — 66 + 6 passed incl. the two new door tests; `-p crush-lang-js` and
  `-p crush-lang-sdk` green (the JS comprehension/`Math` suites exercise the same table).
- AOT parity harnesses `cargo test -p crush-aot --test python_parity --test differential_aot` —
  `51 passed` (differential) + `2 passed` (python parity), 0 failed, exit 0. The first attempt at the
  full `-p crush-aot` suite died with `No space left on device` while linking in `/build` — disk, not a
  test failure; re-run with `CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0` to fit.

### Still open after this ticket

- Dotted capability calls on user receivers (CRUSH-112).
- The walker door still accepts *unknown plain* names as late-bound `call` opcodes; the checked door
  refuses them. That asymmetry is by design and is now pinned by
  `unknown_names_stay_refused_by_the_checker` rather than being implicit.
