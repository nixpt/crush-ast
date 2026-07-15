# Walker Pipeline Benchmarks

Performance of the Crush language walkers (Python → CAST, JavaScript → CAST)
compared against native interpreter execution, including the full pipeline
through CASM compilation and FastVM execution.

## Harness

`harness.py` in the repo root orchestrates the benchmark:

```bash
cd crush-ast
python3 harness.py --runs 20 --warmup 3
```

First run builds the walker binaries (one-time, ~2 minutes). Subsequent runs
use `--skip-build` for instant re-measurement.

## Results

Run on `crush-ast` workspace with Python 3.14, Node 26.2. 20 runs + 3 warmup.

### Simple arithmetic (3 operations)

| Stage | Python | JavaScript |
|-------|--------|------------|
| Native interpreter | 37.2 ms | 52.1 ms |
| **Walker only** (src → CAST) | **1.7 ms** | **1.3 ms** |
| **Full pipeline** (walk → CAST → CASM → FastVM) | **4.3 ms** | **4.2 ms** |

### Compute chain (11 operations)

| Stage | Python | JavaScript |
|-------|--------|------------|
| Native interpreter | 39.5 ms | 35.2 ms |
| **Walker only** (src → CAST) | **4.7 ms** | **2.9 ms** |
| **Full pipeline** (walk → CAST → CASM → FastVM) | **6.5 ms** | **4.9 ms** |

## Analysis

### 1. Walker speed is dominated by parser initialization

The walker binaries show first-run costs of 2-8 ms for the parser library
initialization (rustpython-parser for Python, SWC for JavaScript). Subsequent
runs are faster (sub-millisecond for simple files) due to OS-level caching of
the binary and shared libraries.

### 2. CAST lowering adds marginal cost

Comparing "Walker only" vs "Full pipeline": the CAST → CASM compilation and
FastVM execution adds only 2-3 ms on top of walking. The compile step is
consistently sub-millisecond for programs under ~50 instructions.

### 3. Native interpreter comparison is unfair at this scale

Python3 and Node.js show 35-57 ms per run because each measurement spawns a
new process. This measures cold-start overhead, not steady-state throughput.
For a fair comparison, the native interpreter must run the same workload
in-process (e.g., via embedded CPython or V8 isolates).

### 4. JS walker (SWC) outperforms Python walker (rustpython-parser)

JavaScript parsing via SWC is consistently faster than Python parsing via
rustpython-parser:
- Simple: 1.3 ms (JS) vs 1.7 ms (Python)
- Compute: 2.9 ms (JS) vs 4.7 ms (Python)

This reflects SWC's design as a high-performance Rust-native JS/TS parser vs
rustpython-parser's focus on Python AST fidelity over raw speed.

## Pipeline Architecture

```
┌──────────────────────────────────────────────────────────┐
│  Stage 1: Walk                                           │
│                                                          │
│  Python (.py)  ──▶ rustpython-parser  ──▶ CAST JSON      │
│  JavaScript    ──▶ SWC (or Boa)       ──▶ CAST JSON      │
│  Rust (.rs)    ──▶ syn                ──▶ CAST JSON      │
│  C/C++         ──▶ tree-sitter        ──▶ CAST JSON      │
│  Go, Zig, ...  ──▶ tree-sitter        ──▶ CAST JSON      │
│                                                          │
│  Measured as: Walker src→CAST (python_walker / js_walker)│
├──────────────────────────────────────────────────────────┤
│  Stage 2: Compile                                        │
│                                                          │
│  CAST JSON  ──▶ crush_frontend   ──▶ casm::Program       │
│               (semantics + optimizer + compiler)         │
│                                                          │
│  Measured as: Full - (Walker + Execute)                  │
├──────────────────────────────────────────────────────────┤
│  Stage 3: Execute                                        │
│                                                          │
│  casm::Program  ──▶ FastVM        ──▶ FastYield          │
│                  ──▶ CVM1         ──▶ VmResult           │
│                  ──▶ JIT (CL)     ──▶ native code        │
│                  ──▶ AOT Rust     ──▶ precompiled .so    │
│                  ──▶ AOT C        ──▶ precompiled .so    │
│                                                          │
│  Measured via: crush-walk-run -t <file>                  │
└──────────────────────────────────────────────────────────┘
```

## Known Limitations

### Unsupported operations in walker pipeline

The walker lowering from native AST → CAST is incomplete. Operations that
fail during CAST → CASM compilation:

| Language | Operation | Status |
|----------|-----------|--------|
| Python | `//` (floor division) | Not lowered |
| Python | `print()` (→ `io.print` cap) | Capability not registered in FastVM |
| Python | Function calls (`def f(): ... f()`) | Stack underflow in FastVM |
| JavaScript | `console.log()` (→ `io.print` cap) | Same as Python |
| JavaScript | Function calls | Same as Python |
| All | String operations | Partially implemented |

### What works

- Arithmetic chains with locals (`x = a + b; y = c * d`)
- Simple expressions and literals
- Top-level statement sequences without function calls or capabilities

## Related Benchmarks

- [BENCHMARKS.md](BENCHMARKS.md) — Crush execution tier comparisons (CVM1, FastVM, JIT, AOT Rust, AOT C)
- [crush-language-guide appendix](https://github.com/nixpt/crush-language-guide/blob/main/src/appendix/comparison.md) — Language comparison with performance data
