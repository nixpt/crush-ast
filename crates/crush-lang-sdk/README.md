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

The source-compilation APIs below are unreleased and are not in the published
0.6.0 SDK. Version 0.6.0 supports embedding Crush with `compile_crush_source`
as shown above.

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

Mapped Rust methods such as `.to_lowercase()` and `.pow()` use named standard
library capabilities. Embedding hosts enable those with
`Runtime::new().with_host_caps(HostCapsBuilder::new().stdlib(true).build())`
(requires the `stdlib` feature). `Runtime::new()` alone does not register the
SDK standard library or grant filesystem, process, or network access.

Rust source support is a subset with known semantic gaps. The
[conformance inventory](../../docs/rust-conformance.md) records VM/AOT parity,
unsupported constructs, and ticket-linked blockers before `.rs` CLI dispatch.

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

`compile::compile_crush_source` marshals variables across the Crush/guest
boundary for `@python { ... }` and `@javascript { ... }` blocks, via real
free-variable analysis over the guest AST (`crush_lang_python::analyzer::
free_variables`, `crush_lang_js::freevars::free_variables`) — not a regex or
a blind "inject everything in scope".

**Inputs are automatic.** Names the block *reads* but never binds are injected
as real guest locals (JSON-decoded, not raw env-var strings) from the
enclosing Crush scope.

**Outputs are explicit** (CRUSH-270). A block header lists the names marshaled
back into Crush:

```crush
let count = 10
@python -> total {
    tmp = 3
    total = tmp * 2
}
io.print(total)   // 6
io.print(count)   // 10 — untouched; only `total` crossed
@python -> (mean, stdev) { ... }   // several outputs
```

Each listed name becomes (or assigns) a Crush variable at that line, and
nothing else leaves the block. A listed name the block never binds at its top
level is a compile error; a name it only binds conditionally (inside
`try`/`if`) that misses at run time is a loud error naming the variable,
never `null`.

**Transition, one release.** A block *without* `->` keeps the old implicit
rule — its last top-level plain assignment is exported — but `crush-run` emits
a compile warning naming the exported variable and suggesting `-> <name>`
(the warning also fires when that implicit export overwrites an existing Crush
variable). In the release after, no header means no outputs.

Other `@<lang> { ... }` blocks still execute (via `EXEC_LANG`'s subprocess
shell-out), but with no input/output marshaling; `->` on such a language is a
loud compile error rather than a silent no-op. An unregistered language name
(no executor at all, e.g. `@rust`) fails loudly too: `"no executor registered
for language 'X'"`.

### Running polyglot blocks somewhere else

By default a granted block runs in the local interpreter (`python3 -c`,
`node -e`, `bash -c`). An embedder such as a browser or an OS can run it its
own way by supplying a `PolyglotExecutor` (CRUSH-271); both VM engines use it.
The VM still does the grant check first, so a block without its
`polyglot.<lang>` grant never reaches the executor.

```rust,ignore
use crush_lang_sdk::{HostCapsBuilder, PolyglotRequest, Runtime, polyglot_executor_fn};

let caps = HostCapsBuilder::new().polyglot(&["python"]).build();
let runtime = Runtime::new()
    .with_host_caps(caps)
    .with_polyglot_executor(polyglot_executor_fn(|req: &PolyglotRequest<'_>| {
        // Run `req.code` with `req.vars` in its environment, inside
        // `req.deadline_ms`; return its stdout (the result line included).
        my_sandbox::run(req.lang, req.code, req.vars).map_err(|e| e.to_string().into())
    }));
```

The request also carries the block's declared outputs and pinned deps
(`@python[pypi:six]`). Return `PolyglotError::Failed`, `Guest` (the guest code
failed; its stderr) or `Timeout`, and the VM raises the matching error. With
no executor set, the built-in `SubprocessExecutor` runs the block.

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
