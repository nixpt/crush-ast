# JS → crush AOT vs Node (CRUSH-261)

JavaScript compiled through crush — swc walker → CAST → CASM → AOT `.so` (rustc, gcc or clang),
run by `crush-aot-runner` — against the same file under `node`. Progress gate for CRUSH-257..260.

```sh
export CARGO_TARGET_DIR=$PWD/target                                 # repo root; one dir for both
cargo build --release -p crush-aot --bin crush-aot-runner
cargo build --release --manifest-path docs/benchmarks/js_vs_node/jsaot/Cargo.toml
cd docs/benchmarks/js_vs_node
python3 run.py check --target $CARGO_TARGET_DIR/release   # outputs must equal Node's
python3 run.py perf  --target $CARGO_TARGET_DIR/release
```

`check` compares stdout with Node on every backend; `perf` takes the median of 7 whole-process runs (wall
time, peak RSS) at a startup size and a work size, plus artifact size (`.so` + runner vs the `node` binary).

## Baseline — 2026-10-12, `main` @ c0a3fd7, Node v26.11.1, Linux x86_64

| program | n | Node | AOT rustc | AOT gcc | AOT clang |
|---|---|---|---|---|---|
| fib | 5 (startup) | 26.0 ms | 0.7 | 0.6 | 0.6 |
| fib | 30 | **24.9 ms** | 360.6 | 62.9 | 68.3 |
| loop | 10 (startup) | 18.0 ms | 0.6 | 0.7 | 0.5 |
| loop | 20,000,000 | **321 ms** | 3348 | 990 | 747 |
| sieve | 10 (startup) | 18.6 ms | 0.7 | 0.5 | 0.5 |
| sieve | 2,000,000 | **48 ms** | 1053 | 278 | 339 |

| | Node | AOT gcc/clang | AOT rustc |
|---|---|---|---|
| artifact | 52.5 MiB | **0.45 MiB** (runner 0.42 + `.so` 25-45 KiB) | 2.6 MiB |
| peak RSS, small | 44 MiB | **13 MiB** | 13 MiB |
| peak RSS, sieve 2M | 117 MiB | **33-36 MiB** | 49 MiB |

`strings.js` doesn't run yet: `.length` lowers to a field read (CRUSH-260).

**Reading:** AOT is ~115x smaller to ship, uses 3-4x less memory and starts ~30x faster — short-lived
programs win outright. Under sustained compute Node's JIT is 2.5-6x faster, because the AOT output is a
per-instruction `switch` over boxed values with unboxed locals switched off (CRUSH-257, CRUSH-258); the
rustc backend adds more (CRUSH-259).

## Results log

Append a row set here after each of CRUSH-257 / 258 / 259 / 260 lands.
