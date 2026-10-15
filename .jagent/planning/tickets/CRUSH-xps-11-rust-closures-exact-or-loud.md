# CRUSH-xps-11 — Make Rust closure execution exact or fail loudly

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-11 |
| **Priority** | P1 |
| **Status** | In review |
| **Phase** | M1 / SDK correctness |
| **Assignee** | nixp (user-directed) |
| **Dependencies** | CRUSH-xps-2, CRUSH-197 |
| **Estimated effort** | M |

## Problem

The Rust walker documents closures in its supported subset and lowers a
capture-free closure into CAST `Lambda`, but the SDK integration case
`let add = |x: i32| x + 1; println(add(41));` fails at runtime with
`load from uninitialised slot 1`. The current test accepts arbitrary successful
output and only tolerates a narrow set of error strings, so it neither proves
the advertised behavior nor enforces an intentional unsupported boundary.

## Goal

Give Rust closures an honest, tested contract: supported closures execute with
Rust-compatible output; unsupported capture forms fail during compilation or
lowering with a useful diagnostic before entering the VM.

## Scope

- In: capture-free closures first, capture behavior, explicit diagnostics, SDK
  integration tests, and the Rust supported-subset documentation.
- Out: a general Rust borrow checker, unrestricted closure traits, or changes
  to Crush's closure model beyond what the Rust adapter needs.

## Acceptance

- [x] A capture-free closure fixture executes through RustAdapter → CAST →
      frontend → Runtime and asserts the exact result `42`; after CRUSH-xps-3
      lands, the same fixture also runs through the public SDK API.
- [x] Capturing closures either match `rustc` for the documented subset or are
      rejected before VM execution with a useful construct-specific error.
- [x] No accepted closure reaches the VM with uninitialized local slots.
- [x] Tests do not use vacuous assertions such as `!out.is_empty() || true`;
      successful execution must assert output, and unsupported behavior must
      assert the explicit error.
- [x] `crush-lang-rust` documents the precise supported closure/capture subset.

## Notes

- The current failure is in the xps SDK integration job on [PR #213](https://github.com/nixpt/crush-ast/pull/213).
- Exosphere's older `crush-lang` compiler rejected lambdas outright, so it is
  not evidence that this source-to-CVM1 closure path works.

## Implementation and validation in PR #213

- PR #217 landed the typed-parameter and initial closure-body fix while this
  work was in progress. Its wildcard support and corpus are preserved and moved
  to the SDK integration layer with the other Rust execution fixtures.
- The shared semantic pass checks lambda scopes, body names, and call arity.
  Wildcard slots cannot collide with user-named parameters.
- Capture-free, typed, wildcard, scalar-capture, and mutable scalar-capture
  fixtures run through the public SDK API and match `rustc` exactly.
- Move/async closures and unsupported parameter patterns fail before execution.
  Array and closure tests assert concrete results instead of vacuous success.
- Rust ownership and repeated block/loop binding parity remain CRUSH-xps-6/7;
  the full VM/AOT conformance gate remains CRUSH-xps-5.
