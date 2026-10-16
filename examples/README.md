# Examples

## Crush Language (`examples/crush/`)

Source-level `.crush` programs. Drawn from the exosphere test suite, the
super-surfer web apps, the crush-pipefish-dashboard demo, and LLM-written
programs collected in [nixpt/awesome-crush](https://github.com/nixpt/awesome-crush).
Some are historical fixtures that predate today's language and carry an
`// expect-error:` marker saying how they fail (see below). For one small,
current program per language feature, with its exact output, see the Crush
Language Guide's [`examples/verified/`](https://github.com/nixpt/crush-language-guide/tree/main/examples/verified).

| File | Source | Description |
|------|--------|-------------|
| `fibonacci.crush` | exosphere language fixtures | Recursive fibonacci |
| `control_flow.crush` | exosphere language fixtures | If/else, while, for |
| `arithmetic.crush` | exosphere language fixtures | Arithmetic operators |
| `function_call.crush` | exosphere language fixtures | Function definitions + calls |
| `modulo.crush` | exosphere language fixtures | Modulo operator |
| `strings.crush` | exosphere language fixtures | String operations |
| `arrays_and_loops.crush` | exosphere language tests | Arrays and loop patterns |
| `concurrency_structs.crush` | exosphere language tests | Spawn, yield, structs (old syntax: `expect-error: Unknown struct: Point`) |
| `exception_test.crush` | exosphere language tests | Try/catch/throw |
| `async_test.crush` | exosphere language tests | Async/await |
| `polyglot_braces.crush` | exosphere language tests | Polyglot embedding |
| `snake.crush` | ported from `crush-capsules/games/snake` | Self-playing Snake — recursion + fixed-arity argument state (no arrays: no index-assignment, `.push()` can't chain); verified end-to-end via `crushc`/`crush-run` |
| `lang_test.crush` | exosphere language tests | Various language features |
| `build_pipeline.crush` | exosphere examples | Multi-function pipeline |
| `dashboard.crush` | crush-pipefish-dashboard | Real web dashboard app |
| `sysinfo.crush` | super-surfer app | System info HTML dashboard |
| `calculator.crush` | super-surfer app | Web calculator |
| `greeting.crush` | super-surfer app | Greeting app |
| — 11 more | super-surfer / stdlib / coreutils | — |

## JS, walked (`examples/js-walked/`)

Real JavaScript source, walked through `crush-lang-js` (`js_walker`/swc
backend) into CAST, then compiled and run the same way native `.crush`
source is — a different pipeline from `examples/crush/polyglot_braces.crush`
(which embeds JS via `@javascript{}` blocks that spawn a `node` subprocess;
this directory's JS instead becomes CAST/CASM directly, no subprocess).

| File | Source | Description |
|------|--------|-------------|
| `turtle_runner.js` | ported from `crush-capsules/games/turtle-runner` | Self-playing Chrome-Dino-style runner. `crush-walk-run examples/js-walked/turtle_runner.js` runs it end-to-end (walk → CAST → compile → CVM1 → interpret). See the file's header comment for the (large) set of confirmed-broken JS constructs it avoids — filed as `CRUSH-9`/`CRUSH-10`/`CRUSH-11`. |

## CAST (`examples/cast/`)

JSON-format CAST programs (Abstract Syntax Tree level). There is no CLI for CAST
JSON; use the Rust API: `crush_cast::validate_json`, then
`crush_frontend::compile_cast_owned` and `crush_lang_sdk::compile::casm_to_vm`.
Checked against `3ae0b4a`, every file validates, and all but three compile and
lower: `function-def.cast.json` (`Undefined function: double`, a nested
`FunctionDef`), and `ai-orchestration.cast.json` and
`cookbook/research-summarizer/main.cast.json`, which use `CapabilityDiscovery`
(refused since CRUSH-265: it would compile to a no-op).

See `cookbook.md` in `docs/cast/` for the full index.

## Compile expectations (`// expect-error:`)

Every file under `examples/` is compiled by `crates/crush-lang-sdk/tests/examples_compile.rs`
as part of `cargo test`. Two rules apply:

- An **unmarked** file must compile cleanly. A compile failure there is a bug and
  fails the test.
- A file may carry a `// expect-error: <substring>` marker on its own line within
  the first ~20 lines. It must then *fail* with an error containing `<substring>`
  (either at compile time or at run time). If it compiles and runs cleanly, the
  marker is stale and must be removed — never delete an example to silence the
  test.

The marker exists so examples that intentionally can't compile (or can't run)
still exercise the test suite instead of being silently skipped — the gap found
in CRUSH-226/#226, where `crates/crush-aot/tests/examples_parity.rs` skipped any
example that failed to compile.
