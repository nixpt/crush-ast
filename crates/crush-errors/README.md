# crush-errors

Unified error types for the CRUSH runtime.

## Purpose

Provides a consistent error handling system across all Crush crates. This is the lowest-level crate with no internal dependencies.

## Exports

```rust
use crush_errors::{CrushError, ErrorKind, CrushResult, ResultExt};

fn example() -> CrushResult<()> {
    some_operation()
        .context("Failed to perform operation")?;
    Ok(())
}
```

## Types

- `CrushError` - Main error type with context chain
- `ErrorKind` - Categorized error variants (Parse, Runtime, IO, etc.)
- `CrushResult<T>` - `Result<T, CrushError>` alias
- `ResultExt` - Extension trait for adding context to errors

## Error Kinds

| Kind | Description |
|------|-------------|
| `Parse` | Syntax/parsing errors |
| `Runtime` | Execution errors |
| `IO` | File/network errors |
| `Type` | Type mismatch errors |
| `Capability` | Permission errors |

## Design

Uses `thiserror` for ergonomic error definitions with support for:
- Error context chains
- Regex-based error pattern matching
- Conversion from standard error types

## Dependencies

None (leaf crate)

## Used By

Every crate in the Crush ecosystem depends on this for error handling.
