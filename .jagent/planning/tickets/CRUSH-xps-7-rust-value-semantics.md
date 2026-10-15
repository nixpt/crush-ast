# CRUSH-xps-7 — Make Rust collection and string value semantics exact or loud

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-7 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M2 / Rust parity |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-xps-3, CRUSH-xps-5 |
| **Estimated effort** | L |

## Problem

Rust `Vec`, `String`, and structs have value-oriented clone/assignment behavior,
while Crush arrays and objects are reference types. The Rust walker currently
accepts `clone()` without copying, so mutating the clone can unexpectedly
mutate the source.

## Success criteria

- [ ] `let a = vec![1,2,3]; let mut b = a.clone(); b.push(4); println!("{} {}", a.len(), b.len());`
      prints `3 4`, or the unsupported clone operation fails loudly.
- [ ] Assignment and argument passing for supported `Vec`/`String`/struct
      forms either match Rust value semantics or fail loudly.
- [ ] The exact-or-loud corpus records each supported value operation and
      removes the known-silent entry only after VM parity and AOT exact-or-loud
      coverage pass.

## Technical approach

- Define the supported value types and copy/clone boundary before lowering.
- Add differential fixtures for clone, assignment, mutation, and function
  arguments; do not infer value semantics from Rust syntax alone.
- Reject unimplemented ownership cases rather than silently aliasing values.

## Non-goals

- A general Rust borrow checker or ownership system in Crush.
- Full Rust `Vec`, `String`, or struct support.
