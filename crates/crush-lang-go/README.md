# go_walker

Go to CRUSH AST (CAST) transpiler using Tree-sitter.

## Purpose

Transforms Go source code into CRUSH's universal Abstract Syntax Tree format, enabling Go code to run on the CRUSH VM alongside other languages.

## Supported Features

| Feature | Status | Notes |
|---------|--------|-------|
| Functions | ✅ | Full support with parameters and return types |
| Variables | ✅ | Variable declarations and references |
| Literals | ✅ | integers, floats, strings, booleans |
| Binary Operators | ✅ | Arithmetic and comparison operators |
| If/Else | ✅ | Full conditional support |
| For Loops | ✅ | All for loop variants |
| Function Calls | ✅ | Including method calls |
| Return Statements | ✅ | Multiple return values |
| Structs | ⚠️ | Basic support |
| Interfaces | ❌ | Not yet supported |
| Goroutines | ❌ | Not yet supported |
| Channels | ❌ | Not yet supported |

## Usage

```bash
# Compile Go to CAST
cargo run --bin go_walker input.go > output.cast

# Or use via the CLI dispatcher
cargo run --bin walker input.go > output.cast
```

## See Also

- [`walker-core`](../walker-core/README.md) - Base walker utilities
- [`crush-cast`](../crush-cast/README.md) - CAST definitions
- [The Crush Language Guide](https://github.com/nixpt/crush-language-guide) - Full language documentation
