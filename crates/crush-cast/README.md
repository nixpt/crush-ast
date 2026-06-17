# crush-cast

Crush Abstract Syntax Tree (CAST) — the stable intermediate representation for the Crush toolchain.

## Purpose

CAST is the language-neutral AST that every Crush frontend lowers to. Source in
any supported language (Crush, Python, Rust, …) is parsed into a `Program` of
`Function`s built from `Statement`s and `Expression`s, which the compiler then
lowers to CASM bytecode. CAST is the contract between the frontends and the
backend, so it is versioned and kept backward-compatible.

```
Source (Crush/Python/Rust/…)
       ↓  frontend / walker
   CAST (this crate)   ← stable IR
       ↓  crush-frontend
   CASM (bytecode)
       ↓  crush-vm
   execution
```

## What's here

- **AST types** — `Program`, `Function`, `Statement`, `Expression`, `CastType`.
- **Serialization** — `Format::{Json, Cbor}` via `pack`, with `CAST_VERSION`
  embedded so consumers can gate on the IR version.
- **Validation** — `validate_json` checks a serialized program against the CAST
  schema (`ValidationError` on mismatch).
- **AI metadata** — optional `ai_meta` carried on a `Program` (`ai` module).
- **Diffing** — structural diff of two programs (`diff` module).

## Example

```rust
use crush_cast::{Program, Format, pack};

fn roundtrip(program: &Program) -> Vec<u8> {
    // Serialize to CBOR (compact) or JSON (human-readable).
    pack::to_bytes(program, Format::Cbor).expect("pack")
}
```

## Optional features

- `ts-export` — derive `ts-rs` bindings and emit TypeScript types
  (`export-ts` binary).

The `export-py` binary emits the matching Python dataclasses (see
[`python/README.md`](python/README.md)).

## License

Licensed under either of MIT or Apache-2.0 at your option.
