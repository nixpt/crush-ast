# Examples

## Crush Language (`examples/crush/`)

Source-level `.crush` programs. Drawn from the exosphere test suite, the
super-surfer web apps, and the crush-pipefish-dashboard demo.

| File | Source | Description |
|------|--------|-------------|
| `fibonacci.crush` | exosphere language fixtures | Recursive fibonacci |
| `control_flow.crush` | exosphere language fixtures | If/else, while, for |
| `arithmetic.crush` | exosphere language fixtures | Arithmetic operators |
| `function_call.crush` | exosphere language fixtures | Function definitions + calls |
| `modulo.crush` | exosphere language fixtures | Modulo operator |
| `strings.crush` | exosphere language fixtures | String operations |
| `arrays_and_loops.crush` | exosphere language tests | Arrays and loop patterns |
| `concurrency_structs.crush` | exosphere language tests | Spawn, yield, structs |
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
| `turtle_runner.js` | ported from `crush-capsules/games/turtle-runner` | Self-playing Chrome-Dino-style runner. `crush-walk-run examples/js-walked/turtle_runner.js` runs it end-to-end (walk → CAST → compile → CVM1 → interpret). See the file's header comment for the (large) set of confirmed-broken JS constructs it avoids — filed as `CRUSH-4`/`CRUSH-5`/`CRUSH-6`. |

## CAST (`examples/cast/`)

JSON-format CAST programs (Abstract Syntax Tree level). Use
`crush_cast::validate_json` to process — NOTE: `docs/cast/cookbook.md`
documents a `crush compile --from-cast` command that does not exist in the
current CLI surface (no such flag on `crushc`, no such subcommand found
anywhere); that doc predates the current tool names and is stale, found
while cross-checking CAST tooling for `examples/js-walked/`. Not filed as
a ticket, just noting it here since it's directly adjacent to what this
section is about.

See `cookbook.md` in `docs/cast/` for the full index.
