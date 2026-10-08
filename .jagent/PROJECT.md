# crush-ast

AI-native language ecosystem. Compiler, multi-tier VM (CVM1→FastVM→JIT), debugger, 9+ language walkers, CSON data format, and agent-native tooling.

## Identity

- **Repository:** crush-ast
- **Language:** Rust (edition 2024, rust-version 1.95.0)
- **Ecosystem:** Part of the Exosphere project family. Powers surfer-browser scripting, crush-notebook cells, crush-pkg ecosystem.
- **Protocol:** CLI binaries + library crates. No MCP server (that's crush-notebook's domain).

**Working this backlog?** Read `.jagent/planning/RULES.md` first — verify-before-fix +
one worktree/branch per milestone + push at every phase boundary, not at the end.

## Workspace (35 crates)

```
crush-ast/
├── crates/
│   ├── casm/                  # CASM bytecode format
│   ├── crush-cast/            # CAST AST (serializable, ts-export)
│   ├── crush-caison/          # CAISON (formerly CSON) semantic data format
│   ├── crush-cson-shim/       # retired crush-cson name (re-exports crush-caison)
│   ├── crush-errors/          # Error types
│   ├── tree-sitter-crush/     # Tree-sitter grammar
│   ├── walker-core/           # Walker trait framework
│   ├── crush-frontend/        # Parser + semantics + optimizer + compiler
│   ├── crush-vm/              # CVM1 PortableVm + FastVM (interpreter + lowered)
│   ├── crush-jit/             # Cranelift JIT (Phase 1 of 7)
│   ├── crush-lang-sdk/        # SDK: crushc, crush-run, crush-repl, HostCaps, compile
│   ├── crush-pkg/             # Package manager
│   ├── crush-installer/       # Toolchain installer
│   ├── crush-debugger/        # Interactive debugger
│   ├── crush-index/           # Codebase index/query
│   ├── crush-lint/            # Linter
│   ├── crush-net/             # TCP networking
│   ├── crush-python/          # Python bindings
│   ├── crush-ffi/             # FFI gateway
│   ├── crush-diagnostics/     # Diagnostic types
│   ├── crush-plugin-example/  # Plugin example
│   ├── cli/                   # Walker CLI dispatcher
│   └── 12 walker crates       # Rust, Python, JS/TS, Bash, Zsh, C/C++, Go, Zig, Wasm, custom
├── xtask/                     # CI audit + lint-dejavue
├── docs/                      # Design docs, CAST reference, AI-native specs
├── examples/                  # Language examples (.crush exercises)
├── .dejavue/                  # Architectural memory
└── .jagent/                   # Planning board
```
