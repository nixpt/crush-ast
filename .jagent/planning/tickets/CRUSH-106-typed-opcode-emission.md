# CRUSH-106 — Emit typed casm::OpCode directly; JSON becomes a serialization view

| Field | Value |
|-------|-------|
| **ID** | CRUSH-106 |
| **Priority** | P1 |
| **Status** | In Progress — bounded CASM opcode-surface slice landed; compiler migration remains open |
| **Phase** | Design/perf (CRUSH-71 audit finding #1) |

## Progress

The first bounded slice expands `casm::OpCode` and `Instruction::to_opcode` to cover compiler-emitted opcode spellings that were previously unrepresentable, including try/throw, collection aliases, DOM, and additional AI operations. Focused serde/conversion tests preserve the JSON view. The full frontend migration away from `serde_json` construction remains open and is not claimed complete by this slice.

The next slice routes the five literal expression branches through typed `OpCode` construction before materializing the compatibility `Instruction` JSON view. The bridge intentionally still allocates the legacy args object; remaining compiler call sites are open. A compiler-level regression test now covers all five branches. The bounded variable path (VarDecl/Assign/Var) now uses typed `Load`/`Store`; remaining load/store sites are open.

## Problem

CRUSH-71 audit §3.1 finding #1: instructions are built out of
`serde_json::Value` — `compiler.rs:2271-2288` (`create_instr`, 201 call
sites, 254 `json!` literals) + `casm/src/lib.rs:236-244`. Per instruction:
a String opcode, a serde_json::Map for args (+ String key per field), a
second (essentially always empty) map for meta — 4–6 heap allocations each.
A 10k-instruction program pays 40–60k allocations in codegen alone, and every
consumer pays AGAIN via `to_opcode()` re-parsing JSON at load. The typed
`casm::OpCode` enum already exists (`casm/src/lib.rs:66-222`) — the middle is,
once more, not connected.

## Approach

Compiler emits `Vec<OpCode>` directly; JSON becomes a serialization/debug
view derived from the typed form (serde on OpCode), not the construction
medium. Migrate `create_instr` call sites mechanically (the audit counted
them; the shape is uniform). Consumers reading JSON keep working via the
serialization view; in-process consumers (VM assemble path, notebook) switch
to the typed path and drop their re-parse.

## Definition of done

- [ ] Codegen constructs no serde_json values on the emit path (allocation
      counter or heap-profile evidence, before/after quoted)
- [ ] JSON round-trip view preserved (existing .casm.json fixtures byte-stable
      or migration documented)
- [ ] Compile bench delta quoted (audit baseline: docs/design/CRUSH-71/)
- [ ] `cargo test --workspace` green

## Files in scope

- `crates/crush-frontend/src/compiler.rs`, `crates/casm/src/lib.rs`;
  consumer touch-points (assembler, notebook path) as needed

## Gates

None. Coordinates with CRUSH-79/74 on where debug_info attaches.
