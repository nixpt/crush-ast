# Contributing to Crush

## Development

```bash
# Build the workspace
cargo build

# Run all tests
cargo test --workspace --exclude crush-cast --exclude walker-core

# Build and run the installer end-to-end
cargo build -p crush-lang-sdk -p crush-pkg -p crush-installer
cargo run --bin crush-installer -- install --bin-dir target/debug
```

> `crush-cast` and `walker-core` have pre-existing issues (missing test
> fixtures and doc-test crate references respectively) unrelated to this
> workspace.

## Coding Conventions

- **No comments** in source code — the code should be self-documenting.
  Use meaningful identifier names and expressive types.
- **Keep the dependency DAG acyclic**: `crush-frontend` → `crush-cast` →
  `casm` → `crush-errors`. Never add a back-edge.
- **Capability-based security**: All VM-external operations (I/O, network,
  filesystem) require explicit capability declarations.
- **Cross-platform**: Linux and macOS are first-class targets. Windows paths
  are handled where trivial. Avoid platform-specific assumptions.

## How to Add a New Language Walker

1. Create a new crate `crates/<lang>_walker/` depending on `walker-core` and
   the appropriate `tree-sitter-<lang>` grammar.
2. Implement `walker_core::Walker` for your walker struct.
3. Use `BaseWalker` for common operations (string extraction, literal parsing,
   metadata creation).
4. Map the language's AST constructs to CAST nodes defined in `crush-cast`.
5. Add tests with example source files.
6. Register the walker in `crates/cli/` for auto-detection.

## Commit Messages

Follow conventional commits: `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`,
`test:`. Reference the task ID where applicable.

## Questions

Open an issue on [GitHub](https://github.com/nixpt/crush-ast/issues) or reach
the team via the [exosphere](https://github.com/nixpt/exosphere) project.
