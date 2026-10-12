# Python → crush AOT vs CPython (CRUSH-262)

Python compiled through crush — rustpython walker → CAST → CASM → AOT `.so` (rustc, gcc or clang),
run by `crush-aot-runner` — against the same file under `python3`. Twin of `docs/benchmarks/js_vs_node/`
(CRUSH-261). Progress gate for Python walker/AOT parity after CRUSH-200.

```sh
export CARGO_TARGET_DIR=$PWD/target                                 # repo root; one dir for both
cargo build --release -p crush-aot --bin crush-aot-runner
cargo build --release --manifest-path docs/benchmarks/py_vs_cpython/pyaot/Cargo.toml
cd docs/benchmarks/py_vs_cpython
python3 run.py check --target $CARGO_TARGET_DIR/release   # outputs must equal CPython's
python3 run.py perf  --target $CARGO_TARGET_DIR/release
```

`check` compares stdout with CPython on every backend; `perf` takes the median of 7 whole-process
runs (wall time, peak RSS) at a startup size and a work size, plus artifact size (`.so` + runner
vs the realpath'd `python3` binary).

## Baseline — 2026-10-12, `agent/nixp/CRUSH-200`, CPython 3.14.7, Linux x86_64

| program | n | CPython | AOT rustc | AOT gcc | AOT clang |
|---|---|---|---|---|---|
| fib | 5 (startup) | 9.5 ms | 1.4 | 1.0 | 5.5 |
| fib | 30 | 163 ms | 593 | **103** | 140 |
| loop | 10 (startup) | 11.6 ms | 0.7 | 0.8 | 0.6 |
| loop | 20,000,000 | 4493 ms | 6467 | 1582 | **1252** |
| sieve | 10 (startup) | 13.1 ms | 0.7 | 0.8 | 0.8 |
| sieve | 2,000,000 | 444 ms | 1519 | **302** | 403 |

| | CPython 3.14 | AOT gcc/clang | AOT rustc |
|---|---|---|---|
| artifact | ~6–20 MiB (interpreter; path-dependent) | **0.45 MiB** (runner 0.42 + `.so` 25–45 KiB) | 2.6 MiB |
| peak RSS, small | 17 MiB | 17 MiB | 17 MiB |
| peak RSS, sieve 2M | 25 MiB | 34–36 MiB | 50 MiB |

`check` is green on fib / loop / sieve / strings for rust + gcc + clang (stdout matches CPython).

**Reading:** Unlike the Node comparison (where V8's JIT won sustained compute), AOT **gcc/clang beat CPython** on fib(30), the 20M loop, and sieve(2M) here — CPython 3.14 on this box is a slower baseline than Node was for JS. Startup still favors AOT (~10×). The rustc backend stays the slow/large outlier (CRUSH-259). Shared AOT compute-gap work (CRUSH-257/258) should improve absolute numbers further.

## Results log

Append a row set here after material AOT / Python-walker changes land.
