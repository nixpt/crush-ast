# TASK: CA-JS-1 — Boa-based JavaScript/TypeScript frontend (`crush-lang-js`)

**Status:** OPEN · **Repo:** `nixpt/crush-ast` · **Owner lane:** captain-driven
**Supersedes:** the tree-sitter `crates/js_walker` (626-LOC scaffold, 0 tests)
**Advances:** `TASKS.md` P2 "JavaScript/TypeScript frontend"

---

## Goal

Replace crush-ast's tree-sitter `js_walker` with a **native-parser JS/TS frontend**
built on **`boa_parser` + `boa_ast`**, lowering JavaScript to **CAST** — exactly
the way `crush-lang-python` (rustpython-parser) and `crush-lang-rust` (syn) already
work. Output is a `crush-lang-js` crate implementing `walker_core::Frontend` plus a
`js_walker` binary that emits CAST JSON.

### Why Boa (not swc / not tree-sitter)
The downstream consumer **surfer-browser already depends on `boa_engine` 0.21**,
which bundles `boa_parser` + `boa_ast` (both already in surfer's `Cargo.lock`).
Using the same Boa toolchain means **one Boa in the graph: surfer *executes* web
JS with it, crush-ast *parses* JS→CAST with it** — no second JS parser. This is the
enabling step for the broader arc "**Crush as surfer's scripting language; Boa for
web content only**" (see `workspace-meta/FOREMAN_THREADS.md` → "🟨 Crush as surfer's
first-class scripting language").

---

## Scope

**IN:** the static **transpile lane** — lower the JS subset that maps cleanly to
CAST/CASM (let/const/var, functions + arrow fns, if/while/for, binary/unary ops,
calls, member/index access, array/object literals, return/throw/try-catch,
template literals → string concat where feasible). Produce a `FeatureReport` that
flags what is NOT lowerable so the caller can reject or route it.

**OUT (explicitly):** building a JS *engine*. Do NOT attempt full dynamic JS
semantics (prototype chains, `this` rebinding, generators, `eval`, the JS stdlib,
async runtime). Those stay with **Boa** (surfer's `browser/js.rs` runtime). When a
construct can't be lowered, record it in `FeatureReport` and bail with a clear
error — do not half-emit incorrect CAST.

---

## The pattern to mirror (copy this structure)

`crates/crush-lang-python/` is the reference. Create `crates/crush-lang-js/` with
the same layout:

```
crates/crush-lang-js/
  Cargo.toml          # deps: boa_parser, boa_ast (version matching boa_engine 0.21),
                      #       crush-cast.workspace, walker-core.workspace, crush-frontend (path)
  src/
    lib.rs            # pub struct JsFrontend; impl walker_core::Frontend for JsFrontend;
                      # + convenience `pub fn js_to_cast(&str) -> anyhow::Result<crush_cast::Program>`
    parser.rs         # boa_parser::Parser → boa_ast (the opaque AST returned by Frontend::parse)
    analyzer.rs       # FeatureReport: uses_functions/classes/async/exceptions/imports +
                      # NOT-lowerable flags (generators, prototype tricks, eval, …)
    lower_stmt.rs     # boa_ast statements  → crush_cast::Statement
    lower_expr.rs     # boa_ast expressions → crush_cast::Expression
    bin/
      walker.rs       # js_walker: read file arg → js_to_cast → print CAST JSON to stdout
```

### The `Frontend` trait (in `crates/walker-core/src/lib.rs:90`)
```rust
pub trait Frontend {
    fn language_name(&self) -> &'static str;          // "javascript"
    fn file_extensions(&self) -> &[&'static str];     // &[".js", ".mjs", ".ts", ".jsx", ".tsx"]
    fn parse(&self, source: &str) -> Result<Box<dyn std::any::Any>>;       // -> boxed boa AST
    fn analyze(&self, ast: &Box<dyn std::any::Any>) -> Result<FeatureReport>;
    fn lower(&self, ast: Box<dyn std::any::Any>) -> Result<crush_cast::Program>;
}
```
`walker_core::frontend_pipeline(&frontend, source)` runs parse→analyze→lower.

### The walker binary contract (mirror `crush-lang-python/src/bin/walker.rs`)
```rust
// js_walker <file.js>  →  prints CAST JSON to stdout
let program = crush_lang_js::js_to_cast(&source)?;
println!("{}", serde_json::to_string_pretty(&program)?);
```
This is the subprocess contract exosphere's `SubprocessWalker` and crush-frontend
expect (temp file in → JSON CAST out).

### CAST target (in `crates/crush-cast/src/lib.rs`)
Lower into `crush_cast::{Program, Function, Statement, Expression, CastType}`.
Match the existing variants used by the Python/Rust frontends (VarDecl, ExprStmt,
If, While, For, Return, FunctionDef, TryCatch, Throw; BinaryOp, UnaryOp, Call,
GetField, Index, ArrayLiteral, ObjectLiteral, …). For JS `import`/ES modules use
`Statement::Import` / `ImportStatement`. Inline `<script>`-style blocks can target
`Statement::LangBlock { lang: "javascript", … }` when full lowering isn't possible.

---

## Done conditions

1. `crates/crush-lang-js` exists, builds, and implements `Frontend` for `JsFrontend`.
2. `js_walker` binary parses representative JS and prints valid CAST JSON.
3. **Tests** mirroring `crush-lang-python` (≈9): per-construct lowering round-trips
   (function, if/for/while, ops, array/object, try/catch) + a `FeatureReport` test
   that a non-lowerable construct (e.g. generator/`eval`) is flagged, not mis-lowered.
4. The old **tree-sitter `crates/js_walker` is removed** (and dropped from the
   workspace `members` + any `cli` dispatch table); binary name `js_walker` preserved
   for subprocess dispatch.
5. `boa_parser`/`boa_ast` pinned to the version `boa_engine` 0.21 resolves to
   (so surfer + crush-ast share one Boa — verify against surfer's Cargo.lock).
6. `cargo test --workspace` green (current baseline: 414 tests); `cargo build -p crush-lang-js` clean.

## Verify
```
cargo test -p crush-lang-js
cargo run -p crush-lang-js --bin js_walker -- <some.js>   # prints CAST JSON
cargo test --workspace          # no regressions
```

## Gotchas
- **Boa version drift:** boa crates version in lockstep with `boa_engine`. Match
  surfer's (0.21) or the shared-toolchain benefit is lost. Confirm `boa_parser`/
  `boa_ast` versions in `/workspace/projects/surfer-browser/Cargo.lock`.
- **ECAP/bincode + serde_json::Value** (seen in `crush-pkg`): bincode can't
  deserialize `serde_json::Value`. Irrelevant to lowering, but don't stash arbitrary
  JSON in bincode-serialized structs.
- **Don't over-reach:** TS type annotations can be parsed-and-dropped (lower the
  runtime semantics); full TS type-checking is out of scope.
- Boot the repo with `dejavue context` first; capture the design decision with
  `dejavue decision` when you settle the lowering boundary.

## References
- Pattern: `crates/crush-lang-python/` (rustpython) and `crates/crush-lang-rust/` (syn)
- Design rationale: `docs/design/crushvm-rustpython.md` (the three-lane model + "one VM, frontends provide syntax")
- Arc context: `workspace-meta/FOREMAN_THREADS.md` → "🟨 Crush as surfer's first-class scripting language"
