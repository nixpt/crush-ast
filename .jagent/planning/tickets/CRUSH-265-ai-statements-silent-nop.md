# CRUSH-265 — `capability_discovery` / `adaptation_request` / `semantic_switch` compile to a silent NOP on CVM1

| Field | Value |
|-------|-------|
| **ID** | CRUSH-265 |
| **Priority** | P2 |
| **Status** | Done (declined: loud error) |
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

- [x] No AI statement compiles to a silent NOP: each either runs through `ai_native` or fails loudly.
- [x] `NOP_STUBS` updated accordingly; `ai_agent_ops.crush` behaviour matches the decision.

## Resolution

**Decision (vega, for the owner): DECLINE for now.** Each of `capability_discovery`, `adaptation_request` and `semantic_switch` fails loudly at compile time instead of compiling to a silent NOP. Porting them onto `ai_native` is a later ticket.

**Finding on `semantic_switch`:** it is NOT fully implemented via `semantic_match`. The statement compiles to an `ai_semantic_switch` CASM opcode plus case/jmp blocks, but the `ai_semantic_switch` opcode itself lowers to NOP on CVM1, so dispatch never selects a case by the target — only the first case ever runs unconditionally. The `ai_semantic_match` / `ai_synthesize` calls inside case bodies are real (implemented) ops, but they're unreachable behind a NOP dispatch, so `semantic_switch` as a whole is silent and required the loud error.

**Where the loud error is raised:**
- `crates/crush-frontend/src/compiler.rs` — `compile_ai_statement` bails with
  `` "`capability_discovery` isn't supported yet (CRUSH-265): it would compile to a no-op"``,
  `` "`adaptation_request` isn't supported yet (CRUSH-265): it would compile to a no-op"``,
  `` "`semantic_switch` isn't supported yet (CRUSH-265): it would compile to a no-op"``.
  This catches every source-level use for every engine (scheduler VM, portable VM/crush-web, FastVM/JIT, AOT Rust/C, crush-aotc).
- `crates/crush-lang-sdk/src/compile.rs` — the `ai_*` NOP arms were removed, so hand-built CASM containing one of these ops hits the `Unsupported CVM1 opcode` arm.

**Per-engine behaviour (all refuse, none silent):**
- scheduler VM + PortableVm (crush-web): `casm_to_vm` bails with `Unsupported CVM1 opcode: <op>`.
- FastVM / JIT: `lower_program` → `lower_instruction` rejects `ai_capability_discovery` / `ai_adaptation_request` / `ai_semantic_switch` as `UnknownOpcode` (they're not in its AI match arms).
- AOT Rust (`crush-aot`): the AI opcode arm lists only the implemented ops, so these fall to the `_ => unsupported` arm and codegen fails.
- AOT C (`crush-aot` codegen_c): same, falls to `_ => unsupported`.
- crush-aotc: explicitly bails on the three ops (previously emitted a silent comment stub).

**Example handling:**
- `examples/crush/ai_agent_ops.crush` uses `semantic_switch`; it now fails to compile on purpose. Marked with `// expect-error: semantic_switch isn't supported yet (CRUSH-265): it would compile to a no-op` so the conformance corpus treats it as an intentional failure rather than a regression. `crates/crush-aot/tests/examples_parity.rs` skips it (compile failure → not compared) and stays green.
- `examples/cast/build_ai_ops_ast.py` builds a SemanticSwitch AST but only dumps JSON; it does not compile or run the statement, so no change required.

**Tests:**
- `crates/crush-frontend/tests/compiler_tests.rs`: `capability_discovery_fails_loudly_naming_statement`, `adaptation_request_fails_loudly_naming_statement`, `semantic_switch_fails_loudly_naming_statement` — each asserts the error names the statement and mentions CRUSH-265 / no-op.
- `crates/crush-lang-sdk/tests/casm_lowering_completeness.rs`: `declined_ai_ops_are_refused_by_every_engine` — hand-built CASM containing each op is refused by `casm_to_vm` (CVM1 engines) and `fastvm::lower_program` (FastVM/JIT). The three ops moved from `NOP_STUBS` to the `UNSUPPORTED` allowlist (with reasons), covered by `unsupported_ops_fail_loudly_with_the_op_name` and `every_opcode_is_lowered_or_explicitly_unsupported`.
- `crates/crush-frontend/tests/annotation_parse_tests.rs`: parsing of `semantic_switch` is unchanged, so `parse_ai_agent_ops_example_parses_cleanly` stays green.
