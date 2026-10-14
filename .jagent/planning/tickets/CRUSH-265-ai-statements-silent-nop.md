# CRUSH-265 — `capability_discovery` / `adaptation_request` / `semantic_switch` compile to a silent NOP on CVM1

| Field | Value |
|-------|-------|
| **ID** | CRUSH-265 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M9 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-157 (ai_native toolchain engine) |
| **Estimated effort** | S (decline → error) / M (port) |
| **Filed by** | claude — 2026-10-12 exosphere→crush-ast stdlib audit (v0.5.0 `c1186b8`) |

## Problem

The AI statements `capability_discovery`, `adaptation_request` and `semantic_switch` parse
(`crush-frontend/src/parser/mod.rs`, `AIStatement::{CapabilityDiscovery, AdaptationRequest}`),
compile to CASM ops (`crush-frontend/src/compiler.rs` ~1230–1250) and then lower to **`NOP`** on CVM1
(`crates/crush-lang-sdk/src/compile.rs:664`). The program compiles and runs; the statement does
nothing. `tests/casm_lowering_completeness.rs` lists them in `NOP_STUBS`, so the gap is known but
invisible to users.

exosphere registered `ai.adaptation_request` as a capability; crush-ast's `ai_native::KINDS` has
query, agent_delegation, goal_declaration, progress_update, knowledge_sharing — not these. Used by
`examples/crush/ai_agent_ops.crush` and `examples/cast/build_ai_ops_ast.py`.

## Decide

- **Port** onto `ai_native` (grant-gated like CRUSH-156/157), or
- **Decline** for now: make them a compile error (or a hard runtime error) naming the statement,
  instead of NOP, and record the decline in MIGRATION-INVENTORY §6.

## Success criteria

- [ ] No AI statement compiles to a silent NOP: each either runs through `ai_native` or fails loudly.
- [ ] `NOP_STUBS` updated accordingly; `ai_agent_ops.crush` behaviour matches the decision.
