# bash_walker

Bash to CRUSH AST (CAST) transpiler using Tree-sitter.

## Purpose

Transforms Bash shell scripts into CRUSH's universal Abstract Syntax Tree format, enabling shell scripts to run on the CRUSH VM alongside other languages.

## Supported Features

| Feature | Status | Notes |
|---------|--------|-------|
| Commands | ✅ | Basic command execution |
| Variables | ✅ | Variable assignment and expansion |
| If/Else | ✅ | Conditional statements |
| While Loops | ✅ | Basic while loops |
| For Loops | ✅ | For-in loops |
| Functions | ✅ | Function definitions |
| Pipes | ⚠️ | Limited support |
| Redirections | ⚠️ | Limited support |
| Command Substitution | ❌ | Not yet supported |
| Arrays | ❌ | Not yet supported |

See [LANGUAGE_READINESS.md](../../LANGUAGE_READINESS.md) for detailed status.

## Usage

```bash
# Compile Bash to CAST
cargo run --bin bash_walker script.sh > output.cast

# Or use via crush-cli
crush compile script.sh -o output.casm
```

## See Also

- [`walker-core`](../walker-core/README.md) - Base walker utilities
- [`crush-lang`](../../core/crush-lang/README.md) - CAST definitions
- [LANGUAGE_READINESS.md](../../LANGUAGE_READINESS.md) - Feature support matrix
