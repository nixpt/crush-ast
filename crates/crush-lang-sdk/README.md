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

## Polyglot blocks (`@python { ... }`)

`compile::compile_crush_source` marshals variables across the Crush/Python
boundary for `@python { ... }` blocks: names the block reads but never
binds are injected as real Python locals (JSON-decoded, not raw env-var
strings), and the last name it binds at its own top level is JSON-encoded
and marshaled back — via real free-variable analysis over the
`rustpython-parser` AST (`crush_lang_python::analyzer::free_variables`),
not a regex or a blind "inject everything in scope".

**Only Python has this.** `@javascript { ... }` and other `@<lang> { ... }`
blocks still execute (via `EXEC_LANG`'s subprocess shell-out), but with no
input/output marshaling — a block that reads or produces a Crush variable
under any language other than Python won't see it, silently, because
there is no parser wired up to run the same free-variable analysis for
those languages yet. An unregistered language name (no executor at all,
e.g. `@rust`) does fail loudly instead: `"no executor registered for
language 'X'"`.

## Binaries

- `crush-run` — compile and execute a Crush program.
- `crush-compile` — compile source to CASM.
- `crush-repl` — interactive read-eval-print loop.

## License

Licensed under either of MIT or Apache-2.0 at your option.
