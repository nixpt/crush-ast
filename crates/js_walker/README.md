# js_walker

JavaScript to CRUSH AST (CAST) transpiler using Tree-sitter.

## Purpose

Transforms JavaScript source code into CRUSH's universal Abstract Syntax Tree format, enabling JavaScript code to run on the CRUSH VM alongside other languages.

## Supported Features

| Feature | Status | Notes |
|---------|--------|-------|
| Functions | ✅ | Full support including arrow functions |
| Variables | ✅ | var, let, const |
| Literals | ✅ | numbers, strings, booleans, null |
| Binary Operators | ✅ | Arithmetic and comparison operators |
| If/Else | ✅ | Full conditional support |
| While Loops | ✅ | while and do-while |
| For Loops | ✅ | Traditional and for-of loops |
| Function Calls | ✅ | Including method calls |
| Return Statements | ✅ | With optional values |
| Objects | ⚠️ | Basic support |
| Arrays | ⚠️ | Basic support |
| Classes | ❌ | Not yet supported |
| Async/Await | ❌ | Not yet supported |
| Promises | ❌ | Not yet supported |

See [LANGUAGE_READINESS.md](../../LANGUAGE_READINESS.md) for detailed status.

## Usage

```bash
# Compile JavaScript to CAST
cargo run --bin js_walker input.js > output.cast

# Or use via crush-cli
crush compile input.js -o output.casm
```

## See Also

- [`walker-core`](../walker-core/README.md) - Base walker utilities
- [`crush-lang`](../../core/crush-lang/README.md) - CAST definitions
- [LANGUAGE_READINESS.md](../../LANGUAGE_READINESS.md) - Feature support matrix
