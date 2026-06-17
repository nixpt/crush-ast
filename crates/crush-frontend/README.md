# crush-frontend

The portable Crush language frontend: parser, semantic analyzer, optimizer, and
CASM compiler.

## Purpose

crush-frontend turns Crush source into the [CAST](https://crates.io/crates/crush-cast)
intermediate representation and lowers it to [CASM](https://crates.io/crates/casm)
bytecode for [crush-vm](https://crates.io/crates/crush-vm). It is the middle of
the pipeline:

```
Crush source → crush-frontend (parse → analyze → optimize → compile) → CASM
```

## What's here

- **Parser** — `parse_source` produces a `crush_cast::Program`; `parser::Parser`
  for finer control (collects all errors, not just the first).
- **Semantics & optimizer** — `semantics` analysis and an `optimizer` pass over
  the CAST.
- **Compiler** — lowers CAST to CASM bytecode.
- **Polyglot** — `language_walkers` / `polyglot_imports` drive the subprocess
  walker seam (other-language source → CAST), and `import_system` resolves imports.
- **AI runtime** — `ai_runtime` support for CAST `ai_meta`.

## Example

```rust
let program = crush_frontend::parse_source(src)?; // -> crush_cast::Program
```

## License

Licensed under either of MIT or Apache-2.0 at your option.
