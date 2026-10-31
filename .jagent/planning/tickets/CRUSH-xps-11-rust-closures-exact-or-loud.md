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

## Detailed review specification (2026-10-15)

[Batch spec](../designs/rust-parity-xps-8-13.md#xps-10-12-close-the-existing-review-contracts).
The implementation remains **In review in PR #213**; PR #217's landed typed
parameter fix is part of its baseline. This is closure verification of that
work, not a new closure runtime implementation.

### Supported and refused boundary

| Form | Contract |
|---|---|
| Synchronous closure with named/typed/wildcard parameters | Ordered argument slots; wildcard names cannot collide with user locals |
| Complete statement body with tail/explicit returns | Preserve every supported effect and result, including branches |
| Scalar read and mutable scalar capture | Existing shared-cell behavior matches valid Rust fixtures; capture refers to xps-6 binding identity |
| Shadowed parameters, captures, or loop bindings | Distinct storage identity and correct lexical resolution |
| Owned/unknown collection or String capture | Source-located refusal from xps-7 until ownership semantics exist |
| Move/async closures, destructured/ref parameter patterns | Explicit lowering refusal, not a late uninitialized slot |
| Closure-valued programs on AOT | Retain explicit closure-opcode refusal under CRUSH-251 until that backend work lands |

Do not infer Fn/FnMut/FnOnce trait completeness from VM closure execution.
Numeric precision/width on parameters and results belongs to xps-8; closure
checks must preserve those facts when its implementation arrives.

### Review matrix

| Case | Required proof |
|---|---|
| `let add = |x: i32| x + 1; add(41)` | Exact `42` through the public SDK |
| Several typed/wildcard parameters and unused arguments | Correct order, arity and collision-free slots |
| Parameter shadows captured/outer `x` | Correct lexical binding rather than accidental capture |
| Scalar read and repeated mutable scalar capture calls | Exact rustc outputs across repeated calls |
| Body declarations, effects, nested branches and early return | All statements retained, tails and returns correct |
| Wrong call arity and unknown body names | Compile-time rejection before VM execution |
| Owned/move/async/destructured/ref cases | Construct-specific source diagnostic with the correct owning ticket |
| Same corpus through Rust/C AOT | Exact where supported or the pinned CRUSH-251 compile-time refusal |

### Review exit

- [ ] Tie every supported/refused form to a regression and an explicit
      rustc-backed fixture or frontend-only rejection case. No vacuous success
      assertion, arbitrary nonempty output, or arbitrary runtime error counts.
- [ ] Recheck capture identity after xps-6 and xps-7 are integrated; no accepted
      fixture may reach an uninitialized slot or acquire an unproven owned alias.
- [ ] SDK, shared conformance and supported native proof are recorded on the
      current integration state. Keep CRUSH-251 separate from this ticket.
- [ ] Merge PR #213 and required dependent corrections before marking Done;
      update the supported closure/capture documentation with the same change.

## Dispatch for review closure

Read Rust closure lowering, binding metadata, xps-7 capture checks, shared
lambda semantic checking and SDK `rust_source`/`rustc_corpus`. Inspect existing
PR #213 tests before adding anything. Correct only demonstrated gaps in a
focused worktree; do not expand owned capture or closure traits in this lane.
