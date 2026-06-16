# c_walker

C to CRUSH AST (CAST) transpiler using Tree-sitter.

## Purpose

Transforms C source code into CRUSH's universal Abstract Syntax Tree format, enabling C code to run on the CRUSH VM alongside other languages.

## Supported Features

| Feature | Status | Notes |
|---------|--------|-------|
| Functions | ✅ | Full support with parameters and return types |
| Variables | ✅ | Variable declarations and references |
| Literals | ✅ | integers, floats, strings, characters |
| Binary Operators | ✅ | Arithmetic and comparison operators |
| If/Else | ✅ | Full conditional support |
| While Loops | ✅ | Basic while loops |
| For Loops | ✅ | Traditional for loops |
| Function Calls | ✅ | Including standard library |
| Return Statements | ✅ | With optional values |
| Pointers | ⚠️ | Limited support |
| Structs | ⚠️ | Basic support |
| Preprocessor | ❌ | Not yet supported |
| Macros | ❌ | Not yet supported |

## Usage

```bash
# Compile C to CAST
cargo run --bin c_walker input.c > output.cast

# Or use via the CLI dispatcher
cargo run --bin walker input.c > output.cast
```

## See Also

- [`walker-core`](../walker-core/README.md) - Base walker utilities
- [`crush-cast`](../crush-cast/README.md) - CAST definitions
- [The Crush Language Guide](https://github.com/nixpt/crush-language-guide) - Full language documentation
