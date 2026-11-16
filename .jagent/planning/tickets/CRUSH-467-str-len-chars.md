# CRUSH-467 — len(s), str.len and str.index_of count Unicode characters (ruling 3)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-467 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | 0.12 (freeze prep) |
| **Issue** | [#467](https://github.com/nixpt/crush-ast/issues/467) |
| **Size** | M |
| **Dependencies** | CRUSH-453 |

## Problem

Ruling 3: strings count characters everywhere. Today crush_vm::str_len::str_len returns s.len() (bytes) and its doc names byte length as canonical (CRUSH-114). That helper backs len() and str.len on the scheduler, PortableVm, FastVM and the JIT. str.index_of (StrIndexOfCap, stdlib.rs:438-444) returns a byte offset via s.find. Meanwhile s[i], slices and str.char_at count characters, so mixing them cuts the wrong text (#368). Recorded decisions pin bytes (.dejavue/decisions.md:613-616, :1335), as do a unit test in str_len.rs and tests/data_types.rs:83.

## Evidence (at origin/main 7ecd2515)

- crates/crush-vm/src/str_len.rs:1-20,33-38
- crates/crush-vm/src/scheduler.rs:1407,2589; portable_vm.rs:1518,2421; fastvm/execution.rs:641; crates/crush-jit/src/runtime.rs:394
- crates/crush-lang-sdk/src/stdlib.rs:438-444
- crates/crush-vm/src/caps.rs:207-212
- crates/crush-vm/src/tests/data_types.rs:83
- .dejavue/decisions.md:613-616, :1335
- issue #368, ruling 3

## Goal

str_len returns chars().count(). StrIndexOfCap returns a character position (or -1). The str.len capability summary, the str_len.rs doc and the pinning tests state character semantics. A superseding decisions.md entry cites ruling 3 and the two superseded entries. CHANGELOG gets a behaviour-change note. The JIT follows through the shared helper.

## Non-goals

- byte_len / bytes() APIs (CRUSH-STR-BYTE-APIS)
- AOT Rust/C len (CRUSH-AOT-STR-LEN-CHARS); do not add non-ASCII cases to differential_aot until it lands
- H4 string-op coercion (open)
- Grapheme clusters: Unicode scalar values only

## Files

- crates/crush-vm/src/str_len.rs
- crates/crush-vm/src/caps.rs
- crates/crush-vm/src/tests/data_types.rs
- crates/crush-lang-sdk/src/stdlib.rs
- crates/crush-lang-sdk/tests/tier1_str_chars.rs (new)
- .dejavue/decisions.md
- CHANGELOG.md

## Done when

- tier1_str_chars: len('héllo')==5, len('😀')==1, str.len('héllo')==5, str.index_of('héllo','l')==2, str.index_of('héllo','z')==-1 on scheduler, PortableVm and FastVM
- s[str.index_of(s,'l')] == 'l' for s='héllo' on all three engines (the #368 round trip)
- str_len.rs doc and caps.rs str.len summary no longer say 'byte'
- decisions.md has a superseding entry citing ruling 3; CHANGELOG has the behaviour note
- cargo +1.100.0 clippy -p crush-vm -p crush-lang-sdk --all-targets -- -D warnings is clean

## Depends on

- CRUSH-453

## Refs

- #368
- CRUSH-114
- CRUSH-217
- ruling 3

Rulings: /workspace/workspace-meta/plans/2026-11-13-crush-ast-rulings.md (captain, 2026-11-13). Drafted by the ticket-draft workflow (s518), checked by Opus, final cut by Fable; captain checkpoint 2 on 2026-11-13. CI clippy is pinned to 1.100.0; engine tickets need a test per engine they claim; JIT tests assert the JIT compiled.
