# crush-errors

Unified error types for the CRUSH runtime.

## Purpose

Provides a consistent error handling system across all Crush crates. This is the
lowest-level crate with no internal Crush dependencies.

## Exports

```rust
use crush_errors::{CrushError, ErrorKind, CrushResult, ErrorContext};

fn example() -> CrushResult<()> {
    some_operation()
        .context("Failed to perform operation")?;
    Ok(())
}
```

## Types

- `CrushError` — main error type with `kind`, `message`, and optional `source`
- `ErrorKind` — categorized error enum (see below)
- `CrushResult<T>` — `Result<T, CrushError>` alias
- `ErrorContext` — extension trait on `Result` for adding a context message

## Error Kinds

| Kind | Description |
|------|-------------|
| `PermissionDenied` | Access or capability denied |
| `NotFound` | Resource, variable, or function not found |
| `InvalidArgument` | Bad argument value, parse error, or bad opcode |
| `TypeMismatch` | Wrong type at a binary or VM operation |
| `CapabilityViolation` | Missing or unknown capability |
| `ResourceExhausted` | Gas/queue/memory limit exceeded |
| `Unsupported` | Unsupported platform operation or version |
| `Io` | File or network I/O error |
| `Internal` | Internal invariant violation, arena error |
| `Cancelled` | VM or operation cancelled |
| `Timeout` | Watchdog or timeout triggered |
| `AlreadyExists` | Duplicate resource |

## Conversions

`From` impls are provided for common error types so `?` works seamlessly:

- `std::io::Error` (maps `NotFound`, `PermissionDenied`, `InvalidInput`, `TimedOut`)
- `std::string::FromUtf8Error`
- `regex::Error`
- `std::num::ParseIntError`
- `crush_errors::vm::RuntimeError` / `VmError` / `SchedulerError` / `CbvError` / `BinaryError`
- `crush_errors::hal::HalError` / `HostDispatchError`
- `crush_errors::exo::HostError` / `LoaderError` / `IpcError` / `CryptoError`
- `crush_errors::stdlib::StdlibError`
- `crush_errors::casm::CasmError`
- `crush_errors::version::VersionMismatch`

## Version Boundaries

The `version` module provides a unified `VersionMismatch` shape for the six
load-time version gates (`ipc`, `manifest`, `casm`, `cap_schema`, `service`,
`cast`). Each gate keeps its own typed error and maps onto `VersionMismatch`
for uniform audit-log rendering.

## Dependencies

- `thiserror` — derive `Error` for `ErrorKind`
- `regex` — `From<regex::Error>` conversion
- `serde` — `Serialize`/`Deserialize` for `VersionMismatch` and `VersionBoundary`

## Used By

Every crate in the Crush ecosystem depends on this for error handling.
