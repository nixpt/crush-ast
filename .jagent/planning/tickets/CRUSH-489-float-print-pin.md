# CRUSH-489 — Pin float print (Rust Display rule) on all three Tier-1 engines and document it (ruling 4)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-489 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#489](https://github.com/nixpt/crush-ast/issues/489) |
| **Size** | S |
| **Dependencies** | CRUSH-453 |

## Problem

Ruling 4 keeps Rust Display for float print and asks for it to be documented (#393). The rule lives in one function (crush_float_to_text: shortest round-trip digits, never an exponent, '.0' on whole numbers, NaN/inf/-inf) that scheduler, PortableVm and FastVM all call. But the only end-to-end pin (crates/crush-aot/tests/float_text_agreement.rs) runs the scheduler VM against AOT, never PortableVm or FastVM, and no user-facing doc states the rule. A later 'CPython repr' fix could land on one engine unnoticed.

## Evidence (at origin/main 7ecd2515)

- crates/crush-vm/src/float_text.rs:1-25
- crates/crush-vm/src/vm.rs:498, crates/crush-vm/src/portable_vm.rs:3049, crates/crush-vm/src/io_print.rs:169
- crates/crush-aot/tests/float_text_agreement.rs
- issue #393, ruling 4

## Goal

A tier1 test prints a float table (1e21 as 1000000000000000000000.0, 0.0000001, 0.1+0.2, 1.0, -0.0, NaN, inf, -inf, plus string concatenation of one) and asserts identical stdout equal to crush_float_to_text on scheduler, PortableVm and FastVM. A docs page states float print is the Rust Display rule (no exponent), not CPython repr. #393 is closed with a pointer to the doc.

## Non-goals

- Changing the format
- py_float / CPython repr for the Python walker
- AOT (already pinned)

## Files

- crates/crush-lang-sdk/tests/tier1_float_print.rs (new)
- docs/language/float-print.md (new) or docs/FILE-FORMATS.md

## Done when

- tier1_float_print passes on scheduler, PortableVm and FastVM (stdout compared, FastVM driven with io.print granted)
- a docs page names the Rust Display rule (no exponent) as the print format and cites ruling 4
- cargo +1.100.0 clippy -p crush-lang-sdk --all-targets -- -D warnings is clean

## Depends on

- CRUSH-453

## Refs

- #393
- ruling 4
- CRUSH-229

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
