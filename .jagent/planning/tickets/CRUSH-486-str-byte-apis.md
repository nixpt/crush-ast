# CRUSH-486 — Add explicit byte APIs: str.byte_len(s) and str.bytes(s)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-486 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#486](https://github.com/nixpt/crush-ast/issues/486) |
| **Size** | M |
| **Dependencies** | CRUSH-453, CRUSH-467 |

## Problem

Ruling 3 makes len/index character-based and requires explicit byte APIs for byte work. No str.byte_len or str.bytes capability exists; the only length capability is str.len (caps.rs:207, scheduler.rs:2587, portable_vm.rs:2419), which CRUSH-STR-LEN-CHARS turns into a character count.

## Evidence (at origin/main 7ecd2515)

- crates/crush-vm/src/caps.rs:207-212
- crates/crush-vm/src/scheduler.rs:2587
- crates/crush-vm/src/portable_vm.rs:2419
- crates/crush-lang-sdk/src/stdlib.rs (str_cap! host caps)
- ruling 3

## Goal

Two capabilities: str.byte_len(s) -> Int (UTF-8 length) and str.bytes(s) -> array of Int 0..=255, registered in caps.rs beside str.len and implemented on the same path as str.len so scheduler, PortableVm and FastVM all serve them (first step: confirm how FastVM resolves str.len and follow it). Both are listed in the crush-run capability help and the capability reference doc.

## Non-goals

- Bytes literals or a Bytes value type for strings
- The char-count switch (CRUSH-STR-LEN-CHARS)
- AOT and JIT support (refusal is acceptable)

## Files

- crates/crush-vm/src/caps.rs
- crates/crush-vm/src/scheduler.rs
- crates/crush-vm/src/portable_vm.rs
- crates/crush-vm/src/fastvm/ (wherever str.len is served)
- crates/crush-lang-sdk/src/bin/crush-run.rs
- crates/crush-lang-sdk/tests/tier1_str_bytes.rs (new)

## Done when

- tier1_str_bytes: str.byte_len('héllo')==6, str.bytes('é')==[195,169], str.bytes('')==[] on scheduler, PortableVm and FastVM
- the capability registry test lists str.byte_len and str.bytes
- cargo +1.100.0 clippy -p crush-vm -p crush-lang-sdk --all-targets -- -D warnings is clean

## Depends on

- CRUSH-453
- CRUSH-467

## Refs

- #368
- ruling 3

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
