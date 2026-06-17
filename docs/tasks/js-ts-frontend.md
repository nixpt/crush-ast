# TASK: CA-JS-1 — JavaScript/TypeScript frontend (`crush-lang-js`), dual-backend

**Status:** OPEN · **Repo:** `nixpt/crush-ast` · **Owner lane:** captain-driven
**Supersedes:** the tree-sitter `crates/js_walker` (626-LOC scaffold, 0 tests)
**Advances:** `TASKS.md` P2 "JavaScript/TypeScript frontend"

---

## Goal

Replace crush-ast's tree-sitter `js_walker` with a **native-parser JS/TS frontend**
(`crush-lang-js`) that lowers JavaScript **and TypeScript/JSX** to **CAST**, the way
`crush-lang-python` (rustpython-parser) and `crush-lang-rust` (syn) already work.

Build it **complete in one pass** with a **pluggable parser backend** so it never
needs revisiting:

- **swc backend (primary, default):** `swc_ecma_parser` + `swc_ecma_ast` → CAST.
  Full **JS + TS + JSX/TSX**, robust on real-world source. This is the completeness
  guarantee — it parses everything, so TS/JSX/edge cases never force a revisit.
- **boa backend (optional, feature `boa-backend`):** `boa_parser` + `boa_ast` → CAST.
  **JS-only**, lighter binary; for Boa-aligned builds or a future *in-process*
  surfer embedding (see note below). May be JS-subset/best-effort — it must never
  block completeness.

Both backends lower into the **same `crush_cast::Program`** behind one
`walker_core::Frontend`.

### Architecture note (why dual-backend is cheap here)
The walkers run as **standalone subprocess binaries** (exosphere's `SubprocessWalker`:
file in → CAST JSON out). They are **not linked into surfer's graph** — surfer deps
`crush-frontend`/`crush-cast` for the *Crush* runtime, never the JS walker. So **swc's
weight does NOT land in surfer's build.** The "share Boa with surfer" benefit only
applies to a hypothetical in-process JS→CAST embedding in surfer (not today's model);
that's the sole reason the boa backend exists at all. Surfer keeps executing real web
JS with `boa_engine` regardless — this frontend is the *transpile* path, not a JS engine.

---

## Scope

**IN:** the static **transpile lane** — lower the JS/TS subset that maps cleanly to
CAST/CASM: let/const/var, functions + arrow fns, classes (where lowerable), if/while/
for, binary/unary ops, calls, member/index access, array/object literals,
return/throw/try-catch, template literals, ES `import`/`export`. TS **type
annotations are parsed and dropped** (lower the runtime semantics; no type-checking).
Produce a `FeatureReport` flagging anything NOT lowerable so the caller can reject/route.

**OUT (explicitly):** a JS *engine*. No full dynamic semantics (prototype chains,
`this` rebinding, generators, `eval`, JS stdlib, async runtime). Those stay with **Boa**
in surfer. Unlowerable construct → record in `FeatureReport`, bail with a clear error;
never half-emit incorrect CAST. TS **type-checking** is out of scope.

---

## The pattern to mirror

`crates/crush-lang-python/` is the reference. Create `crates/crush-lang-js/`:

```
crates/crush-lang-js/
  Cargo.toml      # deps: swc_ecma_parser + swc_ecma_ast (+ swc_common) [default];
                  #       boa_parser + boa_ast [optional, feature "boa-backend",
                  #       version matching surfer's boa_engine 0.21];
                  #       crush-cast.workspace, walker-core.workspace, crush-frontend (path)
                  # features: default = ["swc-backend"]; swc-backend; boa-backend
  src/
    lib.rs        # pub struct JsFrontend; impl walker_core::Frontend;
                  # pub fn js_to_cast(src, opts) -> anyhow::Result<crush_cast::Program>
                  # backend dispatch (by feature + file extension)
    backend/
      mod.rs      # Backend enum { Swc, Boa }; selection logic
      swc.rs      # swc_ecma_parser → swc_ecma_ast  (JS/TS/JSX)
      boa.rs      # boa_parser → boa_ast            (JS only; cfg(feature="boa-backend"))
    analyzer.rs   # FeatureReport over the chosen AST (functions/classes/async/
                  # exceptions/imports + NOT-lowerable flags: generators/eval/proto tricks)
    lower_swc.rs  # swc_ecma_ast  → crush_cast::{Statement,Expression}
    lower_boa.rs  # boa_ast       → crush_cast::{Statement,Expression}  (cfg boa-backend)
    bin/
      walker.rs   # js_walker: read file arg → js_to_cast → CAST JSON to stdout
```

### `Frontend` trait (`crates/walker-core/src/lib.rs:90`)
```rust
pub trait Frontend {
    fn language_name(&self) -> &'static str;          // "javascript"
    fn file_extensions(&self) -> &[&'static str];     // &[".js",".mjs",".cjs",".ts",".tsx",".jsx",".mts"]
    fn parse(&self, source: &str) -> Result<Box<dyn std::any::Any>>;
    fn analyze(&self, ast: &Box<dyn std::any::Any>) -> Result<FeatureReport>;
    fn lower(&self, ast: Box<dyn std::any::Any>) -> Result<crush_cast::Program>;
}
```
`walker_core::frontend_pipeline(&frontend, source)` runs parse→analyze→lower.

### Backend routing
- `.ts/.tsx/.jsx/.mts` → **always swc** (boa can't parse TS/JSX).
- `.js/.mjs/.cjs` → **boa** if the `boa-backend` feature is enabled, else **swc**.
- A construct swc parses but the lowerer can't handle → `FeatureReport` + error (do not emit).

### Walker binary contract (mirror `crush-lang-python/src/bin/walker.rs`)
```rust
// js_walker <file.(js|ts|jsx|tsx)>  →  prints CAST JSON to stdout
let program = crush_lang_js::js_to_cast(&source, Default::default())?;
println!("{}", serde_json::to_string_pretty(&program)?);
```
This is the subprocess contract exosphere's `SubprocessWalker` + crush-frontend expect.

### CAST target (`crates/crush-cast/src/lib.rs`)
Lower into `crush_cast::{Program, Function, Statement, Expression, CastType}` — reuse the
variants the Python/Rust frontends already target (VarDecl, ExprStmt, If, While, For,
Return, FunctionDef, TryCatch, Throw; BinaryOp, UnaryOp, Call, GetField, Index,
ArrayLiteral, ObjectLiteral, …). ES modules → `Statement::Import`/`ImportStatement`.
Where full lowering isn't possible but you want to preserve the source, fall back to
`Statement::LangBlock { lang: "javascript", … }`.

---

## Done conditions

1. `crates/crush-lang-js` builds with **both** feature sets: `--no-default-features
   --features swc-backend` and `--features boa-backend`.
2. `JsFrontend` implements `walker_core::Frontend`; `js_walker` prints valid CAST JSON
   for `.js`, `.ts`, `.jsx`, `.tsx` inputs (swc path).
3. **swc backend** lowers the full IN-scope JS+TS+JSX subset; TS type annotations
   parsed-and-dropped.
4. **boa backend** lowers the JS subset (feature-gated); routing sends TS/JSX to swc.
5. **Tests** (mirror crush-lang-python ≈9, per backend where relevant): per-construct
   lowering round-trips (fn, arrow, class, if/for/while, ops, array/object, try/catch,
   template literal, import); a TS-annotation test (swc) confirming types are dropped;
   a `FeatureReport` test that a non-lowerable construct (generator/`eval`) is flagged,
   not mis-lowered.
6. Old tree-sitter `crates/js_walker` **removed** (and dropped from workspace `members`
   + any `cli` dispatch); binary name `js_walker` preserved for subprocess dispatch.
7. `boa_parser`/`boa_ast` pinned to the version `boa_engine` 0.21 resolves to (verify
   against surfer's Cargo.lock) so the optional boa path stays Boa-aligned.
8. `cargo test --workspace` green (baseline 414); both backends build clean.

## Verify
```
cargo build -p crush-lang-js --no-default-features --features swc-backend
cargo build -p crush-lang-js --features boa-backend
cargo run  -p crush-lang-js --bin js_walker -- some.ts     # CAST JSON (swc)
cargo test -p crush-lang-js
cargo test --workspace
```

## Gotchas
- **swc dep surface:** `swc_ecma_parser`/`swc_ecma_ast` pull `swc_common` (source maps,
  interning). Use just the parser+ast crates; don't pull the full `swc` bundler.
- **swc AST is verbose** and versions fast — pin the swc crates and keep the lowerer
  tolerant of nodes you intentionally reject (route to FeatureReport, don't panic).
- **Boa version drift:** boa crates version in lockstep with `boa_engine`; match
  surfer's 0.21 (`/workspace/projects/surfer-browser/Cargo.lock`).
- **bincode + serde_json::Value** (seen in `crush-pkg`): bincode can't deserialize
  `serde_json::Value` — don't stash arbitrary JSON in bincode-serialized structs.
- Boot with `dejavue context`; capture the lowering-boundary + backend-routing
  decision with `dejavue decision` once settled.

## References
- Pattern: `crates/crush-lang-python/` (rustpython) and `crates/crush-lang-rust/` (syn)
- Design rationale: `docs/design/crushvm-rustpython.md` (three-lane model; "one VM, frontends provide syntax")
- Arc context: `workspace-meta/FOREMAN_THREADS.md` → "🟨 Crush as surfer's first-class scripting language"
