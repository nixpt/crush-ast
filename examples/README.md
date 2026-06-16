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
| `lang_test.crush` | exosphere language tests | Various language features |
| `build_pipeline.crush` | exosphere examples | Multi-function pipeline |
| `dashboard.crush` | crush-pipefish-dashboard | Real web dashboard app |
| `sysinfo.crush` | super-surfer app | System info HTML dashboard |
| `calculator.crush` | super-surfer app | Web calculator |
| `greeting.crush` | super-surfer app | Greeting app |
| — 11 more | super-surfer / stdlib / coreutils | — |

## CAST (`examples/cast/`)

JSON-format CAST programs (Abstract Syntax Tree level). Use
`crush_cast::validate_json` or `crush compile --from-cast` to process.

See `cookbook.md` in `docs/cast/` for the full index.
