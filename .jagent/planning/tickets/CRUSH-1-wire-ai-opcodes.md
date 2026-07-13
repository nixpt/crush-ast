# CRUSH-1 — Wire AI-native opcodes in crush-vm

| Field | Value |
|-------|-------|
| **ID** | CRUSH-1 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M1 |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | L |

## Problem

10 AI-native opcodes (`ai_query`, `ai_tool_chain`, `ai_agent_delegation`, `ai_learning_loop`, `ai_context_aware`, `ai_semantic_match`, `ai_synthesize`, `ai_goal_decl`, `ai_progress_update`, `ai_knowledge_share`) are parsed and stored in CAST AST nodes (`crush-cast/src/ai.rs`) but compile to NOP at runtime in both the FastVM CASM→assembly translator and the CVM1 PortableVm interpreter. This blocks crush-notebook's M2 AI-native cells (`@ai.synthesize`, `@ai.query`, `@ai.agent_delegate`).

The AI expression types (`AIExpression::Query`, `Synthesize`, `AgentDelegation`, `LearningLoop`, `SemanticMatch`, `ContextAware`, `ToolChain`) and AI statement types (`AIStatement::GoalDeclaration`, `ProgressUpdate`, `KnowledgeSharing`) have well-defined AST nodes with fields. What's missing is the runtime — the compiler emits NOP, the VM does nothing.

## Success criteria

- [ ] `@ai.synthesize("sort function", constraints=["O(n log n)"])` in a crush cell produces real output
- [ ] `ai_query` routes to a configurable HostCap (MCP tool call or LLM inference)
- [ ] `ai_agent_delegation` spawns a named agent task
- [ ] `ai_semantic_match` computes vector similarity via the existing FastVM similarity module
- [ ] `ai_goal_decl`, `ai_progress_update`, `ai_knowledge_share` produce annotation-equivalent effects at runtime
- [ ] All 10 AI opcodes no longer map to NOP in `casm_to_assembly`
- [ ] Tests cover each AI opcode's execution path

## Technical approach

1. **AI query/synthesize/tool_chain**: Route through FastVM's existing HostRequest pattern (`CallHost`). The VM yields `HostRequest::CallHost { capsule_name: "ai", method_name, args }`. The host (crush-notebook kernel or surfer runtime) processes the request and resumes the VM.
2. **AI semantic_match**: Leverage the existing `fastvm/similarity.rs` module (cosine similarity, already implemented). Route `AIExpression::SemanticMatch` through this path.
3. **AI statements** (goal/progress/knowledge): Post events to the codebase index (`crush-index`) or annotations system. Treat as structured log output.
4. **AI agent_delegation/learning_loop**: Defer to HostRequest pattern — the VM yields and the host dispatches to an actual agent system (joker-mcp or bro-cli).

## Files to modify

- `crates/crush-frontend/src/compiler.rs` — replace NOP emissions with real CASM instructions for AI opcodes
- `crates/crush-vm/src/portable_vm.rs` — handle new AI opcodes in string-match dispatch
- `crates/crush-vm/src/fastvm/instructions.rs` — add FastOp variants for AI instructions
- `crates/crush-vm/src/fastvm/execution.rs` — implement AI instruction execution
- `crates/crush-lang-sdk/src/compile.rs` — remove NOP mappings for AI opcodes
- `crates/crush-cast/src/ai.rs` — possibly add `to_host_request()` methods

## Non-goals

- Implementing actual LLM inference in crush-vm (that's the host's job)
- Full agent lifecycle management (future)
- AI model training or fine-tuning
- Real-time AI streaming in the VM
