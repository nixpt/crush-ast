# CRUSH-487 — AOT Rust and C: len on a string counts characters (ruling 3, Tier-2 follow-through)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-487 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#487](https://github.com/nixpt/crush-ast/issues/487) |
| **Size** | S |
| **Dependencies** | CRUSH-467, CRUSH-474 |

## Problem

After CRUSH-STR-LEN-CHARS every VM and the JIT count characters, but both AOT backends still count bytes: AOT Rust's "arr_len" | "len" arm uses s.len() (codegen.rs:1144) and AOT C uses strlen (codegen_c.rs:2208). Both were left byte-based on purpose by CRUSH-114 (decisions.md:616). The C runtime's _str_char_at and slice helper already walk UTF-8 by character. The existing agreement test (differential_aot.rs:458-467) is ASCII-only and cannot see the divergence.

## Evidence (at origin/main 7ecd2515)

- crates/crush-aot/src/codegen.rs:1144
- crates/crush-aot/src/codegen_c.rs:2208 and the _str_char_at / slice helpers
- .dejavue/decisions.md:616
- crates/crush-aot/tests/differential_aot.rs:458-467

## Goal

AOT Rust and AOT C len on a string return the Unicode scalar count, matching the VMs. differential_aot gets non-ASCII cases.

## Non-goals

- Tier-1 engines
- str.index_of in AOT
- Byte APIs in AOT
- JIT (follows str_len already)

## Files

- crates/crush-aot/src/codegen.rs
- crates/crush-aot/src/codegen_c.rs
- crates/crush-aot/tests/differential_aot.rs

## Done when

- differential_aot: assert_all_backends_agree on len('héllo') (5) and len('😀') (1) passes, covering the VMs, AOT Rust and AOT C
- cargo test -p crush-aot differential_aot passes
- cargo +1.100.0 clippy -p crush-aot --all-targets -- -D warnings is clean

## Depends on

- CRUSH-467
- CRUSH-474

## Refs

- #368
- CRUSH-114
- ruling 3

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
