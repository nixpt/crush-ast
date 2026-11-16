# CRUSH-478 — casm spec absorbs the COSM ops (0xC0-0xCA) and a test ties spec to code

| Field | Value |
|-------|-------|
| **ID** | CRUSH-478 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#478](https://github.com/nixpt/crush-ast/issues/478) |
| **Size** | M |
| **Dependencies** | CRUSH-458 |

## Problem

crates/casm/specs/casm_v1.0.md is marked FROZEN (2026-01-15) and has 41 op sections, none for get_member, set_member, has_member, del_member, call_member, construct, get_slot, set_slot, is_kind, copy, new_record (0xC0-0xCA) or load_kind (0xCB). The runtime defines all of them (crates/crush-vm/src/bytecode.rs:200-233). casm's OpCode/text parser accepts every one except new_record (casm/src/lib.rs:248-285, ~588-622); NEW_RECORD exists only in the CVM1/CVMS assembler (assembler.rs:537,730). Nothing ties the spec to the code, so it has already drifted once (RC F4).

## Evidence (at origin/main 7ecd2515)

- crates/casm/specs/casm_v1.0.md (41 '#### ' sections, no COSM ops)
- crates/crush-vm/src/bytecode.rs:200-233
- crates/crush-vm/src/assembler.rs:527-537,720-730
- crates/casm/src/lib.rs:248-285,~588-622
- ruling 12/15

## Goal

Add spec sections for 0xC0-0xCA (operands, stack effect, error behaviour as the VMs implement it today) and load_kind 0xCB. Mark new_record explicitly 'CVM1/CVMS only, no CASM text form'. Expose casm's accepted op names as a list (a const or fn the parser also uses). Add crates/casm/tests/spec_opcode_table.rs: it extracts '#### <op>' headings and fails on any op present in one list and absent from the other, with an explicit allowlist (new_record plus any pre-existing gaps, each with an issue number). Bump the spec revision/date; keep it 1.0 additive.

## Non-goals

- Documenting AI/other non-spec ops (allowlist with an issue number and list in the PR)
- COSM dispatch semantics (#2, no ruling) beyond what the engines do today
- Adding a casm NewRecord op or changing opcodes

## Files

- crates/casm/specs/casm_v1.0.md
- crates/casm/tests/spec_opcode_table.rs (new)
- crates/casm/src/lib.rs (op-name list shared with the parser)

## Done when

- cargo test -p casm --test spec_opcode_table passes; deleting one new spec section makes it fail
- the spec contains the 11 ops 0xC0-0xCA (new_record marked CVM1-only) plus load_kind 0xCB
- cargo test -p casm passes (parser behaviour unchanged)
- cargo +1.100.0 clippy -p casm --all-targets -- -D warnings clean

## Depends on

- CRUSH-458

## Refs

- ruling 12/15
- RC gate F4

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
