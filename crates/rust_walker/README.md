# rust_walker

Rust to CRUSH AST (CAST) transpiler using Tree-sitter.

## Purpose

Transforms Rust source code into CRUSH's universal Abstract Syntax Tree format, enabling Rust code to run on the CRUSH VM alongside other languages.

## Supported Features

| Feature | Status | Notes |
|---------|--------|-------|
| Functions | ✅ | Full support with parameters and return types |
| Variables | ✅ | let bindings and references |
| Literals | ✅ | integers, floats, strings, booleans |
| Binary Operators | ✅ | Arithmetic and comparison operators |
| If/Else | ✅ | Full conditional support |
| While Loops | ✅ | Basic while loops |
| For Loops | ⚠️ | Limited support |
| Function Calls | ✅ | Including method calls |
| Return Statements | ✅ | With optional values |
| Structs | ⚠️ | Basic support |
| Enums | ❌ | Not yet supported |
| Traits | ❌ | Not yet supported |
| Macros | ❌ | Not yet supported |

## Usage

```bash
# Compile Rust to CAST
cargo run --bin rust_walker input.rs > output.cast

# Or use via the CLI dispatcher
cargo run --bin walker input.rs > output.cast
```

## See Also

- [`walker-core`](../walker-core/README.md) - Base walker utilities
- [`crush-cast`](../crush-cast/README.md) - CAST definitions
- [The Crush Language Guide](https://github.com/nixpt/crush-language-guide) - Full language documentation
