# TODO

Tracked by joker. One-line items; complex work goes in `.jagent/planning/tickets/`.

## Priority: Make AI opcodes real

- [ ] Wire AI-native opcodes in crush-vm (Query, Synthesize, AgentDelegation, SemanticMatch, LearningLoop, ContextAware, ToolChain)
- [ ] Wire GoalDeclaration, ProgressUpdate, KnowledgeSharing to VM execution
- [ ] Wire DOM opcodes (dom_mutate, dom_event_listener, dom_query)
- [ ] Wire spawn/await/yield to VM execution

## Priority: JIT completion

- [ ] Phase 2: function calls, store/load, CapCall, CallHost
- [ ] Phase 3: MakeList, MakeMap, Index, Len, arena
- [ ] Phase 4: EnterTry, ExitTry, Throw
- [ ] Phase 5: ExoLight integration
- [ ] Phase 6: Optimization passes
- [ ] Phase 7: AOT compilation

## Priority: Debugger

- [ ] Variable inspection (print <var>)
- [ ] Source → bytecode sourcemap from crush-frontend
- [ ] Step-by-step state inspection

## Priority: Test coverage

- [ ] 18 zero-coverage error paths
- [ ] 6 uncovered opcodes (BITAND, BITOR, BITXOR, BITNOT, SHL, SHR)
- [ ] 5 uncovered capability functions

## Bugs

- [ ] MOD sign bug between portable_vm and FastVM
- [ ] EXEC_LANG missing from PortableVm
- [ ] Unreachable code in vm.rs:326

## Cross-project

- [ ] Migrate surfer's in-tree Crush runtime → crush-ast
- [ ] Reconcile exosphere's in-tree crush divergence
