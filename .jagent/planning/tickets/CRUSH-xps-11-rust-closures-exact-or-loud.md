# CRUSH-xps-11 — Make Rust closure execution exact or fail loudly

| Field | Value |
|-------|-------|
| **ID** | CRUSH-xps-11 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 / SDK correctness |
| **Assignee** | unassigned |
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

- [ ] A capture-free closure fixture executes through RustAdapter → CAST →
      frontend → Runtime and asserts the exact result `42`; after CRUSH-xps-3
      lands, the same fixture also runs through the public SDK API.
- [ ] Capturing closures either match `rustc` for the documented subset or are
      rejected before VM execution with a useful construct-specific error.
- [ ] No accepted closure reaches the VM with uninitialized local slots.
- [ ] Tests do not use vacuous assertions such as `!out.is_empty() || true`;
      successful execution must assert output, and unsupported behavior must
      assert the explicit error.
- [ ] `crush-lang-rust` documents the precise supported closure/capture subset.

## Notes

- The current failure is in the xps SDK integration job on [PR #213](https://github.com/nixpt/crush-ast/pull/213).
- Exosphere's older `crush-lang` compiler rejected lambdas outright, so it is
  not evidence that this source-to-CVM1 closure path works.
