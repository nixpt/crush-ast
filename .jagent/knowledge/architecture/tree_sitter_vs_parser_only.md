# Tree-Sitter vs Native Parser: Polyglot Frontend Comparison

## Overview

The crush-ast codebase contains **two coexisting architectures** for transforming source code into CAST:
1. **Tree-sitter walkers** (`Walker` trait) — used by `go_walker`, `c_walker`, `zig_walker`
2. **Native parser frontends** (`Frontend` trait) — used by `crush-lang-python`, `crush-lang-js`

Both produce the same output (`crush_cast::Program`), but differ fundamentally in parsing strategy, type safety, dependency footprint, and ergonomics.

---

## 1. The Traits

### `Walker` trait (tree-sitter coupled)
```rust
// in walker-core/src/lib.rs
pub trait Walker {
    fn language(&self) -> tree_sitter::Language;
    fn walk(&self, tree: &tree_sitter::Tree, source: &[u8]) -> Result<ast::Program>;
}
```

**Key constraint:** The trait signature binds implementors to `tree_sitter::Tree` and `&[u8]`. You cannot implement `Walker` without tree-sitter. `BaseWalker` provides shared helpers operating on raw byte ranges.

### `Frontend` trait (parser agnostic)
```rust
pub trait Frontend {
    fn language_name(&self) -> &'static str;
    fn file_extensions(&self) -> &[&'static str];
    fn parse(&self, source: &str) -> Result<Box<dyn Any>>;
    fn analyze(&self, ast: &Box<dyn Any>) -> Result<FeatureReport>;
    fn lower(&self, ast: Box<dyn Any>) -> Result<Program>;
}
```

**Key property:** Uses `Box<dyn Any>` to transport language-specific ASTs. No coupling to any parser framework. Adds a `FeatureReport` analysis step between parse and lower — enabling pre-flight safety checks.

---

## 2. Concrete Pattern Comparison

### Tree-Sitter Walker (go_walker — 298 lines)

```
Parser::new() → set_language(ts_go::LANGUAGE) → parse(source) → Walker::walk(&tree, bytes)
```

Lowering is **CST-based string matching**:
```rust
match node.kind() {
    "function_declaration" => {           // ← string literal
        let name = base.child_text(node, "name")?;
        // ... navigate by field names, also strings
    }
    "short_variable_declaration" => { ... }
    "call_expression" => {
        let func_name = base.text(func_node)?;
        if let Some(cap_name) = walker_core::map_to_capability("go", func_name) { ... }
    }
    ...
}
```

Advantages:
- **Build deps are light**: `tree-sitter` + one grammar crate per language (`tree-sitter-go`, `tree-sitter-c`, `tree-sitter-zig`)
- **Any language with a grammar**: tree-sitter has 100+ grammars — adding a new language is ~200-600 lines
- **CST fidelity**: raw parse tree captures comments, whitespace, formatting (if needed)

Disadvantages:
- **No compile-time safety**: `node.kind()` is a `&str` — typos, grammar upgrades that rename node kinds are silent runtime failures
- **Flat dispatch**: no destructuring — must manually navigate children via `child_by_field_name()`, `child(N)`, etc.
- **No semantic analysis**: no `FeatureReport` equivalent — capability detection is ad hoc during walking
- **Byte-level source access**: works with `&[u8]` instead of `&str`, requires conversion

### Native Parser — Python (crush-lang-python — 578 lines lowering)

```
rustpython_parser::Suite::parse(source) → Frontend::lower() via lower_stmt()/lower_expr()
```

Lowering is **typed enum pattern matching**:
```rust
match stmt {
    py_ast::Stmt::FunctionDef(py_ast::StmtFunctionDef {
        name, args, body, ..
    }) => {
        let params = args.args.iter()
            .map(|a| (a.def.arg.to_string(), CastType::Any))
            .collect();
        // ... typed field access via named struct fields
    }
    py_ast::Stmt::Assign(py_ast::StmtAssign { targets, value, .. }) => { ... }
    py_ast::Stmt::Return(py_ast::StmtReturn { value, .. }) => { ... }
}
```

### Native Parser — JavaScript (crush-lang-js — 2,369 lines lowering, dual-backend)

```
swc_ecma_ast::Module (primary, 1451 lines)
    or
boa_parser → BoaAst (optional, feature-gated, 918 lines)
```

Both use Rust enum destructuring:
```rust
match item {
    ModuleItem::Stmt(Stmt::Decl(Decl::Fn(fn_decl))) => {
        let name = fn_decl.ident.sym.to_string();
        // ... typed access via swc_ecma_ast::FnDecl
    }
    ModuleItem::Stmt(Stmt::Decl(Decl::Class(class_decl))) => { ... }
    ModuleItem::Stmt(Stmt::Decl(Decl::Var(var_decl))) => { ... }
}
```

Advantages (native parser):
- **Compile-time exhaustiveness**: Rust's `match` ensures you handle all variants (or use `_ =>` explicitly)
- **Smarter ASTs**: parsers resolve names, types, syntactic sugar — richer trees than CST
- **Destructuring**: direct field access by name, no `child_by_field_name()` navigation
- **Rich analysis**: `FeatureReport` pre-checks imports, detects dangerous features before lowering
- **String source**: works with `&str`, convenient for text processing

Disadvantages:
- **Heavy deps**: swc pulls in ~100+ crates (entire JS toolchain); boa has icu_* dependency conflicts
- **Coverage gaps**: lang-specific — Python lowerer doesn't support comprehensions, generators, async; JS lowerer may miss modern syntax
- **No CST access**: lose formatting/whitespace info if needed
- **New language = new dep**: each language needs a Rust parser crate to exist

---

## 3. Capability Detection Comparison

| Approach | Method | Example |
|---|---|---|
| Tree-sitter (go_walker) | Centralized `map_to_capability()` in walker-core | `map_to_capability("go", "fmt.Println")` → `Some("io.print")` |
| Native parser (Python) | Inline during lowering + pre-analyze imports | `lower_call()` checks func name, analyzer flags `import os` as dangerous |
| Native parser (JS/swc) | `analyzer::analyze_item()` for FeatureReport + inline in lowering | Dual-path: pre-analysis for imports, inline for call expressions |

The `Frontend` trait adds a separate `analyze()` pass — this is impossible in the tree-sitter `Walker` path because the trait only has `walk()`.

---

## 4. The Subprocess Bridge

Both architectures connect to the same subprocess dispatch pattern in exosphere:

```
WalkerRegistry (exosphere)
    → LanguageWalker trait (not tree-sitter bound)
        → SubprocessWalker (spawns crush-lang-js, crush-lang-python binaries)
            → These binaries implement Frontend, output CAST JSON on stdout
```

The `go_walker`, `c_walker`, `zig_walker` binaries output CAST JSON too — but they implement `Walker` (tree-sitter) internally. From the subprocess dispatch perspective, the internal architecture is invisible — only the JSON protocol matters.

---

## 5. Gap Analysis

### Known gap: No `TreeSitterFrontend` adapter

There is no `Frontend` implementation that wraps a `Walker` to provide the `frontend_pipeline()`. This means:

- Languages with only tree-sitter grammars (Go, C, Zig) cannot use `frontend_pipeline()`
- They skip the `analyze()` / `FeatureReport` step entirely
- The `Walker` trait remains the only path for these languages
- A `TreeSitterFrontend` wrapper would unify the architecture:

```rust
struct TreeSitterFrontend<W: Walker> {
    walker: W,
}

impl<W: Walker> Frontend for TreeSitterFrontend<W> {
    fn parse(&self, source: &str) -> Result<Box<dyn Any>> {
        let mut parser = Parser::new();
        parser.set_language(&self.walker.language())?;
        let tree = parser.parse(source, None)?;
        Ok(Box::new((tree, source.to_string())))
    }
    fn analyze(&self, _ast: &Box<dyn Any>) -> Result<FeatureReport> {
        // Tree-sitter walkers don't do pre-analysis — return default
        Ok(FeatureReport { lang: /* ... */, ..Default::default() })
    }
    fn lower(&self, ast: Box<dyn Any>) -> Result<Program> {
        let (tree, source) = *ast.downcast::<(Tree, String)>()?;
        self.walker.walk(&tree, source.as_bytes())
    }
}
```

### Gap resolved: `LowerCtx` for source positions (2026-06-24)

The `walker-core` crate now provides:
- `source_meta(file, lang, line, column)` — creates position metadata matching `BaseWalker::create_meta()`
- `byte_offset_to_line_col(source, offset)` — byte offset → 1-based (line, column)
- `LowerCtx` — context struct holding source/file/lang with a `meta_at(offset)` method

The Python, JavaScript (swc + boa), Rust, and Bash frontends now populate source position metadata using `LowerCtx`. The Zsh frontend still needs the update.

---

## 6. Comparison Summary

| Dimension | Walker (tree-sitter) | Frontend (native parser) |
|---|---|---|
| **Parsing** | `tree_sitter::Parser` (C grammar) | Rust parser crate (pure Rust) |
| **AST type** | `tree_sitter::Tree` (untyped CST) | Typed enums (e.g., `py_ast::Stmt`) |
| **Walking** | `node.kind()` string matching | Rust enum destructuring |
| **Type safety** | None — strings at runtime | Full — compiler-checked variants |
| **Feature analysis** | None built-in | `FeatureReport` via `analyze()` |
| **Build deps** | Light (tree-sitter + 1 grammar) | Heavy (swc, boa, rustpython-parser) |
| **Lines per language** | ~200-600 | ~600-2,400 (more coverage) |
| **Source metadata** | Position via `create_meta()` | None (empty HashMap) |
| **Capability mapping** | Centralized `map_to_capability()` | Inline + centralized |
| **Existing languages** | Go, C, Zig | Python, JavaScript/TypeScript |
| **Best for** | Languages w/o Rust parser | Languages with mature Rust parser |

---

## 7. Recommendation

For the polyglot consolidation strategy (exosphere → single NanoVM):

1. **Keep tree-sitter for Go/C/Zig** — no mature Rust parsers exist for these in the workspace, and the grammar-crate approach is the fastest path to CAST support.

2. **Add `TreeSitterFrontend` adapter** — wraps any `Walker` impl as a `Frontend`, so all languages go through `frontend_pipeline()` and get `FeatureReport` checks. ~50 lines, unblocks consistent security analysis.

3. **Add source metadata to `Frontend` lowering** — either thread an optional position provider through, or require frontends to populate `meta` with line/column. ~1 line per AST node (trivial but pervasive).

4. **Migrate JS/Python to be the primary lowering path** (already the plan in `polyglot-runtime-consolidation.md`). These have the richest ASTs and most complete lowering. Keep tree-sitter as fallback for unsupported constructs.

5. **Consider `syn` for Rust lowering** — already planned in crush-ast TODO, would give Rust the same AST-level safety as JS/Python.

---

## References

- `crates/walker-core/src/lib.rs` — `Walker` and `Frontend` trait definitions
- `crates/go_walker/src/main.rs` — tree-sitter walker example (298 lines)
- `crates/c_walker/src/main.rs` — tree-sitter walker example (604 lines)
- `crates/zig_walker/src/main.rs` — tree-sitter walker example (516 lines)
- `crates/crush-lang-python/src/` — native parser Python frontend (578 lines lowering)
- `crates/crush-lang-js/src/` — native parser JS frontend (2,369 lines lowering)
- `crates/tree-sitter-crush/grammar.js` — custom Crush grammar (400 lines)
