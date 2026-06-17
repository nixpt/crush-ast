# crush-lang-sdk

Rust SDK for hosting and extending the [crush-vm](https://crates.io/crates/crush-vm)
CVM1 runtime — the high-level entry point to the Crush toolchain.

## Purpose

crush-lang-sdk ties the pipeline together (parse → compile → run) behind an
ergonomic builder, and provides the host integration surface (capabilities, a
message bus) for embedding Crush in a Rust application. If you want to run Crush
programs from Rust, start here rather than wiring crush-frontend + crush-vm by hand.

## What's here

- **builder** — configure and drive a compile-and-run flow.
- **compile** — source → CASM via [crush-frontend](https://crates.io/crates/crush-frontend).
- **caps** — declare the host capabilities exposed to running programs.
- **bus** — message bus for host ↔ program communication.
- **akg** — application knowledge graph integration.

## Binaries

- `crush-run` — compile and execute a Crush program.
- `crush-compile` — compile source to CASM.
- `crush-repl` — interactive read-eval-print loop.

## License

Licensed under either of MIT or Apache-2.0 at your option.
