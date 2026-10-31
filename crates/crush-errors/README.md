# crush-errors

Shared error types for the Crush toolchain: one error type with a kind, conversions from the std
errors the toolchain hits, the CASM error type, and one shape for format-version mismatches.

It is the leaf crate of the workspace (no Crush dependencies). Used by `casm`, `crush-cast`,
`crush-frontend`, `crush-vm`, `crush-lang-sdk` and `crush-ptx`.

```rust
use crush_errors::{CrushError, CrushResult, ErrorContext, ErrorKind};

fn load(path: &str) -> CrushResult<String> {
    let text = std::fs::read_to_string(path).context("reading the program")?;
    if text.is_empty() {
        return Err(CrushError::new(ErrorKind::InvalidArgument, "empty program"));
    }
    Ok(text)
}
```

## Types

- `CrushError` — `kind`, `message`, optional `source` (`CrushError::new`, `with_source`).
- `ErrorKind` — the category (table below).
- `CrushResult<T>` — `Result<T, CrushError>`.
- `ErrorContext` — `.context("…")` on a `Result` to prefix a message.
- `convert::casm::CasmError` — errors from reading and validating CASM (re-exported by `casm`).
- `VersionMismatch` / `VersionBoundary` — a format-version failure and where it happened.

## Error kinds

| Kind | Used for |
|------|----------|
| `PermissionDenied` | access or capability denied |
| `NotFound` | resource, variable or function not found |
| `InvalidArgument` | bad argument, parse error, bad opcode, version mismatch |
| `TypeMismatch` | wrong type in an operation |
| `CapabilityViolation` | missing or unknown capability |
| `ResourceExhausted` | step, stack, queue or memory limit |
| `Unsupported` | unsupported operation or version |
| `Io` | file or network I/O |
| `Internal` | internal invariant violated |
| `Cancelled` | run cancelled |
| `Timeout` | time limit hit |
| `AlreadyExists` | duplicate resource |

## Conversions

`From` impls so `?` works: `std::io::Error` (`NotFound`, `PermissionDenied`, `InvalidInput` and
`TimedOut` map to their kinds, the rest to `Io`), `std::string::FromUtf8Error`, `regex::Error`,
`std::num::ParseIntError`, `CasmError` and `VersionMismatch`.

## Version boundaries

Two load-time gates check a format version and report a `VersionMismatch`:

| `VersionBoundary` | Gate |
|---|---|
| `Casm` | `casm::Program` deserialize (`CASM_VERSION` major) |
| `Cast` | `crush_cast::pack` unpack (CAST pack format) |

`VersionMismatch` is `Serialize`, with the boundary as a lowercase string (`"casm"`, `"cast"`).
`VersionBoundary` is `#[non_exhaustive]`, so a new gate is not a breaking change.
