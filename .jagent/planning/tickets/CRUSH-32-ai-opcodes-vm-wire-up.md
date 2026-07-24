# CRUSH-32 — AI opcodes VM-side wire-up (CRUSH-1 pickup: Query, Synthesize, AgentDelegation, …)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-32 |
| **Priority** | P0 — very high. Unblocks `crush-notebook` AI-native cells. AOT-side stubs already merged to `main` (`f49ece5`); this ticket closes the VM-side gap. |
| **Status** | Done |
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Closed by** | `07f64ee` (feat: CRUSH-32 -- AI opcodes VM-side wire-up) |

| **Dependencies** | Existing AOT stubs at `f49ece5` 2026-07-20 (salvaged from retired `agent/buffy/CRUSHAST-CRUSH-1`); no upstream CRUSH-NN blocker |
| **Estimated effort** | L |

## Origin

Filed s394 (2026-07-23) from `.jagent/planning/ROADMAP.md` M5 section.
CRUSH-1 was filed as a separate ticket in the existing ladder (M1 line in
`.jagent/planning/TASKS.md`: "CRUSH-1 (L): Wire 10 AI-native opcodes + spawn/
await/yield to real VM execution … currently all NOP"). AOT-side stubs for
those opcodes were salvaged into commit `f49ece5` (2026-07-20, `main`) when
the `agent/buffy/CRUSHAST-CRUSH-1` workstream was retired; this is confirmed
by `.dejavue/timeline.jsonl` ("CRUSH-1: wire AI opcode stubs into AOT Rust +
C backends" → `f49ece5`) and `.dejavue/decisions.md` ("Author type: agent"
entry at 2026-07-20T15:56:29). **The VM-side wiring is what's left.**

`TODO.md` priority 1 ("Make AI opcodes real") — `crush-notebook` AI-native
cells have been blocked by this gap since the notebook workstream started.
This ticket closes the **AI opcodes half** of priority 1 — i.e., TODO.md
priority 1 lines 1+2 — the 10 AI opcodes (Query, Synthesize,
AgentDelegation, SemanticMatch, LearningLoop, ContextAware, ToolChain,
GoalDeclaration, ProgressUpdate, KnowledgeSharing). DOM opcodes (TODO.md
priority 1 line 3) are CRUSH-33. spawn/await/yield (TODO.md priority 1
line 4) are CRUSH-34.

## Problem

`TODO.md` priority 1 enumerates the AI-shape opcodes that need VM-side
execution (today all NOP):

- `Query`, `Synthesize`, `AgentDelegation`, `SemanticMatch`, `LearningLoop`,
  `ContextAware`, `ToolChain` (7 AI ops)
- `GoalDeclaration`, `ProgressUpdate`, `KnowledgeSharing` (3 more)

(That priority list also includes DOM opcodes — those are `CRUSH-33` — and
spawn/await/yield — those are `CRUSH-34`. This ticket covers only the 10
listed above — strictly the AI opcodes.)

For each of the 10:

- AOT parse time emits a stub produced by `f49ece5` (codegen.rs and
  codegen_c.rs both have stubs that mostly return Null today)
- NOP at the VM level: not in `crush-vm/src/scheduler.rs`'s dispatch loop,
  not in `portable_vm.rs`, not in FastVM, not in `crush-jit`'s lowering.
  Falls through to the default error arm at every tier.
- Differential coverage gap: `crush-diff` (per CRUSH-13's existing
  5-arithmetic-implementation harness) doesn't include these new opcodes
  yet.

## Success criteria

- [ ] All 10 opcodes defined in `casm::Instruction` enum (verify presence
      in `f49ece5`-derived codegen output; add if missing).
- [ ] Each opcode has a corresponding dispatch branch in
      `crush-vm/src/scheduler.rs`, `crush-vm/src/portable_vm.rs`,
      `crush-vm/src/fastvm/`, and `crush-jit/src/compiler.rs`.
- [ ] `crush-notebook` regression test: a cell containing
      `ai.query("What is the workspace structure?")` returns a structured
      `Value::Map`-shaped response.
- [ ] All 5 execution tiers (scheduler, portable_vm, fastvm, aot-rust,
      aot-c) **agree** on the outputs of all 10 opcodes (per the existing
      `crush-diff` differential harness — extends CRUSH-13 to cover these
      new opcodes).
- [ ] 60+ unit tests across `crush-vm` covering each opcode (10 ops × ~6
      cases each = ~60): basic shape, error-path, cap-call surface, empty
      workspace, multi-tier differential, deterministic-mode parity.

## Technical approach

1. **Opcode definitions.** Verify `casm::Instruction` carries all 10
      variants from `f49ece5`'s codegen output; add any missing (likely
      none, but re-verify).
2. **VM dispatch.** Add 10 branches (one per opcode) to the
      `match Instruction { ... }` in `scheduler.rs::dispatch_one` (and
      the parallel match in `portable_vm.rs`). Each branch delegates to
      a new `host_cap::ai_*` family of caps; no arithmetic, no control
      flow inside the branch other than the cap call.
3. **FastVM lowering.** Add the 10 opcodes to FastVM's instruction table;
      for now, FastVM is a noop wrapper that delegates to the same
      `host_cap::ai_*` functions.
4. **JIT lowering.** Add 10 Cranelift-IR expressions for each opcode;
      similar delegate-to-cap pattern.
5. **Caps.** Implement `host_cap::ai_query`, `ai_synthesize`, etc. in
      `crush-lang-sdk` — at minimum, stub implementations that return
      `Value::Map` of `{ status: "ok" | "not-implemented" }` plus a
      per-cap test mock provider.
6. **Differential coverage.** Extend `crush-aot/tests/differential_aot.rs`
      (per CRUSH-13) to include the 10 new opcodes in its cross-tier test
      matrix. Tier parity assertion: all 5 tiers produce byte-equal
      outputs for each opcode's standard test input.

## Files to modify

- `crates/crush-vm/src/scheduler.rs` — 10 dispatch branches
- `crates/crush-vm/src/portable_vm.rs` — 10 mirror branches
- `crates/crush-vm/src/fastvm/` — 10 lowering entries
- `crates/crush-jit/src/compiler.rs` — 10 Cranelift lowering
- `crates/crush-lang-sdk/src/host_caps.rs` — `ai_*` provider registration
- `crates/crush-lang-sdk/src/ai_caps.rs` (new) — stub impls
- `crates/crush-aot/tests/differential_aot.rs` — extend CRUSH-13's
  harness with the 10 new opcodes

## Non-goals

- **No real AI backend integration.** Stub caps return
  `{ status: "not-implemented" }` initially; real backend wiring
  (e.g., Ollama, OpenAI-compat) is a separate ticket post-M5.
- **DOM opcodes (CRUSH-33) are out of scope here.**
- **spawn/await/yield (CRUSH-34) is out of scope here.**
- **No `@module`/`@errors` annotation enforcement.** Annotation-node work
  lives in CRUSH-27; this ticket is purely the VM dispatch.

## Cross-references

- `.jagent/planning/ROADMAP.md` — M5 ticket 6 of 8
- `.jagent/planning/TASKS.md` M1 line "CRUSH-1 (L): Wire 10 AI-native
  opcodes" — this ticket is the realization of that line item
- `TODO.md` priority 1 — "Make AI opcodes real"
- `CRUSH-DISTRIB` (`f49ece5` 2026-07-20) — the AOT stub salvage this
  ticket builds on, per `.dejavue/timeline.jsonl`
- `.dejavue/decisions.md` 2026-07-20T15:56:29 entry — same salvage record
- `crush-notebook` workstream — the consumer blocked on this ticket
- CRUSH-13 — the existing differential-test harness this ticket extends
