# Walker authoring — adding a source language that lowers to CAST

**Status:** design note, 2026-10-07 (CRUSH-173). Written from `crates/crush-walker-core` and the
existing walker crates. `crates/crush-walker-core/README.md` still documents only the older
tree-sitter `Walker` trait and its example no longer compiles — use this note instead.

A *walker* (the code calls new ones *frontends*) turns a foreign source file into a `crush_cast::Program`.
Everything after that — semantics, compiler, CVM1, AOT — is shared, so a walker is the whole cost of
adding a language.

## The traits (`crates/crush-walker-core/src/lib.rs`)

**`Frontend`** — the one to implement for a new language. Five required methods and one default:

```rust
pub trait Frontend: LanguageAdapter {
    fn language_name(&self) -> &'static str;
    fn file_extensions(&self) -> &[&'static str];
    fn parse(&self, source: &str) -> anyhow::Result<Box<dyn Any>>;        // your parser's AST, opaque
    fn analyze(&self, ast: &Box<dyn Any>) -> anyhow::Result<FeatureReport>;
    fn lower(&self, ast: Box<dyn Any>) -> anyhow::Result<crush_cast::Program>;
    fn walk(&self, source: &str, filename: &str) -> anyhow::Result<(FeatureReport, Program)> { /* parse → analyze → lower */ }
}
```

- `parse` may use any parser — the existing native frontends use `rustpython-parser` (Python), `syn`
  (Rust), `swc` (JS/TS). To get source positions into `lower`, return the source together with the
  AST (`Box::new((source.to_string(), ast))`) and build a `LowerCtx` from it.
- `analyze` fills a `FeatureReport` (dangerous imports, `unsafe`, FFI, classes, async, …);
  `can_lower_safely()` is false when the program uses something a sandbox should refuse.
- A blanket impl gives every `Frontend + Send + Sync` a `LanguageAdapter`, so you write no adapter.
- The default `walk` ignores `filename`; override it if `meta` should carry the real file name.

**`LanguageAdapter`** — what the registry stores: `language_name`, `file_extensions`, `walk`, and a
default `can_handle(ext)`. Implement it by hand only when wrapping a parser that can't fit `Frontend`.

**`Walker`** (feature `tree-sitter`, on by default) — the older trait: `language() ->
tree_sitter::Language` + `walk(&Tree, &[u8])`. Go, C, Zig, Dart and Java are still written this way;
`TreeSitterFrontend<W: Walker>` adapts one to `Frontend`, and `BaseWalker` has node-text and
`create_meta` helpers. Not deprecated, but new languages should not start here.

(`crush-frontend` has a fourth, `LanguageWalker`, used by its `WalkerRegistry`; see below.)

## How a walker gets called

There is no plugin discovery. Three paths exist; only the first runs in shipped binaries:

1. **In-process `AdapterRegistry`** — `crates/crush-aot/src/bin/walk_run.rs` (`crush-walk-run`) and
   `aotc.rs` (`crush-aotc`) each build a registry and `.register(...)` 11 adapters by hand.
   `AdapterRegistry::walk(source, filename)` picks the first adapter whose `can_handle` matches the
   file's extension. **Adding a language = a dependency in `crates/crush-aot/Cargo.toml` + a
   `.register` line in both binaries** (and the extension in `aotc.rs`'s `load_casm_program` list).
2. **A walker binary** — each crate also ships `<lang>_walker`, which takes one file path and prints
   the CAST as JSON on stdout; failure = non-zero exit + message on stderr. Tree-sitter crates do this
   with `run_walker_binary(...)` in `src/main.rs`; native ones in `src/bin/walker.rs`. The `walker`
   CLI (`crates/cli`) dispatches to these by extension.
3. **`SubprocessWalker` / `WalkerRegistry`** (`crates/crush-frontend/src/language_walkers.rs`) —
   spawns a walker binary (temp file in, JSON out; binary found on `PATH`, `~/.cargo/bin`,
   `~/.local/bin`). Only tests use it today.

`crushc` and `crush-run` take Crush source only; walkers are not on their path. Their only link to a
walker crate is `@python { }` / `@javascript { }` free-variable analysis (features `polyglot-python` /
`polyglot-javascript` in crush-lang-sdk).

**Extension trap:** `AdapterRegistry` compares against `Path::extension()`, which has no dot. Adapters
must list `["py", "pyw"]`, not `[".py"]`. Some `Frontend` impls list dotted extensions and only work
because a differently-declared adapter is what gets registered. The extension → language mapping is
currently duplicated in four tables (adapters, `aotc.rs`, `crates/cli`, `WalkerRegistry`); keep them
in sync until they are merged.

## Lowering conventions

Look at `crates/crush-lang-python` (native, `Frontend`) and `crates/crush-lang-go` (tree-sitter,
`Walker`) side by side.

- **Program shape:** top-level function definitions go into `Program.functions`; every other
  top-level statement goes into a synthesized `main`, and `entry = "main"`. Set `lang`.
- **Locations:** put line/column/file/lang in each node's `meta` (`LowerCtx::meta_at`,
  `BaseWalker::create_meta`) — diagnostics and the debugger read them.
- **Library calls → capabilities:** map the language's I/O (`print`, `fmt.Println`, …) to
  `Expression::CapabilityCall` / named capability calls such as `io.print`, never to an ambient host
  call. What the program can do is then decided by the grants at run time, the same as for Crush.
- **Unsupported constructs — fail loudly.** The Python frontend `bail!`s with the construct's name
  (`class`, `with`, `global`, lambdas, f-string interpolation, …). The Go walker silently drops unknown
  statements and turns unknown expressions into `null`; tree-sitter is error-tolerant, so even
  non-Go input "succeeds". Prefer the Python behaviour: a program that compiles but silently lost a
  statement is worse than one that doesn't compile. A `LangBlock` (run the original code via
  `EXEC_LANG`) is a legitimate fallback only when the block can run as the guest language (crush-lang-js
  uses it this way).

## Crate checklist

- `crates/crush-lang-<lang>/`, added to the root `members`.
- `version.workspace = true`, `license.workspace = true`, `repository.workspace = true`; internal deps
  via `.workspace = true` (path + version — a dep without `version` cannot publish).
- `[[bin]] name = "<lang>_walker"`. Keep `crate-type = ["lib"]` (see the crate-type invariant in
  `.dejavue/invariants.md`).
- If the frontend should build for wasm32, depend on `crush-walker-core` with
  `default-features = false` (drops tree-sitter) and use a pure-Rust parser.
- Tests: unit tests on CAST shape *and* an end-to-end test — CAST → `crush_frontend::compile_cast` →
  `crush_lang_sdk::compile::casm_to_vm` → `crush_vm::run_with_caps` — asserting the program's output
  (`crates/crush-lang-python/tests/pipeline_test.rs` is the model). Add a test that an unsupported
  construct is refused with a useful message.
- Register in `crush-walk-run` / `crush-aotc` (above) and add the binary name to the tables the
  binary-name tests check (`crates/crush-frontend/tests/walker_binaries.rs`, `crates/cli`).
