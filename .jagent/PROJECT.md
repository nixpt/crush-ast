# crush-ast

AI-native language ecosystem: compiler, multi-tier engines (CVM1 → FastVM → JIT, plus AOT C/Rust and PTX), debugger, GUI, 12+ language walkers, CAISON data format and agent-native tooling. Published to crates.io as `crush-lang-sdk` and friends.

## Identity

- **Repository:** crush-ast
- **Language:** Rust (edition 2024, rust-version 1.95.0)
- **Ecosystem:** Standalone. Used by crush-notebook (kernel), crush-lsp, crush-squeeze, arniko-crush, polydex, crush-visuals, the language guide and crushlang.org (crush-web).
- **Protocol:** CLI binaries + library crates. No MCP server (that's crush-notebook's domain).

**Working this backlog?** Read `.jagent/planning/RULES.md` first — verify-before-fix +
one worktree/branch per milestone + push at every phase boundary, not at the end.

## Workspace (44 members, 2026-10-31)

`cargo metadata --no-deps`: 43 crates under `crates/` plus `xtask`; every one but `crush-gui` is a
default member. The annotated crate list lives in the root `README.md` (Repository Structure) — kept
there only, so it has one place to go stale.

```
crush-ast/
├── crates/        # IR, grammar, walkers, frontend, engines, SDK + binaries, tools (see README.md)
├── xtask/         # CI audits: lint-dejavue, ticket-id (unique CRUSH-N IDs)
├── docs/          # design docs, CAST reference, planning (RELEASE-1.0-CRITERIA.md)
├── examples/      # 50+ .crush programs; examples/verified/ conformance corpus
├── scripts/       # bump-version.sh, publish-crates.py, release helpers
├── .dejavue/      # architectural memory (why)
└── .jagent/       # planning board (what/when): ROADMAP, MILESTONES, BACKLOG-INDEX, tickets/
```
