# CRUSH-453 — Strict Tier-1 differential harness: compare value, stdout and error class on scheduler, PortableVm and FastVM, with capabilities

| Field | Value |
|-------|-------|
| **ID** | CRUSH-453 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#453](https://github.com/nixpt/crush-ast/issues/453) |
| **Size** | M |
| **Dependencies** | #448 |

## Problem

Release criterion L4 (a Tier-1 disagreement is a bug) can only be checked loosely today. crush_lang_sdk::differential::differential_run (crates/crush-lang-sdk/src/differential.rs:187-271) runs all three Tier-1 engines, but scheduler vs FastVM compares only ok vs err (248-256), never value, stdout or error kind; the module doc (30-34) says FastVM is 'flagged for review, not asserted'. FastVM also abstains whenever a program needs a capability (243) because the harness calls run_fastvm without caps, though run_fastvm_with_caps exists (crates/crush-vm/src/vm.rs:1160). Every program that prints is unchecked on FastVM. No normalised error class exists, so 'same error' cannot be asserted. FastVM is Tier-1 now (ruling 17).

## Evidence (at origin/main 7ecd2515)

- crates/crush-lang-sdk/src/differential.rs:30-34, 240-256
- crates/crush-vm/src/vm.rs:1152,1160 (run_fastvm vs run_fastvm_with_caps)
- crates/crush-aot/tests/differential_aot.rs:240-300 (outcome class plus scalar return only)
- PR #448 and PR #449 each hand-roll a per-engine loop
- ruling 17

## Goal

A strict Tier-1 entry point in crush_lang_sdk::differential (e.g. tier1_run(source) -> Tier1Report and assert_tier1_agree(source)). It runs scheduler, PortableVm and FastVM with the same capability grants (FastVM via run_fastvm_with_caps) and compares stdout, the returned value (via the existing Norm) and a normalised, #[non_exhaustive] ErrorClass enum (ArrayBounds, BadIndex, TypeError, DivByZero, ArithmeticOverflow, CapDenied, CapFailed, StepQuota, WallTimeQuota, Other(String)) mapped from VmError and FastError. Known disagreements are #[ignore = "CRUSH-... / #NNN"] tests that later tickets un-ignore.

## Non-goals

- Fixing any disagreement it finds (later tickets)
- AOT or JIT in the strict comparison (differential_aot.rs keeps that role)
- Changing the loose differential_run behaviour other tests rely on
- CI job wiring beyond cargo test

## Files

- crates/crush-lang-sdk/src/differential.rs
- crates/crush-lang-sdk/tests/tier1_agreement.rs (new)

## Done when

- cargo test -p crush-lang-sdk --test tier1_agreement passes with at least 10 agreeing programs, at least 3 using io.print (FastVM no longer abstains)
- one #[should_panic] test shows assert_tier1_agree failing, and the message names the disagreeing engine and both values or error classes
- each known disagreement found while seeding (at least the FastVM out-of-range read, #445) is an #[ignore] test naming its ticket or issue
- cargo +1.100.0 clippy -p crush-lang-sdk --all-targets -- -D warnings is clean

## Depends on

- #448

## Refs

- RELEASE-1.0-CRITERIA L4
- ruling 17
- CRUSH-13

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
