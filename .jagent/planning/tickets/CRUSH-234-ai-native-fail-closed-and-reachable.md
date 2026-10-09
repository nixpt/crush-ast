# CRUSH-234 — AI-native calls fail closed, are reachable from source, and one real provider exists

| Field | Value |
|-------|-------|
| **ID** | CRUSH-234 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M5 |
| **Assignee** | unassigned |
| **Dependencies** | CRUSH-156, CRUSH-158 |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-09 use-case review (agents + polyglot), on `main` `2d71b96` |

## Problem

- Ungranted AI opcodes return `Null` and keep going (`crates/crush-vm/src/ai_args.rs:~72-74`,
  pinned by `tests/ai_native_args.rs`), a CRUSH-156 decision kept for pre-CRUSH-32
  compatibility. For agent code a silently-null model call is worse than an error.
- 9 of 10 `ai_native.*` caps are echo stubs (`crates/crush-lang-sdk/src/ai_native.rs`);
  `QueryProvider`/`DelegationBackend` have only test implementations; no model is ever called.
- `semantic_switch` lowers to `NOP` (`crates/crush-lang-sdk/src/compile.rs:~545`): the first
  case always runs and a value is left on the stack.
- query / synthesize / delegation are reachable only through CAST JSON, not `.crush` text;
  `crush-run` has no flag to grant `ai_native` at all.
- `examples/crush/ai_agent_ops.crush` has no `main` and dies with `stack underflow`; only
  its parsing is tested. `crates/crush-frontend/src/ai_runtime.rs` is dead code.

## Success criteria

- [ ] An ungranted AI call is a runtime error naming the capability (supersedes the
      CRUSH-156 null decision — record it).
- [ ] `semantic_switch` either works with a granted provider or is a compile error.
- [ ] One real `QueryProvider` (host-supplied, e.g. HTTP to a configured endpoint) behind
      an explicit grant; `crush-run` flag to enable it.
- [ ] `ai_agent_ops.crush` runs (or is removed); dead `ai_runtime.rs` removed.
