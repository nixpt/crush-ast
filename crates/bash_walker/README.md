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

## Usage

```bash
# Compile Bash to CAST
cargo run --bin bash_walker script.sh > output.cast

# Or use via the CLI dispatcher
cargo run --bin walker script.sh > output.cast
```

## See Also

- [`walker-core`](../walker-core/README.md) - Base walker utilities
- [`crush-cast`](../crush-cast/README.md) - CAST definitions
- [The Crush Language Guide](https://github.com/nixpt/crush-language-guide) - Full language documentation
