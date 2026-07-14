# Crush Benchmarks

Cross-language performance comparison across 5 Crush execution tiers, Python, Node.js, Lua, LuaJIT, and Perl.

## Results

Run on `crush-aotc benchmark` with 500 iterations per tier. System: Linux x86_64, Rust 1.96, gcc 16.1, clang 22.1, Python 3.14, Node 26.2, Lua 5.4, LuaJIT 2.1, Perl 5.40.

### Benchmark: `simple` (return 42)

Single constant return — measures VM/minimal execution overhead.

| Tier | Time (µs) | Speedup vs CVM1 |
|------|-----------|-----------------|
| **CVM1** (interpreter) | 19.3 | 1.0× |
| **FastVM** (lowered interpreter) | 211.7 | 0.09× |
| **AOT Rust** (rustc -O3) | 0.5 | 41.9× |
| **AOT C (gcc)** (gcc -O3) | 0.4 | 53.6× |
| **AOT C (clang)** (clang -O3) | 0.4 | 54.5× |
| Python 3.14 | 33,287.6 | 0.0006× |
| Node.js 26.2 | 42,825.6 | 0.0005× |
| Lua 5.4 | 779.6 | 0.025× |
| LuaJIT 2.1 | 1,168.2 | 0.017× |
| Perl 5.40 | 1,591.7 | 0.012× |

### Benchmark: `compute` (14-op arithmetic chain)

Multi-step arithmetic with locals — measures optimizer strength.

| Tier | Time (µs) | Speedup vs CVM1 |
|------|-----------|-----------------|
| **CVM1** (interpreter) | 162.4 | 1.0× |
| **FastVM** (lowered interpreter) | 348.0 | 0.5× |
| **AOT Rust** (rustc -O3) | 1.3 | 129.5× |
| **AOT C (gcc)** (gcc -O3) | 0.5 | 317.1× |
| **AOT C (clang)** (clang -O3) | 0.4 | 377.6× |
| Python 3.14 | 33,207.3 | 0.005× |
| Node.js 26.2 | 43,879.5 | 0.004× |
| Lua 5.4 | 615.1 | 0.26× |
| LuaJIT 2.1 | 877.1 | 0.19× |
| Perl 5.40 | 1,951.7 | 0.08× |

## Key Findings

### 1. AOT dominates

The AOT tiers (Rust via rustc, C via gcc/clang) are **40–380× faster** than
the CVM1 interpreter. For the `compute` benchmark, LLVM/GCC constant-fold the
entire 14-op chain into a single constant — eliminating all stack operations.
This is a legitimate advantage: an AOT compiler sees through the stack machine
to the underlying computation.

### 2. FastVM overhead on trivial programs

FastVM is slower than CVM1 for trivial programs (0.09× on `simple`). This is
expected — FastVM's lowering pass and stack frame setup have fixed overhead
that dominates for programs with <50 instructions. FastVM is designed for
hot loops with thousands of iterations, where its index-based dispatch and
pre-resolved symbol tables pay off.

### 3. Process startup dominates scripting languages

Python and Node.js show ~33,000–44,000 µs per run because the benchmark runs
each iteration as a separate process (`python3 script.py`). This measures
startup time (interpreter init + JIT warmup), not steady-state throughput.
For fair comparison, a single process should loop internally.

**Lesson:** Use `crush-aotc benchmark` for comparing Crush's internal tiers
and compiled languages. For scripting languages, a process-per-iteration
benchmark is a startup-cost measurement, not a throughput measurement.

### 4. Lua/LuaJIT as the fastest scripting runtimes

Lua 5.4 and LuaJIT 2.1 show significantly lower per-call overhead (~600–1200 µs)
than Python or Node.js (~33,000–44,000 µs). Lua's minimal runtime and
smaller interpreter contribute to faster cold-start performance.

### 5. C backend edge

The C AOT backend via clang consistently edges out Rust via rustc (54.5× vs
41.9× on `simple`, 377.6× vs 129.5× on `compute`). This is partly because
the generated C code is simpler (no formatting/panic infrastructure) and
partly because clang's optimizer handles the switch-dispatch pattern
differently than LLVM via rustc.

## Running Your Own Benchmarks

```bash
# Build crush-aotc
cd crush-ast && cargo build -p crush-aot --release

# Run a benchmark with companion files
crush-aotc benchmark docs/benchmarks/simple.crush --runs 500

# With external languages
crush-aotc benchmark docs/benchmarks/compute.crush --runs 500 \
    --extern Lua "lua docs/benchmarks/compute.lua" \
    --extern LuaJIT "luajit docs/benchmarks/compute_luajit.lua"

# JSON output for CI
crush-aotc benchmark docs/benchmarks/compute.crush --runs 100 --json > results.json
```

## Companion Files

Place equivalently-named files alongside your `.crush` file for automatic
discovery:

```
mybench.crush          # required
mybench.py             # auto-discovered as "Python3"
mybench.js             # auto-discovered as "Node.js"
mybench.mjs            # auto-discovered as "Node.js (ESM)"
```

Additional languages via `--extern`:

```bash
crush-aotc benchmark mybench.crush \
    --extern Lua "lua mybench.lua" \
    --extern Ruby "ruby mybench.rb"
```

## Source Files

All benchmark sources live in `docs/benchmarks/`:

| File | Description |
|------|-------------|
| `simple.crush` / `.py` / `.js` / `.lua` / `.pl` | Return integer constant |
| `compute.crush` / `.py` / `.js` / `.lua` / `.pl` | 14-op arithmetic chain with locals |

## Tier Reference

| Tier tag | Description |
|----------|-------------|
| `cvm1` | Portable interpreted VM (debuggable, complete) |
| `fastvm` | Lowered bytecode interpreter (hot path) |
| `rust` | AOT via rustc — CASM → Rust source → rustc -O3 |
| `c-gcc` | AOT via gcc ��� CASM → C source → gcc -O3 |
| `c-clang` | AOT via clang — CASM → C source → clang -O3 |
| `py` | Python3 (companion file auto-discovery) |
| `js` | Node.js (companion file auto-discovery) |
| `mjs` | Node.js ESM (companion file auto-discovery) |
