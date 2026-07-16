# CRUSH-15 — `crushc --emit casm` and `crush-run`'s CASM assembler speak incompatible dialects

| Field | Value |
|-------|-------|
| **ID** | CRUSH-15 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

`crushc foo.crush --emit casm` and `crush-vm/src/assembler.rs` (used by
`crush-run run foo.casm`) are two different, textually incompatible CASM
dialects:

- `crushc --emit casm` emits `.permission`/lowercase-mnemonic `key=value`
  text — a raw `Instruction`-list dump.
- The assembler only recognizes `.func` plus UPPERCASE mnemonics, matching
  `crush-lang-sdk::compile::casm_to_vm`'s internal text-generation
  convention.

`crushc foo.crush --emit casm` output cannot be fed into `crush-run run
foo.casm`, even though the CLI's own doc comments imply that round-trip
works. Found while trying to manually verify a Python-lowering ticket via
`crush-run`; the workaround was `--emit vm` (CVM1 binary), which round-trips
fine — this ticket is about the *text* dialect specifically.

## Impact

Low severity (a working binary round-trip exists via `--emit vm`), but any
user or agent trying to inspect/hand-edit CASM as text and feed it back in
hits a confusing, undocumented failure. Misleading docs are worse than no
docs here — the implication that this works sends people down a dead end.

## Reproduction

```bash
crushc /tmp/foo.crush --emit casm -o /tmp/foo.casm.txt
crush-run run /tmp/foo.casm.txt
# parse error — assembler expects .func + UPPERCASE, got .permission + lowercase
```

## Technical approach

Either (a) unify the two textual dialects (pick one, make both
`crushc --emit casm` and the assembler agree), or (b) if intentionally
different formats for different purposes, fix `crushc`'s docs/help text to
stop implying `--emit casm` output is assembler-consumable. (a) is
preferable if there's no reason for two dialects to exist.

## Files to modify

- `crates/crush-lang-sdk/src/bin/crushc.rs` (or wherever `--emit casm`
  is implemented) — the emission side
- `crates/crush-vm/src/assembler.rs` — the parsing side
- Whichever CLI doc comments currently imply the round-trip works

## Non-goals

- Changing the CVM1 binary format or `--emit vm` (already works correctly)
