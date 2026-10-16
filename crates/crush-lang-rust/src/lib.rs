//! crush-lang-rust — Rust language support for Crush.
//!
//! Uses `syn` to parse Rust source and lower it to CAST.
//!
//! # Supported subset
//!
//! Free functions (block tail expressions are the return value), `let` /
//! `let mut`, `if`/`else` (also as expressions), `while`, `loop`, `for` over
//! `a..b` / `a..=b` / fresh sequences or direct `.iter()`, `break`/`continue`, `return`, `match` with literal,
//! binding, `_` and `|` patterns, `vec![..]`, indexing, synchronous closures
//! with identifier parameters (including typed parameters and `_` wildcards),
//! `+= -= *= /= %=`,
//! `println!`/`format!` with Display `{}`, `{0}`, `{name}` and typed literal
//! Debug `{:?}` (dynamic Debug is refused),
//! and the common `str`/`Vec`/numeric methods (`len`, `push`, `contains`,
//! `to_uppercase`, `abs`, `sqrt`, …). The SDK's `rustc_corpus` integration suite
//! compares this subset against real `rustc` output.
//!
//! Not supported — rejected with an error rather than mistranslated: structs,
//! enums, traits and `impl` blocks, match guards and range patterns, format
//! width/precision (`{:.2}`, `{:>5}`), `vec![x; n]`, move/async closures,
//! destructured closure parameters, iterator chains (`.map`,
//! `.sum`, …), `use` declarations and qualified module/crate paths. Rust crate
//! APIs need an explicit Crush capability/plugin; a Rust `use` is not treated
//! as a runtime import. `print!`, `eprint!`, and `eprintln!` are refused until
//! their newline/channel routing is faithful. Potentially effectful format
//! arguments need one use each, in source order, without inline captures.
//! Multi-statement blocks are supported as function/closure bodies and
//! statement blocks. In a value position (e.g. `let x = { ... }` or branches
//! of an assigned `if` expression), only empty blocks or a single tail
//! expression are supported; other forms are rejected with a source location.
//!
//! Closures can read implicit scalar captures through shared cells. Rust
//! lexical bindings have distinct storage identities (CRUSH-xps-6). Rust
//! flat sequence clones create fresh values (CRUSH-xps-7). Implicit owned binding
//! transfers, nested/unknown clones, owned captures, escaping borrowed views, and
//! `pop`/`unwrap` are rejected with source locations. Fresh values and supported
//! clones can enter known owned parameters; String clones and local `+=` retain
//! value semantics. This is not a Rust type/borrow checker or full closure-trait
//! implementation. Nested functions are rejected until
//! block-local item resolution is implemented.
//! Integer casts use canonical binding/range proofs and one shared CASM expansion.
//! Small u64 values and portable bounded usize conversions are supported;
//! unproven producers, mutable lengths and closure casts are refused.
//! Numeric suffixes and annotations are retained as source hints; this does not
//! implement type inference or typed arithmetic. Runtime integer overflow
//! still has a known silent divergence tracked by CRUSH-xps-8. Typed literal
//! Debug is exact; dynamic Debug is refused pending the shared formatter.

pub mod lower_expr;
pub mod lower_stmt;
pub mod parser;
mod bindings;
mod formatting;
mod diagnostics;
mod numeric;
mod values;

use std::any::Any;
use std::collections::HashMap;

use crush_cast::{Function, Program, Statement};
use crush_walker_core_no_tree_sitter::{FeatureReport, Frontend, LowerCtx};
use quote::ToTokens;
use syn::spanned::Spanned;

pub struct RustFrontend;

impl Frontend for RustFrontend {
    fn language_name(&self) -> &'static str {
        "rust"
    }
    fn file_extensions(&self) -> &[&'static str] {
        &[".rs"]
    }

    fn parse(&self, source: &str) -> anyhow::Result<Box<dyn Any>> {
        let file = parser::parse_source(source)?;
        Ok(Box::new((source.to_string(), file)))
    }

    fn analyze(&self, ast: &Box<dyn Any>) -> anyhow::Result<FeatureReport> {
        let (_, file) = ast
            .downcast_ref::<(String, syn::File)>()
            .ok_or_else(|| anyhow::anyhow!("expected (String, syn::File)"))?;
        let mut r = FeatureReport::default();
        r.lang = "rust".to_string();
        for item in &file.items {
            match item {
                syn::Item::Fn(_) => r.uses_functions = true,
                syn::Item::Struct(_) | syn::Item::Impl(_) | syn::Item::Trait(_) => {
                    r.uses_classes = true;
                }
                syn::Item::Use(_) | syn::Item::ExternCrate(_) => {
                    r.uses_imports.push(format!("{:?}", item));
                }
                syn::Item::ForeignMod(..) => r.uses_ffi = true,
                _ => {}
            }
            r.estimated_complexity += 1;
        }
        Ok(r)
    }

    fn lower(&self, ast: Box<dyn Any>) -> anyhow::Result<Program> {
        let (source, file) = *ast
            .downcast::<(String, syn::File)>()
            .map_err(|_| anyhow::anyhow!("expected (String, syn::File)"))?;
        file_to_cast(file, &source)
    }
}

/// Parse Rust source and lower to CAST (convenience wrapper).
pub fn rust_to_cast(source: &str) -> anyhow::Result<Program> {
    let (_, program) = crush_walker_core_no_tree_sitter::frontend_pipeline(&RustFrontend, source)?;
    Ok(program)
}

fn file_to_cast(file: syn::File, source: &str) -> anyhow::Result<Program> {
    let ctx = LowerCtx::new(source, "<crush>", "rust");
    let mut main_body = Vec::new();
    let mut functions: HashMap<String, Function> = HashMap::new();

    for item in &file.items {
        match item {
            syn::Item::Fn(_) => {
                if let syn::Item::Fn(f) = item {
                    anyhow::ensure!(f.sig.ident != numeric::CAST,
                        "unsupported Rust function at {}:{}: reserved Rust numeric compiler helper (CRUSH-xps-8)",
                        f.sig.ident.span().start().line, f.sig.ident.span().start().column + 1);
                }
                let lowered = lower_stmt::lower_stmt(&syn::Stmt::Item(item.clone()), &ctx)?;
                if let Statement::FunctionDef {
                    name, params, body, meta
                } = lowered
                {
                    functions.insert(
                        name,
                        Function {
                            params,
                            body,
                            meta,
                            ..Default::default()
                        },
                    );
                }
            }
            // A `use`, `struct`, or other Rust item cannot be represented by
            // this expression-only CAST surface. Do not turn it into a null
            // statement: that made imported crate calls look as if they had
            // been accepted, then fail later or silently do nothing.
            _ => anyhow::bail!(
                "unsupported Rust top-level item `{}` at {}:{}; Rust imports and crate APIs need an explicit Crush capability/plugin; type declarations need COSM walker support",
                item.to_token_stream(), item.span().start().line, item.span().start().column + 1
            ),
        }
    }

    if !main_body.is_empty() {
        functions.insert(
            "main".to_string(),
            Function {
                params: vec![],
                body: main_body,
                meta: HashMap::new(),
                ..Default::default()
            },
        );
    }

    let mut program = Program {
        cast_version: "0.2".to_string(),
        entry: "main".to_string(),
        lang: Some("rust".to_string()),
        functions,
        ai_meta: None,
        ..Default::default()
    };
    bindings::resolve(&mut program, source)?;
    numeric::check(&mut program)?;
    values::check(&mut program)?;
    Ok(program)
}

#[cfg(test)]
mod tests {
    //! CRUSHRUST-1: inline parity test suite — adapted from
    //! `crates/crush-lang-python/tests/{frontend_test,pipeline_test}.rs`
    //! for the syn-lowering path. Three representative tests:
    //! (1) Rust-specific feature detection (struct + extern),
    //! (2) import detection parity,
    //! (3) safe-code path-through via `rust_to_cast`.
    //!
    //! Tests use the *function-call form* `println(x)`; the macro form
    //! `println!` is covered by `tests/rustc_corpus_test.rs` (CRUSH-197).

    use super::{rust_to_cast, RustFrontend};
        use crush_walker_core_no_tree_sitter::Frontend;

    /// Mirror of python's `test_analyze` helper: frontend.parse ->
    /// frontend.analyze on raw rust source.
    fn test_analyze(source: &str) -> crush_walker_core_no_tree_sitter::FeatureReport {
        let frontend = RustFrontend;
        let ast = frontend.parse(source).unwrap();
        frontend.analyze(&ast).unwrap()
    }

    /// Test 1 (mirrors python's
    /// `test_python_frontend_detects_classes_and_async`): exercises
    /// Rust-specific feature paths — `struct Foo` flips
    /// `uses_classes`; `extern "C" { ... }` flips `uses_ffi`; absence
    /// of any top-level `fn` keeps `uses_functions` false.
    #[test]
    fn test_rust_analyze_classes_and_ffi() {
        let report = test_analyze(
            "struct Foo { x: i32 }\nextern \"C\" { fn bar(); }\n",
        );
        assert!(
            report.uses_classes,
            "expected `struct Foo` to flip uses_classes (rust-native, not python's class)"
        );
        assert!(
            report.uses_ffi,
            "expected `extern \"C\" {{ ... }}` to flip uses_ffi"
        );
        assert!(
            !report.uses_functions,
            "no top-level `fn` in source; uses_functions must stay false"
        );
    }

    /// Test 2 (mirrors python's `test_frontend_detects_imports`):
    /// two `use` statements populate `uses_imports` with 2 entries
    /// (one per AST item, debug-formatted). Both go through
    /// `syn::Item::Use` — we deliberately avoid `extern crate`
    /// because `Item::ExternCrate` carries `#[deprecated]` in
    /// syn 2.0 and would invite an unrelated lint failure later.
    #[test]
    fn test_rust_analyze_imports() {
        let report = test_analyze(
            "use std::collections::HashMap;\nuse std::io::Result;\n",
        );
        assert_eq!(
            report.uses_imports.len(),
            2,
            "two imports expected, found: {:?}",
            report.uses_imports
        );
        assert!(!report.uses_functions);
        assert!(!report.uses_classes);
        assert!(!report.uses_ffi);
    }

    /// CRUSH-36 Commit 1: regression-resistance for the Frontend ->
    /// LanguageAdapter migration (already landed via the macro at the
    /// bottom of this file). Without this test, the
    /// `impl_adapter_from_frontend!` line could be deleted and the crate
    /// would still compile against `Frontend` alone, silently losing
    /// its unified-registry registration.
    ///
    /// Note: this test scopes `use super::RustAdapter;` because rust's
    /// existing `mod tests` top-level use statement is specific imports
    /// (`use super::{rust_to_cast, RustFrontend};`), not glob -- unlike
    /// nepali/bash/custom which all use `use super::*;` (auto-resolves
    /// the macro-generated adapter without a scoped fn-body import).
    /// `rust` is the lone fn-body-scoped exception; the scoped-add is
    /// minimal: one struct-name addition inside this fn body.
    #[test]
    fn rust_adapter_registers_in_unified_registry() {
        use super::RustAdapter;
        use crush_walker_core_no_tree_sitter::AdapterRegistry;
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(RustAdapter));
        let langs = registry.languages();
        assert!(
            langs.contains(&"rust"),
            "RustAdapter must register with name 'rust', got: {:?}",
            langs
        );
    }

    /// Test 3 (mirrors python's `test_python_frontend_safe_code`):
    /// a small Rust program parses + lowers to a CAST Program whose
    /// `functions` map contains `main`. The main body holds exactly
    /// 2 statements (the let-binding + the `println` call). Note the
    /// *function-call form* `println(x)` (no `!`, no arithmetic);
    /// see the module-level docstring for why. We deliberately
    /// avoid arithmetic here because the current `lower_expr` does
    /// not yet support binary operators (it returns
    /// `unsupported binary operator`); exercising that gap is out of
    /// scope for this test.
    #[test]
    fn test_rust_lower_safe_code() {
        let source = "fn main() {\n    let x = 42;\n    println(x);\n}\n";
        let program = rust_to_cast(source).unwrap();
        assert!(
            program.functions.contains_key("main"),
            "main function must be registered after lowering"
        );
        let main_body = &program.functions["main"].body;
        assert_eq!(
            main_body.len(),
            2,
            "two stmts expected in main body: let-binding + println call, got: {:?}",
            main_body
        );
    }
}

// ── Adapter ──────────────────────────────────────────────────────────────────

use crush_walker_core_no_tree_sitter::impl_adapter_from_frontend;

impl_adapter_from_frontend!(
    RustAdapter,
    "rust",
    &["rs"],
    crate::rust_to_cast
);
