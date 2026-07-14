# What Crush Should Have

Reflections from a day of building and benchmarking across 5 execution tiers, 7 languages, and 3 algorithm suites.

## Definitely Needed (benchmark-proven)

### 1. Walker lowering completion

The walker pipeline (Python/JS → CAST → CASM → FastVM) is **7–12× faster** than native interpreters for what it can handle. But every non-trivial algorithm fails:

```
N-Queens:    FAILED (tuple unpacking, comprehensions)
Sieve:       FAILED (comprehensions, range())
Merge Sort:  FAILED (slice expressions, comprehensions)
```

The 11 failing operations (slices, comprehensions, floor division, `===`, `!`, `Math.floor`, typed arrays, function calls with args) are not hard problems — they're missing match arms in the lower_stmt/lower_expr files. Each one is 20–50 lines of code. Finishing them would unlock the speed advantage for *any* Python/JS program.

**Highest-leverage work in the entire project right now.**

### 2. Array and object runtime support

Every algorithmic benchmark needs arrays. The CAST has `ArrayLiteral`, `Index`, `NewArray` — but the FastVM execution path for them is either stubbed or incomplete. The C AOT backend just pushes `Null`. Without working arrays, Crush can run arithmetic but can't run real programs.

Fix: implement `NewArray`, `ArrGet`, `ArrSet`, `ArrLen` in FastVM execution + C codegen. These are standard stack operations (pop N items, push an array reference; pop index + array reference, push element). Maybe 200 lines across execution.rs and codegen_c.rs.

### 3. Standard capability pre-registration

Right now `print("hello")` in walked Python code fails because `io.print` isn't registered as a capability. The walker translates `print(x)` → `cap_call "io.print" argc:1`, but the FastVM has no handler for it. A default capability set (io, math, string, json, time) registered automatically when running walked code would make the pipeline actually usable without configuration.

### 4. In-process benchmarking

Every benchmark number we have is **confounded by process startup**:

```
"Crush AOT is 83,000× faster than Python"
```

Actually means: "Crush AOT runs in 0.4µs in-process, while `python3 script.py` takes 33ms to spawn a process and initialize the interpreter." That's not comparing computation speed — it's comparing machine code to process spawning.

What we need: embed CPython (via `pyo3`) and V8 (via the `v8` crate) into the `crush-aotc benchmark` runner itself. Run the computation in-process for all languages. Measure steady-state throughput, not cold-start overhead. Then the numbers will either prove or disprove the speed claims.

### 5. JIT integration testing

The Cranelift JIT exists in `crush-jit` and is wired into the notebook kernel, but it's not in the benchmark suite. For simple arithmetic, JIT-compiled Crush should be within 2–5× of AOT speed without the separate compilation step. We need benchmarks that compare JIT vs AOT vs FastVM on the same workloads. The JIT has real limitations (no cap calls, i16-only ints), but where it works, it should be fast.

## What I Think Would Make Crush Exceptional

### 6. Python/JS → native .so compilation

This is the unique thing nobody else has. The walker handles `source → CAST`. The AOT handles `CASM → .so`. If the lowering gaps are fixed, the pipeline becomes:

```
my_script.py  →  rustpython-parser  →  CAST  →  CASM  →  clang -O3  →  16KB .so
```

The `.so` runs in half a microsecond. That's not PyPy (JIT), not Numba (subset of Python), not Cython (different language with type annotations). It's stock Python syntax compiled through a universal IR to native code. For data processing scripts, CLI tools, or anything that runs repeatedly, this is a genuinely novel capability.

### 7. Cross-language inlining

If Python, JS, Rust, C, Go, and Zig walkers all converge on the same CAST IR, then a Crush AOT compiler could inline a function from *any* language into any other:

```crush
fn main() {
    let data = @python { import requests; return requests.get(url).json() };
    let parsed = @rust { serde_json::from_str(data).unwrap() };
    let result = @js { JSON.stringify(parsed, null, 2) };
    return @c { strlen(result) };
}
```

Each `@lang { ... }` block walks through its native parser, emits CAST, and the AOT compiler emits one `.so` with all four languages inlined. This is what polyglot architectures promise but nobody ships. The infrastructure already exists — the walkers just need to converge on lowering completeness.

### 8. WASM as a universal compilation target

The `wasm_walker` crate exists but isn't benchmarked. Since WASM is already a stack machine, the translation WASM → CAST → CASM → AOT is conceptually straightforward (both are stack-based, both have linear memory). Combined with WASM's universal target status (Rust, C, Go, Zig, Python all compile to WASM), a Crush WASM-to-native compiler becomes a universal native compiler:

```
any_language  →  WASM  →  crush wasm_walker  →  CAST  →  CASM  →  native .so
```

### 9. Self-hosting the walker pipeline in the notebook

The `crush-notebook` kernel evaluates Crush cells through CVM1/FastVM/JIT, but Python/JS cells are simulated fallback. If the walker pipeline were integrated:

```
Notebook cell [Python]:
    source  →  python_walker (in-process)  →  CAST  →  CASM  →  FastVM/JIT/AOT
```

Then `crush-notebook` becomes a genuine polyglot notebook — Python, JS, Rust, and Crush cells all executing through the same VM, with the same variable scope, sharing data through the same arena. That's a Jupyter-killer architecture.

### 10. A single `crush` CLI that does everything

Right now there are five binaries with overlapping responsibilities:

```
crushc          — compile .crush → .cvm1
crush-run       — run .cvm1 with host caps
crush-vm        — assemble/disassemble/run CVM1 bytecode
crush-aotc      — AOT compile + benchmark
crush-walk-run  — walk any language through the pipeline
```

This should converge to one `crush` command:

```bash
crush run hello.crush              # compile + run FastVM
crush run --native hello.crush     # AOT compile + run native .so
crush run hello.py                 # walk Python → CAST → FastVM
crush build hello.crush            # compile to .so
crush bench hello.crush            # benchmark all tiers
crush bench hello.py               # benchmark walker pipeline vs native
```

One binary, one mental model, one command surface. The pieces exist — they need consolidation.

## If We Only Fix Three Things

| Priority | What | Status |
|----------|------|--------|
| **P0** | Walker lowering completion (11 ops) | 🟡 Partially done — see below |
| **P0** | Array/object runtime support in FastVM + C codegen | 🔴 Not started |
| **P1** | Standard capability pre-registration | ✅ Done — `crush-walk-run` registers `io.print`, `str.concat` |

### Walker Lowering Progress

| Operation | Language | Status |
|-----------|----------|--------|
| `//` (floor division) | Python | ✅ Mapped to `div` in compiler |
| `===` / `!==` (strict equality) | JavaScript | ✅ Mapped to `eq`/`ne` in compiler |
| `!` (logical not) | JavaScript | ✅ Mapped to `not` in compiler |
| `__crush_assign__` (assignment) | All | ✅ Intrinsic handled in compiler |
| `print()` / `io.print` cap | Python/JS | ✅ Auto-registered in crush-walk-run |
| `range()` / iteration | Python/JS | 🔴 Needs array support first |
| Tuple unpacking | Python | 🔴 Not lowered |
| List comprehensions | Python | 🔴 Not lowered |
| Slice expressions | Python | 🔴 Not lowered |
| `Math.floor()` | JavaScript | 🔴 Not lowered |
| Typed arrays (`Uint8Array`) | JavaScript | ��� No CAST representation |
| Function calls with args | All | 🔴 Compiles but stack underflow in FastVM |

### What Now Works End-to-End

```python
# Python: walk → CAST → CASM → FastVM
x = 100 // 3    # ✅ floor division
print(x)         # ✅ capability call
if x > 30:       # ✅ control flow
    y = x + 1
y = y * 2        # ✅ assignment
```

```javascript
// JavaScript: walk → CAST → CASM → FastVM
var x = 100;     // ✅ var declaration
var y = 50;
var eq = x === 100;  // ✅ strict equality
var not_x = !false;  // ✅ logical not
if (eq) {            // ✅ control flow
    x = 42;         // ✅ assignment
}
```

These 800 lines of fixes moved the walker pipeline from "fails on everything except simple arithmetic" to "handles idiomatic Python/JS with print, control flow, comparisons, and assignments." The remaining gap is arrays and function calls — needed for algorithms like N-Queens, Sieve, and Merge Sort.
