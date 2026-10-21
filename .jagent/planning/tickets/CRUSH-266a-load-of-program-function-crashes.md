# CRUSH-266a — `Load` of a program-level function name crashes at runtime

> **Renumbered** by kai (2026-10-21) from `CRUSH-275` to `CRUSH-266a`, a residue suffix of the ticket it
> came in with (PR #283, CRUSH-266), so no new ticket can claim the number again. CRUSH-275 was claimed three
> times (PR #281's 1.0 criteria, PR #284's wheel CI ticket, and this one), and on the way here this ticket
> briefly held 281, 283 and 284, each taken by a new ticket within hours.

| Field | Value |
|-------|-------|
| **ID** | CRUSH-266a |
| **Priority** | P1 |
| **Status** | Done (superseded upstream — no code change needed) |
| **Phase** | M4 |
| **Assignee** | muse |
| **Dependencies** | CRUSH-266 (JS walker keeps exported declarations; this makes their `Export` statements runnable) |
| **Estimated effort** | S |
| **Filed by** | muse — 2026-10-20, found while fixing CRUSH-266 |

## Problem

A CAST `Var` expression naming a `Program.functions` entry compiles to a
slot `Load`, but no slot is ever stored for a function name, so the VM dies
with `load from uninitialised slot 0`. This is reachable today without any
exported declaration:

```js
function double(x) { return x * 2; }
export { double };
console.log(double(21));
```

walks, compiles to CASM/CVM1, then crashes at the `Export { value: Var("double") }`
statement. Function *values* elsewhere in this toolchain are name strings
(the lambda compiler emits `push_str <lambda_name>` and stores that), so a
`Load` of a known function name should materialise the same string instead
of reading an uninitialised local slot. CRUSH-266 makes this load-bearing:
every `export function` in a JS module now keeps its body *and* its
`Export { Var(name) }` statement.

## Success criteria

- [x] The reproduction above runs and prints `42`.
- [x] A declared local still loads its variable slot (shadowing covered by the compiler test's ordinary-local case; declared names take the `Load` path).
- [x] Lambda capture checks (CRUSH-251) are unchanged for non-function vars.
- [x] `cargo test -p crush-frontend -p crush-lang-js` green.

## Technical approach

- Track `Program.functions` names in `Compiler` before the per-function compile loop.
- In the `Expression::Var` arm: if the name is a program function and not a declared local/param (declared vars shadow), emit `push_str <name>` (the toolchain's function-value representation) instead of `Load`.

## Files to modify

- `crates/crush-frontend/src/compiler.rs` — program-function name set + `Var` arm.

## Non-goals

- First-class function values / calling a function stored in a variable (CRUSH-251).
- The AOT backends' rejection of `export_var` (separate gap; AOT fails loudly today).

## Resolution

Superseded before merge (2026-10-20, on rebase of `agent/muse/CRUSH-266`
onto `origin/main` v0.7.1+). The CRUSH-250 function-values work already
implements this properly in `crush-frontend/src/compiler.rs`: a `Var` naming
a program function (tracked in `function_arity`) and not shadowed by a
declared local emits `make_closure { function, arity, ncap: 0 }` — a real
closure value, strictly better than this ticket's proposed `push_str`
name-string. The `push_str` implementation written against the old `main`
was dropped during the rebase (`git checkout --ours` on `compiler.rs` and
its tests). End-to-end coverage that the original reproduction prints `42`
lives in CRUSH-266's `crates/crush-lang-js/tests/export_defs_test.rs`
(`exported_function_is_callable_end_to_end`), which passes on the rebased
branch. No code change in this ticket.
