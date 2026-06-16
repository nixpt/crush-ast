# python_walker

Python to CRUSH AST (CAST) transpiler using Tree-sitter.

## Purpose

Transforms Python source code into CRUSH's universal Abstract Syntax Tree format, enabling Python code to run on the CRUSH VM alongside other languages.

## Supported Features

| Feature | Status | Notes |
|---------|--------|-------|
| Functions | ✅ | Full support with parameters |
| Variables | ✅ | Assignment and references |
| Literals | ✅ | int, float, str, bool, None |
| Binary Operators | ✅ | +, -, *, /, //, %, ** |
| Comparisons | ✅ | ==, !=, <, >, <=, >= |
| If/Else | ✅ | Full conditional support |
| While Loops | ✅ | Basic while loops |
| For Loops | ⚠️ | Limited support |
| Function Calls | ✅ | Including capability calls |
| Return Statements | ✅ | With optional values |
| Imports | ⚠️ | Collected but not fully integrated |
| Classes | ❌ | Not yet supported |
| List Comprehensions | ❌ | Not yet supported |
| Decorators | ❌ | Not yet supported |

## Usage

```bash
# Compile Python to CAST
cargo run --bin python_walker input.py > output.cast

# Or use via the CLI dispatcher
cargo run --bin walker input.py > output.cast
```

## Example Transformation

**Input (Python):**
```python
def fibonacci(n):
    if n <= 1:
        return n
    return fibonacci(n - 1) + fibonacci(n - 2)

result = fibonacci(10)
print(result)
```

**Output (CAST):**
```json
{
  "version": "0.2",
  "entry": "main",
  "lang": "python",
  "functions": {
    "fibonacci": {
      "params": ["n"],
      "body": [
        {
          "If": {
            "condition": { "BinaryOp": { "operator": "<=", ... } },
            "then_body": [ { "Return": { "value": { "Var": { "name": "n" } } } } ],
            ...
          }
        }
      ]
    },
    "main": {
      "params": [],
      "body": [
        { "VarDecl": { "name": "result", "value": { "Call": ... } } },
        { "ExprStmt": { "expr": { "CapabilityCall": { "name": "io.print", ... } } } }
      ]
    }
  }
}
```

## Capability Mapping

Python built-ins are mapped to CRUSH capabilities:

| Python | CRUSH Capability |
|--------|------------------|
| `print()` | `io.print` |
| `input()` | `io.read` (planned) |
| `open()` | `io.open` (planned) |

## Implementation Details

- **Parser**: Uses `tree-sitter-python` for robust parsing
- **Walker**: Implements `walker_core::Walker` trait
- **Metadata**: Preserves line/column information for error reporting
- **Main Function**: Top-level code wrapped in `main()` function

## Development

```bash
# Build
cargo build

# Test with example
cargo run -- examples/fibonacci.py

# Run tests
cargo test
```

## See Also

- [`walker-core`](../walker-core/README.md) - Base walker utilities
- [`crush-cast`](../crush-cast/README.md) - CAST definitions
- [The Crush Language Guide](https://github.com/nixpt/crush-language-guide) - Full language documentation
