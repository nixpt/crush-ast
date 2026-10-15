# CRUSH-267 — `xs.slice(1)` method misroutes to capability dispatch

| Field | Value |
|-------|-------|
| **ID** | CRUSH-267 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M6 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

Slice *syntax* `xs[1:]` / `xs[1:3]` works (CRUSH-7, `8f30a96`), but the method form `xs.slice(1)` still misroutes to capability dispatch: `[capabilities] not available in this host: slice`. This is a **loud missing-feature error, not a silent bug** — surfaced while closing CRUSH-7 (2026-10-15).

## Success criteria

- [ ] `xs.slice(1)` and `xs.slice(1, 3)` lower to the same slice op as the `xs[1:3]` syntax and return the same result.
- [ ] A regression test covers the method form alongside the syntax form.

## Technical approach

- The compiler has a hardcoded allowlist of recognized array methods (`.push`/`.append`/`.set`) — `.slice` isn't in it, so it falls through to `cap_call`. Add `.slice` to the allowlist, lowering to the same `arr_slice`/`seq.slice` op the syntax form uses.

## Files to modify

- `crates/crush-frontend/src/compiler.rs` — array method-call dispatch allowlist
- `crates/crush-lang-sdk/src/compile.rs` — ensure the slice op is recognized in the method path

## Non-goals

- Step slicing `xs[::2]` (CRUSH-7 non-goal)
- Slicing non-array values (strings — see CRUSH-268)
