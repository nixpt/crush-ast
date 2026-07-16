# CRUSH-12 — Any struct declaration silently kills `main`

| Field | Value |
|-------|-------|
| **ID** | CRUSH-12 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |

## Problem

```crush
struct P { x }
fn main() { print("hi") }
```

Compiles and runs with exit code 0, `steps=2`, and **prints nothing**.
`main` is never called. No error, no warning. This is not new — verified
against the parent commit before the s385 parser work, so struct programs
have never worked. `concurrency_structs.crush` (part of exosphere's
language corpus) now *parses* (7/7 corpus, fixed elsewhere) but still
cannot *run* because of this bug specifically.

## Impact

The purest silent failure in the codebase: zero exit code, zero error
output, zero indication anything is wrong. Any program that declares a
struct anywhere — even one never touched by `main`'s logic — has its
entire `main` body silently skipped. This is the flagship target named in
`workspace-meta/plans/2026-07-14-PARKING-LOT-bugarium-vs-crush.md`.

## Reproduction

```bash
cat > /tmp/struct_kills_main.crush <<'EOF'
struct P { x }
fn main() { print("hi") }
EOF
crushc /tmp/struct_kills_main.crush -o /tmp/skm.cvm1
crush-run run /tmp/skm.cvm1
# exits 0, prints nothing
```

The VM's `steps=` instruction counter is a free oracle here — assert
`main`'s body executed >= 1 instruction. `steps=2` for a program whose
`main` body alone should need several instructions to print a string is
itself the signal something upstream (top-level-statement compilation,
struct-declaration handling in the compiler's function-table walk, or
`main`'s entry-point lookup) is short-circuiting before `main` is ever
reached.

## Technical approach

Not investigated — found via black-box testing. Likely starting points:

- `crates/crush-frontend/src/compiler.rs` — top-level statement/declaration
  compilation order; check whether a `struct` declaration is consuming or
  corrupting the entry-point (`main`) lookup, similar in shape to the
  already-fixed "ANY top-level statement silently discarded an explicit
  `fn main`" bug from the CRUSHAST-RELEASE-1 arc (a different bug, same
  failure class: top-level items interfering with `main` dispatch).
- Check whether struct *type registration* happens in a pass that
  overwrites or skips the function table instead of appending to it.

## Files to modify

- `crates/crush-frontend/src/compiler.rs` (primary suspect — entry-point /
  top-level-declaration handling)
- `crates/crush-frontend/src/parser/` (if struct parsing itself consumes
  more of the token stream than it should)

## Non-goals

- Full struct feature completeness (methods, generics, etc.) — this
  ticket is just "declaring a struct must not prevent `main` from running"
