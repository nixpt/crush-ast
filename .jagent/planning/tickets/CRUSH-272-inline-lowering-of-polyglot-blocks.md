# CRUSH-272 — Inline lowering of `@python` / `@javascript` blocks (and the walkers on wasm32)

| Field | Value |
|-------|-------|
| **ID** | CRUSH-272 |
| **Priority** | P1 |
| **Status** | In review |
| **Phase** | M6 |
| **Assignee** | claude |
| **Dependencies** | CRUSH-270 (explicit `->` outputs, crush-web parity), CRUSH-250 (boa lowering), CRUSH-251 (closures) |
| **Estimated effort** | M |
| **Filed by** | claude — 2026-10-16 |

## Problem

A browser can't spawn `python3` or `node`. crush-web runs a granted `@python` /
`@javascript` block only through a page-supplied executor (#91), and for Python that
means shipping an interpreter (Pyodide is ~5.6 MB). Without the `polyglot-python`
feature crush-web also had no free-variable analysis, so a block with inputs/outputs
couldn't marshal at all (CRUSH-270's browser-parity gap).

Crush already has walkers that lower Python and JavaScript to CAST. Two things kept
them out of the browser:

1. `crush-lang-python` did not build for `wasm32-unknown-unknown`. Its parser
   (`rustpython-parser`) is pure Rust; the C came only from inheriting
   `crush-walker-core` with default features, which pull `tree-sitter`.
   `crush-lang-js` already opted out with a direct `default-features = false` dep.
2. Nothing could lower a block's body into the enclosing program.

## Decision

- **Manifest fix.** `crush-lang-python` depends on `crush-walker-core` by path with
  `default-features = false` (a `.workspace = true` dep can't turn defaults off). It uses
  only `Frontend` / `LowerCtx` / `FeatureReport`, which never needed tree-sitter. CI's
  `Test (walkers)` job runs `cargo check --target wasm32-unknown-unknown` on both walkers.
  With this, `crush-lang-sdk`'s `polyglot-python` feature builds for wasm32 too, and
  crush-web gains a `polyglot-python` feature.
- **`PolyglotMode::Lower`** (crush-lang-sdk, `compile_crush_source_with` /
  `CompileOptions`), opt-in; `PolyglotMode::Exec` stays the default, so `crush-run` is
  unchanged. Each `@python` / `@javascript` block becomes one CAST function:
  - inputs (the analyzer's free reads that are Crush locals) are its parameters;
  - its body is the block lowered by the walker **as a `def` / `function` body**, so
    everything it binds is its own local and a `def` / `function` in it is a closure over
    those locals (how module code reads its globals);
  - it returns `[outputs...]`; the call site copies each output back into its Crush
    variable.
  - Python: `crush_lang_python::inline::lower_block_function`. JS: the body is wrapped in
    `function NAME(inputs) {…}` (header on the body's first line, so walker positions map
    to `.crush` lines) and lowered by the **boa** lowering
    (`crush_lang_js::inline::lower_block_function`); non-capturing functions it lifts to
    the top level are renamed under the block's prefix.
- **The boundary is JSON's.** A new VM builtin, `lang.marshal(value, name)` (scheduler
  and portable VMs), deep-copies each input and output the way the subprocess's JSON
  round-trip would (sequences → arrays, records → maps) and refuses, naming the
  variable, what JSON can't carry (a function, a handle, a non-finite float). Without it
  `xs.append(3)` inside a block would change the Crush array — the VM shares arrays by
  reference.
- **Exact or loud.** A construct the walker can't lower fails compilation:
  `line N: this @python block can't be lowered inline: <construct> …` plus help saying the
  block runs *as Crush*, not CPython / Node, and needs a real interpreter
  (`--polyglot`). Nothing falls back to `EXEC_LANG`. Refused besides the walkers' own
  errors:
  - module-illegal statements a `def` body would make legal (`return`, `yield`,
    `nonlocal` at the block's top level; a JS block that doesn't parse as a script);
  - a free name that is a Crush function (a real interpreter would `NameError`);
  - `import` other than a top-level unaliased one of a module the walker maps calls of
    (`math`, `json`, …), which is dropped;
  - Python tuples (the walker makes them lists: `print((1, 2))` would print `[1, 2]`) and
    module attributes (`math.pi`), only while lowering a block;
  - `math.floor` / `math.ceil` (always — they returned floats; see CRUSH-273);
  - a builtin with no Crush lowering (`sum`, `int`, `str`, `abs`, `x in xs`, …), which
    used to compile to a bare-name capability that fails only when reached;
  - a block where the pass doesn't reach (inside `try`, a lambda, a `match` arm).
- **Pipeline.** Walker CAST isn't written for Crush's semantic checker or optimizer (the
  walkers' own pipeline compiles it unchecked). In `Lower` mode the Crush program is
  checked as `Exec` would see it (blocks opaque), then blocks are lowered, the Crush
  functions optimized, and the block functions added unoptimized.
- **Grants.** A lowered block spawns nothing and needs no `polyglot.<lang>` grant;
  `@bash` and other languages are left to `EXEC_LANG` as before.
- **crush-web.** `RunOptions.polyglot: "exec" | "lower"` (default `"exec"`), README
  section "Lowered polyglot blocks". The headless-Chromium harness runs a lowered
  `@python` block and a refused one (built with `--features polyglot-python`).

## Results (2026-10-16)

- All 23 programs of `crates/crush-lang-python/tests/cpython_corpus/`, each as an
  `@python` block body, print exactly CPython's output, on the scheduler VM and the
  portable VM, with no host capabilities at all.
- The JS walker's node corpus as `@javascript` blocks: 76 of 119 exact, 43 loud,
  0 silently wrong. The boa lowering runs 81 standalone; the 5 others fail in the
  Crush lexer, which can't capture their source as a block body (CRUSH-274).

## Construct coverage when lowered (Python)

| Construct | Lowered |
|---|---|
| int / float / str / bool / None, arithmetic incl. `//` `%` `**`, comparisons | lowered |
| `print` incl. `sep=` / `end=` / `flush=` | lowered |
| if / while / for over lists, ranges, dicts; break / continue | lowered |
| lists, dicts, indexing, slicing, `len`, `.append` / `.pop`, string methods the walker maps | lowered |
| `def`, recursion, closures, `nonlocal`, `lambda` (CRUSH-251) | lowered |
| list comprehensions as an assignment RHS / return value / call argument | lowered |
| try / except / raise, `match` | lowered |
| `import math` / `json` / … + calls `math.sqrt(...)` | lowered (the import is dropped) |
| tuples (literals, `return a, b`) | loud |
| tuple unpacking (`a, b = …`, `for a, b in …`) | loud |
| a comprehension nested inside a larger expression, generators, dict / set comprehensions, set literals | loud |
| f-strings with interpolation, `%` formatting | loud |
| classes, `with`, `del`, `assert`, `global`, decorators, default arguments | loud |
| `int()` / `str()` / `sum()` / `abs()` / `in` / `is` …, `math.floor` / `math.ceil`, `math.pi` | loud |
| other `import`s, `return` / `yield` / `nonlocal` at block level | loud |
| a function as an output | loud (run time, as with the interpreter's JSON) |

## Success criteria

- [x] `cargo check --target wasm32-unknown-unknown -p crush-lang-python -p crush-lang-js` passes; CI runs it.
- [x] `PolyglotMode::Lower` lowers `@python` / `@javascript` blocks; default unchanged.
- [x] The CPython corpus as `@python` blocks matches CPython on both VMs, without a grant.
- [x] A construct the walker can't lower is a compile error naming it and the block's line.
- [x] Inputs and outputs marshal through lowering, with JSON's copy semantics.
- [x] crush-web `polyglot: "lower"`, native tests, and the headless-Chromium check.
- [ ] crush-lang-guide labels lowered blocks honestly (left for a docs PR).

## Follow-ups

- CRUSH-273: `math.*` module calls pass straight through to Crush's stdlib with Crush semantics.
- CRUSH-274: the Crush lexer can't capture some polyglot block bodies (`'` in a comment, `''`).
- Tuples as real `Value::Tuple` in the Python walker would let lowered blocks use them.
