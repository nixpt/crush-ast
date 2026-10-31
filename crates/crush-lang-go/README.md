# go_walker

Go to CRUSH AST (CAST) transpiler using Tree-sitter.

## Purpose

Transforms Go source code into CRUSH's universal Abstract Syntax Tree format, enabling Go code to run on the CRUSH VM alongside other languages.

## Supported Features

| Feature | Status | Notes |
|---------|--------|-------|
| Functions | ✅ | `func f(a, b) { … }`; parameters typed `Any` |
| Variables | ✅ | `var x = e` and `x := e` (and `var x T = e`) |
| Literals | ✅ | integers, floats, strings, booleans, `nil` |
| Binary Operators | ✅ | arithmetic and comparison operators |
| If/Else | ✅ | including `if init; cond { … }` |
| For Loops | ⚠️ | `for {}`, `for cond {}`, C-style `for i := 0; i < n; i++`, and `for x := range xs`; a two-variable range is rejected |
| Function Calls | ✅ | capability-mapped where known (`fmt.Println` → `io.print`) |
| Return Statements | ⚠️ | a single value; `return a, b` is rejected |
| Assignment / `++` / `--` | ✅ | `x = e`, `x++`, `x--` |
| `switch` | ❌ | rejected loudly (not silently dropped) |
| `defer` / `go` / `select` | ❌ | rejected loudly |
| Structs | ❌ | not lowered |
| Interfaces | ❌ | not lowered |
| Goroutines / Channels | ❌ | not lowered |

**No silent drops.** Every statement or expression the walker does not lower is
a loud error, never a `null` or a dropped statement (CRUSH-198).

**Note:** Go's `fmt.Println(a, b)` joins its arguments with spaces; the Crush
`io.print` it maps to concatenates them. `fmt.Println(a)` (one argument) matches
Go exactly; multi-argument spacing is a known divergence (a Go-aware print cap
is a follow-up).

## Usage

```bash
# Compile Go to CAST
cargo run --bin go_walker input.go > output.cast

# Or use via the CLI dispatcher
cargo run --bin walker input.go > output.cast
```

## See Also

- [`crush-walker-core`](../crush-walker-core/README.md) (formerly `walker-core`) - Base walker utilities
- [`crush-cast`](../crush-cast/README.md) - CAST definitions
- [The Crush Language Guide](https://github.com/nixpt/crush-language-guide) - Full language documentation
