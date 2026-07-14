# Crush — Your Language, Accelerated

> Crush is not a language you learn. It's a compiler that speaks yours.

---

## The Philosophy

Every developer has a grammar they think in. C programmers see `for(int i=0;
i<N; i++)`. Python developers see `for i in range(N):`. JavaScript developers
see `for (let i = 0; i < N; i++)`. All express the same loop — but asking
any of them to write in a *different* loop syntax feels like typing with the
wrong hand.

Crush doesn't ask them to switch. It meets each developer in their native
grammar, lowers their code to a universal representation (CAST), and compiles
it through the fastest backend available.

```
        THE CRUSH PROMISE

┌──────────┐   ┌───────��──┐   ┌──────────┐   ┌──────────┐
│  C dev   │   │ Python   │   │  JS dev  │   │  Go dev  │
│  writes  │   │  dev     │   │  writes  │   │  writes  │
│    C     │   │  writes  │   │    JS    │   │    Go    │
│  syntax  │   │  Python  │   │  syntax  │   │  syntax  │
└────┬────��┘   └────┬─────┘   └────┬─────┘   └────┬─────┘
     │               │               │               │
     ▼               ▼               ▼               ▼
┌─────────────────────────────────────────────────────────┐
│                  CRUSH WALKERS                          │
│  C Walker │ Python Walker │ JS Walker │ Go Walker │ ... │
│                                                         │
│  Each walker: same input language → same CAST IR        │
└────────────────────────┬────────────────────────────────┘
                         │  CAST IR (universal)
                         ▼
┌─────────────────────────────────────────────────────────┐
│                  CRUSH BACKENDS                          │
│                                                         │
│  CVM1 (dev loop) │ FastVM (prod) │ AOT C (gcc -flto)    │
│  ──────────────────────────────────────────────────     │
│  ONE pipeline. Choose backend per deployment target.    │
└─────────────────────────────────────────────────────────┘
```

**The same loop, in four surface syntaxes:**

```c
// Alice writes this
for (int i = 0; i < N; i++) { sum += i; }
```

```python
# Max writes this
for i in range(N): sum += i
```

```javascript
// Mallika writes this
for (let i = 0; i < N; i++) { sum += i; }
```

```crush
// The "native" crush syntax — optional, if you want it
for i in 0..N { sum += i }
```

All four produce the identical CAST IR. All four get the same backends.
Alice keeps her semicolons. Max keeps his colons. Mallika keeps her braces.

---

## Four Developers, Zero Syntax Changes

### Alice — the C Kernel Engineer

> "I've written C for 15 years. I'm not learning a new syntax. But I'd use a
> compiler that gives me polyglot interop and better optimization than my
> Makefile's -O3."

**What Alice already knows:** `int`, `for (;;)`, `*ptr`, `&addr`, `struct`,
`switch`, `malloc`, `printf`.

**What Crush gives her:**
- Her C source compiles through `crush-aotc` to a `.so` with gcc `-O3 -flto
  -march=native` — same flags, same performance.
- Her C module can `__crush_ffi__` into Python teammates' modules. All
  languages link into one process.
- `crush-aotc benchmark sieve.c` compares her code across CVM1, AOT C, AOT Rust.
  She picks the fastest tier.

```bash
$ crush-aotc compile kernel.c --emit c -o libkernel.so   # native .so
$ gcc -o myapp myapp.c -L. -lkernel                        # link as usual
```

### Max — the Web Developer

> "I write Python and JS. My analytics pipeline processes CSV rows. It works
> but it's slow. I don't want to learn C to make it fast."

**What Max already knows:** `def`, `if/elif/else`, `for...in`, lists,
`.append()`, `print()`.

**What Crush gives him:**
- Dev loop: `crush-walk-run analytics.py` — instant feedback, CVM1 interpreter.
- Production: `crush-aotc compile analytics.py --emit c -o libanalytics.so` —
  same logic, native speed. Lambda loads it via `ctypes`.
- He can benchmark: `crush-aotc benchmark analytics.py` — see the speedup before
  committing.

```bash
$ crush-walk-run analytics.py            # dev: 24ms
$ crush-aotc compile analytics.py --emit c -o libanalytics.so
$ python3 -c "import ctypes; lib=ctypes.CDLL('libanalytics.so'); ..."  # prod: ~2ms
```

### Nick — the Go & Shell Scripter

> "I tried Rust once. The borrow checker and I didn't get along. I write Go
> and bash. My deployment script is 800 lines and getting slower."

**What Nick already knows:** `if [ condition ]; then`, `for x in ...; do`,
`echo`, `func`, `go func()`, `defer`.

**What Crush gives him:**
- Bash scripts compiled to standalone native binaries.
- Go programs walk through the same pipeline.
- Zero syntax change. His `deploy.sh` is valid crush input.

```bash
$ crush-walk-run deploy.sh               # dev: interpret
$ crush-aotc compile deploy.sh --emit c -o deploy
$ ./deploy                                # prod: native binary
```

### Mallika — the Game Engine Developer

> "My Node.js physics engine hits 18ms per frame. I need 6ms. Rewriting in
> C++ would take months and I'd lose the iteration speed of JS."

**What Mallika already knows:** `function`, `var`/`let`, `for`, `if`,
arrays, objects.

**What Crush gives her:**
- Hot paths stay in JS. The inner collision loop walks to CAST, compiles to
  C via AOT.
- NAPI wrapper exposes the `.so` to Node — same API, 3× faster.
- GPU path (PTX backend, in design) maps the same JS loops to CUDA kernels
  without changing her source.

```bash
$ crush-aotc compile physics.js --emit c -o libphysics.so
$ node -e "const lib = require('./physics_binding'); lib.update(bodies, dt)"
# 6ms per frame. No C++ rewrite.
```

---

## The Architecture That Makes This Possible

### 1. Walkers: Language → CAST

Each walker is a parser + lowerer pair. Given source in a language, it
produces CAST (Crush AST), a universal representation of computation:

| Walker | Parser | Source Lines | Maturity |
|--------|--------|-------------|----------|
| `c` | tree-sitter-c | ~850 | Full (if/while/for/switch/functions/structs/pointers) |
| `python` | rustpython-parser | ~800 | Strong (functions/loops/lists/ifs; no classes/try) |
| `js` | swc + boa | ~1500 | Strong (async/classes/arrow fns; no generators) |
| `rust` | tree-sitter-rust | ~200 | Basic (functions/variables/control flow) |
| `go` | tree-sitter-go | ~300 | Basic (functions/variables/control flow) |
| `bash` | tree-sitter-bash | ~200 | Basic (commands/if/for/functions) |
| `zsh` | tree-sitter-bash | ~100 | Basic (extends bash walker) |

Each walker implements the `Walker` trait (tree-sitter) or `Frontend` trait
(parser-agnostic). Adding a new language = writing a walker. Every existing
backend lights up automatically.

### 2. CAST IR: The Universal Middle

All walkers produce the same CAST. A `for` loop is a `for` loop, whether it
came from C or Python:

```json
// The same IR, regardless of source language
{
  "Statement::While": {
    "condition": { "Expression::BinaryOp": { "operator": "<", ... } },
    "body": [ ... ]
  }
}
```

### 3. Backends: CAST → Executable

| Backend | How | When to use |
|---------|-----|------------|
| **CVM1** | Bytecode interpreter in Rust | Dev loop, instant feedback |
| **FastVM** | Optimized interpreter | Production, moderate speed |
| **JIT** | Runtime compilation (~20 ops) | Mixed workloads |
| **AOT C** | Transpile to C + gcc -O3 -flto | Maximum speed, `.so` output |
| **AOT Rust** | Transpile to Rust + rustc | Rust ecosystem integration |

---

## Proof Points

Measured on an Intel i7-13700K, single-core:

| Benchmark | CPython 3.14 | Crush CVM1 (release+LTO) | Speedup |
|-----------|-------------|-------------------------|---------|
| Sieve n=10K | 166ms | 24ms | **6.9×** |
| Merge sort n=5K | 30ms | 10ms | **3.0×** |
| Fibonacci n=20 | ~1ms | 10ms | — (dominated by VM overhead) |

The CVM1 interpreter alone beats CPython on numeric workloads. AOT C with
`-O3 -flto` will close the remaining gap to hand-written C.

---

## Getting Started By Persona

```bash
# Alice (C developer)
$ echo 'int main() { int x = 2 + 3 * 4; printf("%d\n", x); return 0; }' > hello.c
$ crush-walk-run hello.c                          # dev
$ crush-aotc compile hello.c --emit c -o libhello.so  # ship

# Max (Python developer)
$ echo 'print(2 + 3 * 4)' > hello.py
$ crush-walk-run hello.py                         # dev
$ crush-aotc compile hello.py --emit c -o libhello.so  # ship

# Nick (shell scripter)
$ echo 'echo "Hello from crush"' > hello.sh
$ crush-walk-run hello.sh                         # dev
$ crush-aotc compile hello.sh --emit c -o hello   # ship as binary

# Mallika (JS developer)
$ echo 'console.log(2 + 3 * 4)' > hello.js
$ crush-walk-run hello.js                         # dev
$ crush-aotc compile hello.js --emit c -o libhello.so  # ship
```

One pipeline. Every language. Any backend.

---

## What This Is NOT

- **Not a new language** — Crush has its own syntax (`.crush` files) but it's
  optional. You can write crush programs in any syntax the walkers support:
  C syntax, Python syntax, JS syntax, Rust syntax, Go syntax, bash syntax.

- **Not a transpiler** — Crush doesn't convert Python to C. It converts Python
  to CAST, then CAST to C. The source language is an *input surface grammar*,
  not the compilation target.

- **Not a WASM runtime** — Crush compiles to native `.so` files, not sandboxed
  bytecode. If you want sandboxing, use the CVM1 interpreter with quotas.

- **Not a polyglot VM in the GraalVM sense** — Crush doesn't run multiple
  VMs side-by-side. It lowers all languages to ONE IR, then compiles that IR
  through ONE backend. No marshaling, no FFI overhead between languages.

---

## Roadmap

| Capability | Status |
|------------|--------|
| C → CVM1, AOT C, AOT Rust | ✅ Complete |
| Python → CVM1, AOT C, AOT Rust | ✅ Complete |
| JS → CVM1 | ✅ Complete |
| Bash → CVM1 | ✅ Complete |
| Rust/Go/Zig → CVM1 | ✅ Walker exists, needs AOT path |
| C↔Crush FFI (`__crush_ffi__`) | ✅ Plugin auto-build |
| Python↔Crush FFI | ✅ Slice/in/is lowering |
| `libcrush_vm.so` embedding | ✅ 19MB cdylib |
| `crush-aotc benchmark` across all walkers | ⬜ Needs walker→CASM path for each |
| GPU backend (PTX) | 🚧 Design complete |
| Inline polyglot blocks (`lang python { ... }`) | ⬜ Design needed |
