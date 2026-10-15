# crush-lang-sdk

Rust SDK for hosting and extending the [crush-vm](https://crates.io/crates/crush-vm)
CVM1 runtime — the high-level entry point to the Crush toolchain.

## Purpose

crush-lang-sdk ties the pipeline together (parse → compile → run) behind an
ergonomic builder, and provides the host integration surface (capabilities, a
message bus) for embedding Crush in a Rust application. If you want to run Crush
programs from Rust, start here rather than wiring crush-frontend + crush-vm by hand.

```rust,no_run
use crush_lang_sdk::{Runtime, Value, compile::compile_crush_source};

let program = compile_crush_source("fn main() { print(\"hello\"); return 42; }")?;
let result = Runtime::new().run(&program)?;
assert_eq!(result.output, "hello\n");
assert_eq!(result.return_value, Some(Value::Int(42)));
# Ok::<(), Box<dyn std::error::Error>>(())
```

The runnable version is `cargo run -p crush-lang-sdk --example embed_crush`.
This is the supported Rust-host direction: Rust embeds the Crush VM and owns
the host capabilities granted to the program. `VmResult::output` captures
printed text and `VmResult::return_value` carries the entry function's explicit
`return` value.

The SDK can also compile source from enabled walker languages into the same VM
program type:

```rust,no_run
use crush_lang_sdk::{Runtime, compile::compile_source};

let program = compile_source("rust", r#"fn main() { println!("hello"); }"#)?;
let result = Runtime::new().run(&program)?;
assert_eq!(result.output, "hello\n");
# Ok::<(), Box<dyn std::error::Error>>(())
```

The selector may be a language name (`"rust"`) or extension (`"rs"` or
`".rs"`). Rust support is controlled by the `polyglot-rust` Cargo feature.
The default SDK feature set enables it; `default-features = false` omits the
`crush-lang-rust` walker dependency. Walker source goes through the shared
`AdapterRegistry`, Crush's compiler, and the same CVM1 assembly path. Crush
source continues to use `compile::compile_crush_source` so polyglot blocks and
declared-capability checks keep their existing preparation.

## Calling a manually built Rust wrapper

`crush-ffi` provides a stable C ABI for small wrapper functions. Build the
wrapper on the host, then explicitly load it in the embedding Rust host:

```sh
cargo build -p crush-plugin-example
cargo run -p crush-lang-sdk --example plugin_host -- target/debug/libcrush_plugin_example.so
```

The example plugin exports `rust.greet(String) -> String` and
`rust.version() -> Int`; the host allowlists only `rust.greet`. Crush cannot
select a library path or invoke Cargo. The host chooses the `.so` path and
grants the wrapper's exported capabilities through `HostCaps`. A `cdylib` is native code
with the host process's authority, so only load wrappers the host trusts. This
MVP demonstrates scalar string arguments and results; it does not import Rust
ABI symbols, generics, or arbitrary crate APIs directly.

## What's here

- **builder** — configure and drive a compile-and-run flow.
- **compile** — Crush source and enabled walker languages → CASM/CVM1 via the shared walker registry and [crush-frontend](https://crates.io/crates/crush-frontend).
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

`crush-run` and `crush-repl` register the standard library (`str.*`,
`math.*`, `conv.*`, `collections.*`, `json.*`, `path.*`, `regex.*`, …) by
default; pass `--no-stdlib` to leave it out. It comes from the `stdlib` cargo
feature, which is on by default — with `default-features = false` the stdlib
is absent and `--stdlib` is an error. Capabilities with real authority
(`--fs`, `--env`, `--time`, `--net`, …) are always opt-in.

## License

Licensed under either of MIT or Apache-2.0 at your option.
