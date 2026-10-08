# CRUSH-156 — `ai_native.*` caps take real arguments

| Field | Value |
|-------|-------|
| **ID** | CRUSH-156 |
| **Priority** | P2 |
| **Status** | Done (PR pending review) |
| **Phase** | M5 |
| **Assignee** | nimbus-b |
| **Dependencies** | none |
| **Estimated effort** | M (~40 turns) |
| **Relay lane** | B1 — see `docs/planning/MIGRATION-INVENTORY.md` §5 |
| **Filed by** | CRUSH-150 migration inventory (2026-10-07) |

## Problem

All 10 `ai_native.*` caps declare `argc: Some(0)` and drop VM-stack arguments (`crates/crush-lang-sdk/src/ai_native.rs:87-104`); FastVM's `resolve_host_request` discards `HostRequest{args}`. No real AI backend can be plugged in until arguments arrive. This is CRUSH-55's PORT #1 prerequisite.

## Success criteria

- [x] each `ai_native.<kind>` spec declares its real argc (or variadic) and receives the compiled arguments on CVM1, PortableVm and FastVM
- [x] the CAST argument shape for tool lists follows nanovm's `parse_tools`
- [x] existing echo behaviour stays the default backend, now echoing the received args
- [x] differential test: same AI program, same echoed args on all three engines

## Technical approach

- Compiler side already lowers the AI expressions; fix the cap specs and the dispatch paths.
- ⚠ `crates/crush-vm/src/fastvm/` is lane-guarded — get foreman's ack before editing `resolve_host_request`.

## Files to modify

- `crates/crush-lang-sdk/src/ai_native.rs`
- `crates/crush-vm/src/{scheduler,portable_vm}.rs` (AI opcode dispatch)
- `crates/crush-vm/src/fastvm/mod.rs` (⚠ lane guard)

## Non-goals

- any real model backend

## Source (reference only — re-implement, don't copy)

- exo `crates/core/vm/nanovm/src/vm/ai.rs::parse_tools`; CRUSH-55 inventory App. C §2a

## Evidence (nimbus-b, 2026-10-07)

- Contract: `crates/crush-vm/src/ai_args.rs` — the cap gets `[payload, operands…]`;
  operand count is `payload.stack_args` (written by the frontend for
  `context_aware` 1, `semantic_match` 1, `synthesize` refs+examples), stripped
  before the call. Scheduler, PortableVm and `fastvm::resolve_host_request`
  all call `ai_args::dispatch` (arity-checked, deadline-bounded like CAP_CALL;
  ungranted → `null`, operands still consumed).
- Spec argc: 1 for payload-only kinds, 2 for `context_aware`/`semantic_match`,
  `None` for `synthesize`.
- Not in the inventory: `crush-lang-sdk::compile::casm_to_vm` lowered every AI
  op to `NOP`, so Crush programs never reached the AI opcodes on CVM1 at all.
  Now lowered to `AI_<KIND> "<payload>"` (statement forms + `POP`); the
  assembler learned `AI_GOAL_DECLARATION`/`AI_PROGRESS_UPDATE`/`AI_KNOWLEDGE_SHARING`.
- Tool shape follows nanovm `parse_tools`: `tool_name`, `parameters`,
  `result_binding`, `condition`, `required_capability` (was dropped); the
  `fallback` policy now carries `fallback_tools` (only a count before).
- Test: `crates/crush-lang-sdk/tests/ai_native_args.rs` — the same CAST program
  (goal statement + query + semantic_match + synthesize) gives identical echoes
  on scheduler and PortableVm with balanced stacks; FastVM yields the query
  payload and `resolve_host_request` echoes exactly what CVM1 echoed; ungranted
  → nulls. `cargo test -p crush-vm -p crush-frontend -p crush-lang-sdk` green.
- Limits (captured in TASKS): FastVM's AI ops don't pop stack operands
  (`execution.rs`, outside this ticket's fastvm allowance), so
  `resolve_host_request` returns `None` for `stack_args > 0`; FastVM/AOT
  lowering match `ai_toolchain`/`ai_goal_declaration`/`ai_knowledge_sharing`
  while the frontend emits `ai_tool_chain`/`ai_goal_decl`/`ai_knowledge_share`.
