# CRUSH-xps-7 — Make Rust collection and string value semantics exact or loud

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-7 |
| **Priority** | P1 |
| **Status** | In review |
| **Phase** | M2 / Rust parity |
| **Assignee** | nixp (user-directed) |
| **Dependencies** | CRUSH-xps-3, CRUSH-xps-5, CRUSH-xps-6 |
| **Estimated effort** | L |

## Problem

Rust `Vec`, `String`, and structs have value-oriented clone/assignment behavior,
while Crush arrays and objects are reference types. The Rust walker currently
accepts `clone()` without copying, so mutating the clone can unexpectedly
mutate the source.

## Success criteria

- [x] `let a = vec![1,2,3]; let mut b = a.clone(); b.push(4); println!("{} {}", a.len(), b.len());`
      prints `3 4`, or the unsupported clone operation fails loudly.
- [x] Assignment and argument passing for supported `Vec`/`String`/struct
      forms either match Rust value semantics or fail loudly.
- [x] The exact-or-loud corpus records each supported value operation and
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

## Implementation

- Resolve lexical bindings first, then check a bounded scalar/text/sequence
  value domain using Rust type hints retained in CAST metadata.
- Flat Vec/array `clone`, `to_vec`, and `to_owned` use the existing shared
  `seq.slice` capability to create fresh storage. Scalar and immutable runtime
  string clones preserve values; local String `+=` replaces the binding.
- Fresh literals/clones/function results support assignment and known compatible
  owned parameters. Assignment expressions keep their unit result type.
- Reject implicit owned binding transfers, nested/unknown clones, owned captures,
  escaping borrowed views, explicit references, indexed writes, `pop`/`unwrap`,
  and unproven adapters with source locations. Valid Rust moves can be rejected;
  this is not Rust type checking or a borrow checker. Struct syntax remains
  explicitly rejected pending COSM's Rust producer.
- Add 21 rustc differential fixtures and seven walker checks. Keep namespace
  shadowing exact; separate the unsupported Option case from that fixture.
  Remove the original vector-clone silent exception from all three engines.

## Validation

Integrated current `origin/main` (`a1b7fdf`, including CRUSH-265's shared compiler
fix) through the SDK parent stack without rewriting its published history.
Validation uses this worktree's dedicated Cargo target.

- SDK + Rust walker libraries/integration tests: 588 passed.
- AOT library + Rust/C conformance tests: 20 passed.
- Shared frontend compiler tests: 46 passed.
- Scoped walker Clippy and formatting of new files/policy pass; existing
  unrelated warnings remain.
- All 58 fixtures recompiled and ran with `rustc --edition=2021 -O`.
  VM: 34 exact / 21 loud / 3 known silent; each AOT backend:
  26 exact / 29 loud / 3 known silent. The remaining silent entries are
  integer widths/overflow (xps-8) and string Debug formatting (xps-9).
- Live external SDK host compiled `.rs` files and matched real rustc output:
  original clone `3 4`; repeated clone `5 4 5 4`; cloned arguments `4 3 3`;
  strings `seed seed! seed!? seed.`; text element clones `1 2 2 one two`.
  Assignment-as-unit plus subsequent clones matched rustc (`1 1 2`).
  Implicit owned transfer failed at `3:17`. A `.crush` program printed
  `crush sdk unchanged` and returned `Int(42)` through the same host API.
