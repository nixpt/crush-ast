//! # walker-core
//!
//! Base utilities and traits for implementing language walkers in CRUSH.
//!
//! This crate provides the foundational infrastructure for building language walkers
//! that transform source code from various programming languages into CRUSH's Abstract
//! Syntax Tree (CAST) format.
//!
//! ## Core Abstractions
//!
//! - [`Walker`]: Trait that all language walkers must implement
//! - [`BaseWalker`]: Utility struct with common tree-sitter operations
//! - [`Frontend`]: Parser-agnostic frontend trait (parse → analyze → lower)
//! - [`TreeSitterFrontend`]: Adapter wrapping a [`Walker`] as a [`Frontend`]
//! - [`LowerCtx`]: Context for populating source position metadata in CAST nodes
//! - [`source_meta`], [`byte_offset_to_line_col`]: Position helpers for frontends
//!
//! ## Implementing a Walker
//!
//! ```rust,ignore
//! // The example below requires a real tree-sitter grammar crate.
//! // Substitute `tree_sitter_yourlang` and `tree_sitter::Language` for
//! // your target language.
//! use crush_walker_core::{Walker, BaseWalker};
//! use crush_cast;
//! use anyhow::Result;
//!
//! struct MyLangWalker;
//!
//! impl Walker for MyLangWalker {
//!     fn language(&self) -> tree_sitter::Language {
//!         todo!("return tree_sitter_yourlang::language()")
//!     }
//!
//!     fn walk(&self, tree: &tree_sitter::Tree, source: &[u8]) -> Result<crush_cast::Program> {
//!         let base = BaseWalker::new(source);
//!         let root = tree.root_node();
//!
//!         // Transform tree to CAST using base utilities
//!         let _meta = base.create_meta(root, "yourlang", "input.ext");
//!
//!         // ... build AST nodes ...
//!
//!         Ok(crush_cast::Program {
//!             cast_version: "0.2".to_string(),
//!             entry: "main".to_string(),
//!             lang: Some("yourlang".to_string()),
//!             functions: Default::default(),
//!             ai_meta: None,
//!         })
//!     }
//! }
//! ```

pub mod adapter;

use anyhow::{Context, Result};
use crush_cast::{self as ast, Program};
use serde_json::json;
use std::collections::HashMap;
use tree_sitter::Node;

// ── Frontend trait (replaces Walker for native-parser frontends) ─────────────

/// Features detected in source code before lowering to CAST.
#[derive(Debug, Default, Clone)]
pub struct FeatureReport {
    pub lang: String,
    pub uses_functions: bool,
    pub uses_classes: bool,
    pub uses_async: bool,
    pub uses_generators: bool,
    pub uses_exceptions: bool,
    pub uses_imports: Vec<String>,
    pub dangerous_imports: Vec<String>,
    pub uses_unsafe: bool,
    pub uses_ffi: bool,
    pub uses_meta_programming: bool,
    pub has_top_level_side_effects: bool,
    pub estimated_complexity: usize,
}

impl FeatureReport {
    pub fn can_lower_safely(&self) -> bool {
        self.dangerous_imports.is_empty() && !self.uses_unsafe && !self.uses_ffi
    }
}

/// A language frontend: parse, analyze, lower.
///
/// Replaces the tree-sitter-bound `Walker` trait for language implementations
/// that use native Rust parsers (rustpython-parser, syn, boa_parser, etc.).
///
/// ## Source position metadata
///
/// Frontends should populate CAST node `meta` with source position information
/// to enable source-mapped error messages. The recommended pattern:
///
/// 1. In [`parse()`](Frontend::parse), bundle the source string with the AST:
///    `Ok(Box::new((source.to_string(), ast)))`
/// 2. In [`lower()`](Frontend::lower), create a [`LowerCtx`] and pass it through
///    the lowering functions instead of using empty `HashMap::new()` for meta.
///
/// ```rust,ignore
/// use crush_walker_core::LowerCtx;
///
/// fn lower(&self, ast: Box<dyn Any>) -> Result<Program> {
///     let (source, stmts) = *ast.downcast::<(String, MyAst)>()?;
///     let ctx = LowerCtx::new(&source, "input.py", "python");
///     // ... lower with ctx, using ctx.meta_at(offset) for position metadata
/// }
/// ```
// CRUSH-36 Commit 2 Sub-Commit 1 (A->B):
// Supertrait tie `Frontend: LanguageAdapter` enables every `impl Frontend
// for X` to register in `AdapterRegistry` directly without a separate
// adapter wrapper struct (the macro `impl_adapter_from_frontend!` is no
// longer required for FE-side walker crates to participate in the unified
// registry). The default `walk` body wraps the existing `frontend_pipeline`
// so existing Frontend impls do NOT need a `walk` body -- they inherit this
// default. The 6 Frontend impls (bash, custom, nepali, python, rust, js)
// continue to compile unchanged: language_name + file_extensions + parse +
// analyze + lower are still declared here, walk is inherited from the default
// below (and that default satisfies `LanguageAdapter::walk` via the
// supertrait tie), and can_handle is inherited from `LanguageAdapter`'s own
// default. The `Send + Sync` bound propagates from `LanguageAdapter: Send +
// Sync`; all 6 existing impls are simple structs (unit or struct-of-strings)
// and auto-derive Send + Sync.
pub trait Frontend: LanguageAdapter {
    fn language_name(&self) -> &'static str;
    fn file_extensions(&self) -> &[&'static str];

    /// Parse source text into a language-specific AST (opaque).
    fn parse(&self, source: &str) -> Result<Box<dyn std::any::Any>>;

    /// Analyze the parsed AST for features and capability requirements.
    fn analyze(&self, ast: &Box<dyn std::any::Any>) -> Result<FeatureReport>;

    /// Lower the parsed AST to a CAST Program.
    fn lower(&self, ast: Box<dyn std::any::Any>) -> Result<Program>;

    /// Default walk for any Frontend: parse -> analyze -> lower inline.
    /// Override only if a custom filename transformation is needed (e.g.
    /// CLI path-aware lowering).
    ///
    /// ## Why inlined (and not calling `frontend_pipeline(self, source)`)
    ///
    /// The free function `frontend_pipeline(frontend: &dyn Frontend, ...)`
    /// takes `&dyn Frontend` -- at the trait-body default-implementation site
    /// this would force a `&Self -> &dyn Frontend` coercion, which only
    /// succeeds when `Self: Sized` is in scope. Adding `where Self: Sized`
    /// would remove `walk` from the trait-object vtable, breaking every
    /// `Box<dyn LanguageAdapter>::walk(...)` call path (including
    /// `AdapterRegistry::walk`). The inlined body is functionally equivalent
    /// to `frontend_pipeline(self, source)` but coerces via Self-sized
    /// method calls instead. Future reviewers: this inlining is intentional,
    /// not a missed abstraction.
    fn walk(&self, source: &str, _filename: &str) -> anyhow::Result<(FeatureReport, Program)> {
        let ast = self.parse(source)?;
        let report = self.analyze(&ast)?;
        let program = self.lower(ast)?;
        Ok((report, program))
    }
}

// CRUSH-36 Commit 2 Sub-Commit 1: this blanket impl is the architectural
// closure of the supertrait-tie. Without it, the `: LanguageAdapter`
// supertrait clause on `pub trait Frontend: LanguageAdapter` would require
// EVERY `impl Frontend for X` to ALSO have an explicit per-type
// `impl LanguageAdapter for X`. Without that per-type LanguageAdapter
// impl, the `impl Frontend for X` block fails the impl-site supertrait
// bound check (Rust verifies at impl site that the target type satisfies
// the supertrait bound).
//
// The blanket `impl<T: Frontend + Send + Sync> LanguageAdapter for T`
// covers all concrete Frontend impls, so a separate per-type LanguageAdapter
// impl is NOT needed. The blanket dispatches to Frontend's required methods
// `language_name` + `file_extensions` via UFCS (`<Self as Frontend>::...`)
// and Frontend's inlined default `walk` body -- which delegates to
// parse/analyze/lower via `self.method()` dispatch -- satisfies
// `LanguageAdapter::walk` end-to-end.
//
// Coherence: the blanket applies ONLY to T: Frontend + Send + Sync. The 6
// existing Frontend impls (`BashFrontend`, `CustomFrontend`,
// `NepaliFrontend`, `PythonFrontend`, `RustFrontend`, `JsFrontend`) are
// simple structs of `{ ... }` that auto-implement Send + Sync, so the
// blanket covers them. Macro-generated wrapper adapters (`PythonAdapter`,
// `RustAdapter`, etc.) are SEPARATE types that already have explicit
// `impl LanguageAdapter for <Adapter>` -- they are NOT Frontend impls, so
// the blanket does NOT apply to them. No coherence conflict.
//
// Send + Sync propagation: `LanguageAdapter: Send + Sync` requires
// `T: Send + Sync` in the blanket bound. For a Frontend type to satisfy
// this via the blanket, the concrete Frontend type itself must be Send +
// Sync (auto via struct-of-strings for all 6 existing impls; the blanket
// itself propagates the Send + Sync requirement to all future Frontend
// impls and is the canonical closure of the cascade).
//
// Future crates (CRUSH-37 Java, CRUSH-38 Kotlin, CRUSH-43+) need
// `use anyhow::Result;` at file scope (per the cross-scope alias-discipline
// diagnostic note) AND should NOT need to write any per-type LanguageAdapter
// impl -- the blanket handles it.
impl<T: Frontend + Send + Sync> LanguageAdapter for T {
    fn language_name(&self) -> &'static str {
        <Self as Frontend>::language_name(self)
    }
    fn file_extensions(&self) -> &[&'static str] {
        <Self as Frontend>::file_extensions(self)
    }
    fn walk(&self, source: &str, filename: &str) -> anyhow::Result<(FeatureReport, Program)> {
        <Self as Frontend>::walk(self, source, filename)
    }
    // can_handle inherits the default from the LanguageAdapter trait body
    // (extends on file_extensions()); no need to override here.
}

/// Run the full frontend pipeline: parse → analyze → lower.
pub fn frontend_pipeline(
    frontend: &dyn Frontend,
    source: &str,
) -> Result<(FeatureReport, Program)> {
    let ast = frontend.parse(source)?;
    let report = frontend.analyze(&ast)?;
    let program = frontend.lower(ast)?;
    Ok((report, program))
}

/// Auto-detect frontend by file extension.
pub fn frontend_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "py" | "pyi" => Some("python"),
        "rs" => Some("rust"),
        "js" | "jsx" | "mjs" => Some("javascript"),
        "ts" | "tsx" => Some("typescript"),
        "sh" | "bash" => Some("bash"),
        "go" => Some("go"),
        "c" | "h" | "cpp" | "cc" | "cxx" | "c++" | "hpp" => Some("c"),
        "zig" => Some("zig"),
        "wasm" => Some("wasm"),
        "sn" => Some("sona"),
        "np" | "nepali" => Some("nepcode"),
        _ => None,
    }
}

// ── TreeSitterFrontend adapter ──────────────────────────────────────────────

/// Adapter that wraps a tree-sitter [`Walker`] as a [`Frontend`].
///
/// This allows tree-sitter-based walkers (Go, C, Zig) to participate in the
/// `frontend_pipeline()` and receive `FeatureReport` checks. The walker's
/// [`Walker::walk()`] method is called directly — no subprocess overhead.
///
/// # Example
///
/// ```rust,ignore
/// use crush_walker_core::{TreeSitterFrontend, Walker, frontend_pipeline};
///
/// struct GoWalker { file_name: String }
/// impl Walker for GoWalker { /* ... */ }
///
/// let frontend = TreeSitterFrontend::new(GoWalker { file_name: "x.go".into() }, "go", &[".go"]);
/// let (report, program) = frontend_pipeline(&frontend, source)?;
/// ```
pub struct TreeSitterFrontend<W: Walker> {
    walker: W,
    language_name: &'static str,
    extensions: &'static [&'static str],
}

impl<W: Walker> TreeSitterFrontend<W> {
    /// Create a new `TreeSitterFrontend`.
    ///
    /// - `walker`: a `Walker` implementation for the target language
    /// - `language_name`: the language name (e.g. "go", "c", "zig") — many
    ///   tree-sitter grammars do not expose a name, so this must be provided
    /// - `extensions`: file extensions for this language (e.g. `&[".go"]`)
    pub fn new(walker: W, language_name: &'static str, extensions: &'static [&'static str]) -> Self {
        Self { walker, language_name, extensions }
    }

    pub fn extensions(&self) -> &'static [&'static str] {
        self.extensions
    }

    pub fn into_inner(self) -> W {
        self.walker
    }
}

// CRUSH-36 Commit 2 Sub-Commit 1: this existing impl<W: Walker> Frontend for
// TreeSitterFrontend<W> must add `+ Send + Sync` because Frontend:
// LanguageAdapter (subtrait tie added in this commit) propagates the Send +
// Sync bound from LanguageAdapter. Without `+ Send + Sync`, no concrete W
// can satisfy Frontend, and the existing call sites that construct
// TreeSitterFrontend<{XWalker}> would break at the trait-resolution step.
// All existing Walker types (GoWalker, CWalker, ZigWalker, DartWalker) are
// simple structs of `{ file_name: String, ... }` and auto-implement Send +
// Sync, so adding the bound is non-breaking at the call-site level.
impl<W: Walker + Send + Sync> Frontend for TreeSitterFrontend<W> {
    fn language_name(&self) -> &'static str {
        self.language_name
    }

    fn file_extensions(&self) -> &[&'static str] {
        self.extensions
    }

    fn parse(&self, source: &str) -> Result<Box<dyn std::any::Any>> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.walker.language())
            .map_err(|e| anyhow::anyhow!("Error setting language: {}", e))?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse source"))?;
        Ok(Box::new((tree, source.to_string())))
    }

    fn analyze(&self, _ast: &Box<dyn std::any::Any>) -> Result<FeatureReport> {
        Ok(FeatureReport {
            lang: self.language_name.to_string(),
            ..Default::default()
        })
    }

    fn lower(&self, ast: Box<dyn std::any::Any>) -> Result<Program> {
        let (tree, source) = *ast
            .downcast::<(tree_sitter::Tree, String)>()
            .map_err(|_| anyhow::anyhow!("expected (Tree, String) from TreeSitterFrontend::parse"))?;
        self.walker.walk(&tree, source.as_bytes())
    }
}

// CRUSH-36 Commit 2 Sub-Commit 1 (Fix #7 truncation): the prior
// `impl<W: Walker + Send + Sync> LanguageAdapter for TreeSitterFrontend<W>`
// block (added earlier in this commit as the Fix #2 supplementary bridge)
// was REDUNDANT after the Fix #6 blanket impl was added -- both impls
// applied to `TreeSitterFrontend<W>` and would have triggered Rust
// coherence error E0119 "conflicting implementations of trait
// `LanguageAdapter` for type `TreeSitterFrontend<_>`". With Fix #6's
// blanket `impl<T: Frontend + Send + Sync> LanguageAdapter for T` in
// place, any concrete T: Frontend + Send + Sync (including
// TreeSitterFrontend<W> via the existing
// `impl<W: Walker + Send + Sync> Frontend for TreeSitterFrontend<W>` block)
// automatically satisfies LanguageAdapter -- no per-type bridge needed.
//
// This truncation prefers the blanket-only pattern over a mixed
// blanket+per-type pattern: the blanket is the uniform architectural
// closure of `Frontend: LanguageAdapter`, and any future concrete type
// implementing Frontend gains LanguageAdapter via the blanket without
// needing to write a per-type impl.

/// Run a tree-sitter walker as a subprocess binary.
///
/// Reads source from `input_path`, parses with `walker`, and prints CAST JSON
/// to stdout. This is the standard entry point for all tree-sitter walker
/// binaries — every walker crate's `main()` should follow this pattern.
///
/// # Example (`main.rs` for a hypothetical Java walker)
///
/// ```rust,ignore
/// use clap::Parser;
/// use crush_walker_core::run_walker_binary;
///
/// #[derive(Parser)]
/// struct Cli { input: String }
///
/// fn main() -> anyhow::Result<()> {
///     let cli = Cli::parse();
///     run_walker_binary(
///         java_walker::JavaWalker { file_name: cli.input.clone() },
///         "java", &[".java"],
///         &cli.input,
///     )
/// }
/// ```
// CRUSH-36 Commit 2 Sub-Commit 1: this existing run_walker_binary<W: Walker>
// function must add `+ Send + Sync` because it constructs a
// TreeSitterFrontend<W> internally and forwards `&frontend` to
// `frontend_pipeline(&dyn Frontend, ...)`. Since Frontend: LanguageAdapter
// supertrait tie propagates Send + Sync, `&TreeSitterFrontend<W> ->
// &dyn Frontend` cast requires `TreeSitterFrontend<W>: Send + Sync`, which
// in turn requires `W: Send + Sync` (W is the held field). All existing
// Walker types (GoWalker, CWalker, ZigWalker, DartWalker) are simple structs
// of `{ file_name: String, ... }` and auto-implement Send + Sync, so adding
// the bound is non-breaking at existing call sites.
pub fn run_walker_binary<W: Walker + Send + Sync>(
    walker: W,
    language_name: &'static str,
    extensions: &'static [&'static str],
    input_path: &str,
) -> Result<()> {
    let source = std::fs::read_to_string(input_path)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", input_path, e))?;
    let frontend = TreeSitterFrontend::new(walker, language_name, extensions);
    let (_, program) = frontend_pipeline(&frontend, &source)?;
    println!("{}", serde_json::to_string_pretty(&program)?);
    Ok(())
}

// ── Legacy Walker trait (tree-sitter-based) ─────────────────────────────────

/// Errors emitted by language walkers.
#[derive(Debug, thiserror::Error)]
pub enum WalkerError {
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),
    #[error("Unsupported file extension: {0}")]
    UnsupportedExtension(String),
    #[error("Invalid filename: {0}")]
    InvalidFilename(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Walk error: {0}")]
    WalkError(String),
    #[error("IO error: {0}")]
    IoError(String),
}

pub trait Walker {
    fn language(&self) -> tree_sitter::Language;
    fn walk(&self, tree: &tree_sitter::Tree, source: &[u8]) -> Result<ast::Program>;
}

pub struct BaseWalker<'a> {
    pub source: &'a [u8],
}

impl<'a> BaseWalker<'a> {
    pub fn new(source: &'a [u8]) -> Self {
        Self { source }
    }

    pub fn text(&self, node: Node) -> Result<&str> {
        node.utf8_text(self.source).map_err(|e| anyhow::anyhow!(e))
    }

    pub fn child_text(&self, node: Node, field: &str) -> Result<&str> {
        let child = node
            .child_by_field_name(field)
            .context(format!("Missing field: {}", field))?;
        self.text(child)
    }

    pub fn unwrap_parens<'b>(&self, node: Node<'b>) -> Node<'b> {
        let mut current = node;
        while current.kind() == "("
            || (current.child_count() == 3 && current.child(0).unwrap().kind() == "(")
        {
            if current.kind() == "(" {
                if let Some(next) = current.next_sibling() {
                    current = next;
                } else {
                    break;
                }
            } else {
                current = current.child(1).unwrap();
            }
        }
        current
    }

    pub fn extract_string_literal(&self, node: Node) -> Result<String> {
        let text = self.text(node)?;
        if text.len() >= 2 {
            Ok(text[1..text.len() - 1].to_string())
        } else {
            Ok(String::new())
        }
    }

    pub fn extract_int_literal(&self, node: Node) -> Result<i64> {
        let text = self.text(node)?;
        text.parse::<i64>().context("Failed to parse int literal")
    }

    pub fn extract_float_literal(&self, node: Node) -> Result<f64> {
        let text = self.text(node)?;
        text.parse::<f64>().context("Failed to parse float literal")
    }

    pub fn extract_bool_literal(&self, node: Node) -> Result<bool> {
        let text = self.text(node)?;
        Ok(text == "true")
    }

    pub fn create_meta(
        &self,
        node: Node,
        lang: &str,
        file: &str,
    ) -> HashMap<String, serde_json::Value> {
        let mut meta = HashMap::new();
        meta.insert("line".to_string(), json!(node.start_position().row + 1));
        meta.insert(
            "column".to_string(),
            json!(node.start_position().column + 1),
        );
        meta.insert("file".to_string(), json!(file));
        meta.insert("lang".to_string(), json!(lang));
        meta
    }
}

// ── Source position helpers for native-parser frontends ────────────────────

/// Create source position metadata with the same shape as
/// [`BaseWalker::create_meta`], but taking explicit line/column values
/// instead of a tree-sitter [`Node`].
///
/// Use this in [`Frontend`] implementations to attach source locations
/// to CAST nodes during lowering.
///
/// All values are 1-based.
pub fn source_meta(
    file: &str,
    lang: &str,
    line: usize,
    column: usize,
) -> HashMap<String, serde_json::Value> {
    let mut meta = HashMap::new();
    meta.insert("line".to_string(), json!(line));
    meta.insert("column".to_string(), json!(column));
    meta.insert("file".to_string(), json!(file));
    meta.insert("lang".to_string(), json!(lang));
    meta
}

/// Convert a byte offset in a source string to 1-based (line, column).
pub fn byte_offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line: usize = 1;
    let mut col: usize = 1;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Context for lowering source code with position tracking.
///
/// Holds the source text, file name, and language name, and provides
/// [`meta_at()`](Self::meta_at) to build position metadata at any byte offset.
/// Pass this through your lowering functions instead of creating empty
/// meta hash maps.
pub struct LowerCtx<'a> {
    pub source: &'a str,
    pub file: &'a str,
    pub lang: &'a str,
}

impl<'a> LowerCtx<'a> {
    pub fn new(source: &'a str, file: &'a str, lang: &'a str) -> Self {
        Self { source, file, lang }
    }

    /// Create position metadata at the given byte offset into `self.source`.
    pub fn meta_at(&self, offset: usize) -> HashMap<String, serde_json::Value> {
        let (line, col) = byte_offset_to_line_col(self.source, offset);
        source_meta(self.file, self.lang, line, col)
    }

    /// Create position metadata from explicit 1-based line and column numbers.
    ///
    /// Use this when the parser already provides line/column directly
    /// (e.g. brush-parser, tree-sitter) — avoids the byte-offset scan.
    pub fn meta_lc(&self, line: usize, column: usize) -> HashMap<String, serde_json::Value> {
        source_meta(self.file, self.lang, line, column)
    }
}

/// Standard CRUSH capability namespaces
///
/// All language walkers should use these constants to ensure
/// consistent capability names across all runtimes.
pub mod capabilities {
    // I/O
    pub const IO_PRINT: &str = "io.print";
    pub const IO_READ: &str = "io.read";
    pub const IO_READLINE: &str = "io.readline";
    pub const IO_WRITE: &str = "io.write";

    // Filesystem
    pub const FS_READ: &str = "fs.read";
    pub const FS_WRITE: &str = "fs.write";
    pub const FS_EXISTS: &str = "fs.exists";
    pub const FS_MKDIR: &str = "fs.mkdir";
    pub const FS_REMOVE: &str = "fs.remove";
    pub const FS_LIST: &str = "fs.list";

    // Network
    pub const NET_HTTP_GET: &str = "net.http_get";
    pub const NET_HTTP_POST: &str = "net.http_post";
    pub const NET_TCP_CONNECT: &str = "net.tcp_connect";
    pub const NET_DNS_RESOLVE: &str = "net.dns_resolve";

    // Process
    pub const PROC_SPAWN: &str = "proc.spawn";
    pub const PROC_EXEC: &str = "proc.exec";

    // Environment
    pub const ENV_GET: &str = "env.get";
    pub const ENV_SET: &str = "env.set";
}

/// Maps language-specific function names to CRUSH capabilities.
///
/// # Example
/// ```
/// use crush_walker_core::map_to_capability;
/// assert_eq!(map_to_capability("python", "print"), Some("io.print"));
/// assert_eq!(map_to_capability("go", "fmt.Println"), Some("io.print"));
/// ```
pub fn map_to_capability(lang: &str, func_name: &str) -> Option<&'static str> {
    match (lang, func_name) {
        // Python
        ("python", "print") => Some(capabilities::IO_PRINT),
        ("python", "input") => Some(capabilities::IO_READLINE),
        ("python", "open") => Some(capabilities::FS_READ),

        // JavaScript
        ("javascript", "console.log") | ("javascript", "print") => Some(capabilities::IO_PRINT),
        ("javascript", "fetch") => Some(capabilities::NET_HTTP_GET),
        ("javascript", "prompt") => Some(capabilities::IO_READLINE),

        // Rust
        ("rust", "println!") | ("rust", "print!") => Some(capabilities::IO_PRINT),
        ("rust", "eprintln!") | ("rust", "eprint!") => Some(capabilities::IO_PRINT),
        ("rust", "write!") | ("rust", "writeln!") => Some(capabilities::IO_PRINT),
        ("rust", "dbg!") => Some(capabilities::IO_PRINT),
        ("rust", "std::fs::read") | ("rust", "std::fs::read_to_string") => {
            Some(capabilities::FS_READ)
        }
        ("rust", "std::fs::write") => Some(capabilities::FS_WRITE),

        // Go
        ("go", "fmt.Println") | ("go", "fmt.Print") | ("go", "println") => {
            Some(capabilities::IO_PRINT)
        }
        ("go", "os.ReadFile") | ("go", "ioutil.ReadFile") => Some(capabilities::FS_READ),
        ("go", "os.WriteFile") | ("go", "ioutil.WriteFile") => Some(capabilities::FS_WRITE),
        ("go", "http.Get") => Some(capabilities::NET_HTTP_GET),

        // C
        ("c", "printf") | ("c", "puts") | ("c", "fputs") => Some(capabilities::IO_PRINT),
        ("c", "fopen") | ("c", "fread") => Some(capabilities::FS_READ),
        ("c", "fwrite") => Some(capabilities::FS_WRITE),

        // Zig
        ("zig", "std.debug.print") | ("zig", "print") => Some(capabilities::IO_PRINT),

        // Bash
        ("bash", "echo") | ("bash", "printf") => Some(capabilities::IO_PRINT),
        ("bash", "read") => Some(capabilities::IO_READLINE),
        ("bash", "cat") => Some(capabilities::FS_READ),

        // Crush native
        ("crush", _) if func_name.contains('.') => {
            // Crush native capability calls pass through
            None // Let walker handle directly
        }

        _ => None,
    }
}


// ── LanguageAdapter — universal walker dispatch ─────────────────────────────────

use std::sync::Arc;

/// Universal walker adapter — one interface for every language frontend.
///
/// Tree-sitter walkers (`Walker` trait) and native-parser frontends (`Frontend`
/// trait) both get wrapped in this. A single `walk(source, filename)` call
/// produces `(FeatureReport, Program)`, hiding the underlying parse/analyze/lower
/// pipeline behind a uniform API.
///
/// # Why
///
/// Before: each call site (aotc.rs, walk_run.rs, SDKs) manually dispatched on
/// file extension -> called a language-specific function. Adding a language
/// meant touching 5+ files.
///
/// After: `AdapterRegistry::walk(source, filename)` -> done. Adding a language
/// = one macro call + one `registry.register()` call.
pub trait LanguageAdapter: Send + Sync {
    fn language_name(&self) -> &'static str;
    fn file_extensions(&self) -> &[&'static str];

    /// Walk source -> (FeatureReport, Program).
    fn walk(&self, source: &str, filename: &str) -> anyhow::Result<(FeatureReport, Program)>;

    fn can_handle(&self, ext: &str) -> bool {
        self.file_extensions().contains(&ext)
    }
}

/// Registry of all known language adapters.
///
/// ```rust,ignore
/// let registry = AdapterRegistry::new();
/// let (report, program) = registry.walk(source, "hello.py")?;
/// ```
pub struct AdapterRegistry {
    adapters: Vec<Box<dyn LanguageAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self { adapters: Vec::new() }
    }

    pub fn register(&mut self, adapter: Box<dyn LanguageAdapter>) -> &mut Self {
        self.adapters.push(adapter);
        self
    }

    /// Walk source with the first adapter that handles the file extension.
    pub fn walk(&self, source: &str, filename: &str) -> anyhow::Result<(FeatureReport, Program)> {
        let ext = std::path::Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let adapter = self
            .adapters
            .iter()
            .find(|a| a.can_handle(ext))
            .ok_or_else(|| anyhow::anyhow!("no walker registered for .{ext} (available: {})",
                self.adapters.iter()
                    .flat_map(|a| a.file_extensions().iter().copied())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))?;
        adapter.walk(source, filename)
    }

    /// Walk source -> CASM in one call. Convenience for CLI tools.
    pub fn walk_to_casm(
        &self,
        source: &str,
        filename: &str,
    ) -> anyhow::Result<()> {
        // Walk -> CAST only. CASM compilation happens downstream
        // (aotc.exe, walk_run.exe, SDKs call crush_frontend::Compiler separately).
        // Return the program; caller compiles.
        let (_report, program) = self.walk(source, filename)?;
        // Returning program through a Box<dyn Any> to avoid depending on casm/crush_frontend.
        // Downstream: let program = registry.walk(...); compiler.compile(program)?;
        std::mem::drop(program);
        Ok(())
    }

    pub fn walk_for_aotc(
        &self,
        source: &str,
        filename: &str,
    ) -> anyhow::Result<crush_cast::Program> {
        Ok(self.walk(source, filename)?.1)
    }

    /// All language names currently registered.
    pub fn languages(&self) -> Vec<&str> {
        self.adapters.iter().map(|a| a.language_name()).collect()
    }
}

/// Macro: create a `LanguageAdapter` from a `Frontend` implementation.
///
/// ```rust,ignore
/// use crush_walker_core::impl_adapter_from_frontend;
/// impl_adapter_from_frontend!(PythonAdapter, "python", &["py", "pyw"], crush_lang_python::python_to_cast);
/// ```
#[macro_export]
macro_rules! impl_adapter_from_frontend {
    ($adapter_name:ident, $lang:expr, $exts:expr, $to_cast_fn:path) => {
        pub struct $adapter_name;

        impl $crate::LanguageAdapter for $adapter_name {
            fn language_name(&self) -> &'static str { $lang }
            fn file_extensions(&self) -> &[&'static str] { $exts }
            fn walk(&self, source: &str, _filename: &str) -> anyhow::Result<($crate::FeatureReport, crush_cast::Program)> {
                let program = $to_cast_fn(source)
                    .map_err(|e| anyhow::anyhow!("{}$@CAST: {e}", $lang))?;
                let report = $crate::FeatureReport {
                    lang: $lang.to_string(),
                    ..Default::default()
                };
                Ok((report, program))
            }
        }
    };
}

/// Macro: create a `LanguageAdapter` from a tree-sitter `Walker` implementation.
///
/// ```rust,ignore
/// use crush_walker_core::impl_adapter_from_walker;
/// impl_adapter_from_walker!(CAdapter, "c", &["c", "h"], CWalker { file_name: String::new() }, tree_sitter_c::LANGUAGE.into());
/// ```
#[deprecated(note = "use impl_both_for_walker! instead -- it generates BOTH `impl Walker` and `impl LanguageWalker` from a single source-of-truth invocation (the cascade-closure-up-front pattern from Sub-Commit 1 Lesson 4). impl_adapter_from_walker! predates the Sub-Commit 2 unification and is dead code (no callers in the post-Sub-Commit-1 landscape).")]
#[macro_export]
macro_rules! impl_adapter_from_walker {
    ($adapter_name:ident, $lang:expr, $exts:expr, $walker_expr:expr) => {
        pub struct $adapter_name;

        impl $crate::LanguageAdapter for $adapter_name {
            fn language_name(&self) -> &'static str { $lang }
            fn file_extensions(&self) -> &[&'static str] { $exts }
            fn walk(&self, source: &str, filename: &str) -> anyhow::Result<($crate::FeatureReport, crush_cast::Program)> {
                let mut walker: $walker_expr = $walker_expr;  // clone from expr
                // tree-sitter walkers need a filename — set it from the parameter
                let _ = filename;  // walker already has its own file_name

                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&walker.language())
                    .map_err(|e| anyhow::anyhow!("{}$@parser: {e}", $lang))?;
                let tree = parser
                    .parse(source, None)
                    .ok_or_else(|| anyhow::anyhow!("{}$@parse: failed", $lang))?;
                let program = $crate::Walker::walk(&walker, &tree, source.as_bytes())
                    .map_err(|e| anyhow::anyhow!("{}$@walk: {e}", $lang))?;
                let report = $crate::FeatureReport {
                    lang: $lang.to_string(),
                    ..Default::default()
                };
                Ok((report, program))
            }
        }
    };
}

/// Macro: create a Walker + LanguageWalker pair from one source-of-truth
/// invocation. Architecture Option D for the 4th-trait unification work
/// (CRUSH-36 Commit 2 Sub-Commit 2).
///
/// Generates a zero-sized type that implements BOTH [`Walker`] (in
/// walker-core) AND [`LanguageWalker`] (in crush-frontend). The macro is
/// the **cascade-closure-up-front pattern** -- concrete impls on a
/// ZST instead of supertrait tie + blanket impl.
///
/// # Why a macro (vs `pub trait Walker: LanguageWalker` supertie)
///
/// Option A's supertrait tie was rejected because:
/// 1. Cross-crate coupling: [`Walker`] lives in walker-core;
///    [`LanguageWalker`] lives in crush-frontend. Supertrait tie
///    would force cross-crate dep inversion.
/// 2. Method-signature conflict: [`Walker::language`] returns
///    `tree_sitter::Language` (grammar-bound, opaque to polyglot
///    frontend), while [`LanguageWalker::language`] returns
///    `&'static str` (UI-bound, polyglot-frontend-legal). The
///    structural types don't unify cleanly.
///
/// Option D (this macro) sidesteps both by generating concrete impls
/// on a ZST. Cascade closure is **UP FRONT** -- no supertrait, no
/// blanket, no E0119 risk, no 7-fix cascade. This is the
/// Sub-Commit 1 Lesson 4 application:
///
/// > supretrait-tie without immediate blanket impl is incomplete --
/// > write the closure structurally in Fix #1, not as a later
/// > discovered-need.
///
/// # Architecture
///
/// For an invocation
///
/// ```rust,ignore
/// use crush_walker_core::impl_both_for_walker;
/// use crush_frontend::language_walkers::{LanguageWalker, WalkerError};
///
/// impl_both_for_walker!(
///     GoAdapter,
///     "go",                                  // LanguageWalker::language
///     &["go"],                               // LanguageWalker::extensions
///     tree_sitter_go::LANGUAGE.into(),      // Walker::language
///     go_walker::GoWalker,                   // walker type (impl Walker)
///     |fname| go_walker::GoWalker { file_name: fname }  // walker ctor
/// );
/// ```
///
/// the macro expands to a ZST `pub struct GoAdapter;` plus TWO
/// `impl` blocks:
///
/// - `impl $crate::Walker for GoAdapter` -- `language()` returns the
///   `$ts_lang` token (typically a const from a tree-sitter grammar
///   crate); `walk(&Tree, &[u8])` constructs `$wtype` via `$w_init`
///   with an empty filename and delegates to
///   `$crate::Walker::walk(&walker, tree, source)`.
/// - `impl crush_frontend::language_walkers::LanguageWalker for
///   GoAdapter` -- `language()` returns the `$lang` token;
///   `extensions()` returns the `$exts` token; `parse(source,
///   filename)` runs a tree-sitter `Parser` configured with the
///   `$ts_lang` token and stores `Box::new((tree, source, filename))`;
///   `walk(Box<dyn Any>)` downcasts the bundle, reconstructs
///   `$wtype` via `$w_init` with the stored filename, and delegates
///   to `$crate::Walker::walk`.
///   Error mapping: `anyhow::Error` -> `WalkerError::SemanticError`
///   (per the Sub-Commit 1 Lesson 3 cross-scope alias-discipline
///   pattern: macro is fully-qualified, no `use anyhow::Result;`
///   alias required at call site).
///
/// # Send + Sync
///
/// The generated struct is a ZST (zero-sized type). ZSTs are
/// `Send + Sync` by structural inheritance -- no additional bound
/// needed. This is the architectural payoff vs Sub-Commit 1's
/// `Frontend: LanguageAdapter` supertrait-tie: the macro opts OUT
/// of the propagation cascade entirely.
///
/// # Forward flags (per Sub-Commit 1 Lesson cascade closure lens)
///
/// F1: the macro generates concrete impls, NOT a blanket. There is
/// NO E0119 risk because the macro doesn't introduce overlapping
/// impls. Each ZST has exactly ONE `impl Walker` and ONE `impl
/// LanguageWalker` from the macro.
///
/// F2: the macro uses `$crate::Walker` (always resolves to
/// walker-core) and `crush_frontend::language_walkers::LanguageWalker`
/// (resolved at the call-site scope where the macro is invoked).
/// Callers must have `crush-frontend` reachable directly (via
/// crate-local dep) or via re-export. The walker-core fixture test
/// in `src/adapter.rs` exercises this resolution path.
///
/// F3: per-parse expression evaluation of `$ts_lang` is intentional
/// (cheap; `tree_sitter_xxx::LANGUAGE.into()` is a const conversion).
/// If perf later demands caching, the macro can be extended with a
/// 7th `ts_lang_expr_for_parse: |&self| tree_sitter::Language`
/// arg -- but no current caller has this need.
///
/// F4: existing walker crates that hand-rolled `impl Walker for X`
/// continue to compile unchanged. The macro is OPT-IN -- the
/// tree-sitter walkers (Go/C/Zig/Dart) are NOT migrated in this
/// commit. Migrating Go as the canonical exemplar is the
/// Sub-Commit 2 Commit B follow-up.
///
/// F5: no per-FE regression for the Sub-Commit 2 macro. Per
/// Sub-Commit 1's F5 ("per-FE regression — 6 existing Frontend
/// impls auto-derive LanguageAdapter via the blanket"), the
/// parallel concern here is: does the macro introduce a per-FE
/// regression for the 4+ existing tree-sitter walkers (Go/C/Zig/
/// Dart)? Answer: NO. The macro is OPT-IN (F7); existing
/// `impl Walker for X` impls in tree-sitter walkers continue to
/// compile unchanged. The migration is a separate reviewable diff
/// (Sub-Commit 2 Commit B). Migration to the macro is a
/// "registration + dispatch" change, not a "remove existing
/// impl" change; it ADDS a `LanguageWalker` impl alongside the
/// existing `Walker` impl, leaving the prior impl untouched.
///
/// F6: the `Walker::walk(&Tree, &[u8])` path uses empty filename
/// for the inner walker's `file_name` field (the trait signature
/// has no filename parameter). The canonical filename-preserving
/// flow is the `LanguageWalker::parse(source, Some(filename))` +
/// `LanguageWalker::walk(stored_ast)` round-trip, which carries the
/// filename through the (Tree, source, filename) bundle in
/// `Box<dyn Any>`. Production callers should use the
/// `LanguageWalker` round-trip; `Walker::walk` is for test-binary
/// and inference paths where filename is not needed. (Renumbered
/// from inline F* per Sub-Commit 1's F1..F5 consistency.)
///
/// F7: this commit does NOT migrate any existing walker crate
/// (Go/C/Zig/Dart) to use the new macro. Migration of Go as the
/// canonical exemplar is the Sub-Commit 2 Commit B follow-up. The
/// macro + active test establish the architectural pattern; the
/// migration is a separate reviewable diff. (Existing walker
/// crates that hand-rolled `impl Walker for X` continue to compile
/// unchanged; the macro is OPT-IN.)
///
/// F8: a test in `src/adapter.rs` invokes the macro with
/// `unreachable!()` as the `$ts_lang` token. The test exercises
/// ONLY the `language()` + `extensions()` paths on the
/// macro-generated ZST. DO NOT call `.parse()` on the test's
/// `MacroGenAdapter` -- it would attempt to construct a
/// `tree_sitter::Parser` with the unreachable language, which
/// would panic at runtime. The `parse()` path is end-to-end
/// validated when GoWalker is migrated in Sub-Commit 2 Commit B.
/// (Renumbered from inline test-limitation flag C per the
/// F1..F5 consistency.)
#[macro_export]
macro_rules! impl_both_for_walker {
    (
        $adapter_name:ident,
        $lang:expr,
        $exts:expr,
        $ts_lang:expr,
        $wtype:ty,
        $w_init:expr
    ) => {
        // ── Zero-sized adapter struct ──────────────────────────────
        // Auto-implements Send + Sync (ZST property); the const fn
        // below is a compile-time check that this holds for
        // every concrete macro invocation. ZST + macro is
        // Sub-Commit 1 Lesson 4 applied as 'closure UP FRONT'.
        //
        // `Clone` + `Copy` are derived so the same ZST can be
        // coerced into BOTH `Box<dyn Walker>` AND `Box<dyn
        // LanguageWalker>` independently without the test/consumer
        // having to manually clone (Copy is implicit). ZSTs are
        // trivially copy + clone (no field to copy), so the derive
        // is zero-cost + zero-risk.
        #[derive(Clone, Copy)]
        pub struct $adapter_name;

        const _: fn() = || {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<$adapter_name>();
        };

        // ── Walker (walker-core) impl ───────────────────────────────
        // $crate::Walker::language is the tree-sitter-bound grammar
        // accessor; $crate::Walker::walk delegates to the inner walker
        // type's walk method. Both are concrete property accesses
        // -- no supertrait tie, no blanket, no E0119.
        //
        // Note: the `Walker::walk(&Tree, &[u8])` signature has no
        // filename parameter, so the macro substitutes an empty
        // `String::new()` for the inner walker's `file_name` field.
        // This is a silent semantic loss — the inner walker cannot
        // report source-position info keyed to a real filename. The
        // canonical filename-preserving flow is
        // `LanguageWalker::parse(source, Some(filename))` followed by
        // `LanguageWalker::walk(stored_ast)`, which DOES carry the
        // filename through the (Tree, source, filename) bundle in
        // `Box<dyn Any>`. Per Sub-Commit 2 forward-flag F*: the
        // `Walker::walk` direct path is for test-binary / inference
        // paths where filename is not needed; production callers
        // should use the `LanguageWalker` round-trip.
        impl $crate::Walker for $adapter_name {
            fn language(&self) -> tree_sitter::Language {
                $ts_lang
            }
            fn walk(
                &self,
                tree: &tree_sitter::Tree,
                source: &[u8],
            ) -> anyhow::Result<crush_cast::Program> {
                let walker: $wtype = $w_init(String::new());
                $crate::Walker::walk(&walker, tree, source)
            }
        }

        // ── LanguageWalker (crush-frontend) impl ────────────────────
        // Bridge A (parse): tree-sitter parse + bundle (Tree +
        // source + filename) in Box<dyn Any>. WalkerError::ParseError
        // carries grammar/setup failures.
        //
        // Bridge B (walk): downcast the bundle, reconstruct the
        // walker_expr with the stored filename, delegate to
        // $crate::Walker::walk. WalkerError::SemanticError carries
        // AST->CAST transformation failures.
        //
        // Per Sub-Commit 1 Lesson 3 (cross-scope alias discipline):
        // the macro uses fully-qualified `anyhow::Result<...>` and
        // explicit `crush_frontend::language_walkers::WalkerError`
        // -- callers do NOT need `use anyhow::Result;` in scope.
        impl crush_frontend::language_walkers::LanguageWalker for $adapter_name {
            fn language(&self) -> &'static str {
                $lang
            }
            fn extensions(&self) -> &'static [&'static str] {
                $exts
            }
            fn parse(
                &self,
                source: &str,
                filename: Option<&str>,
            ) -> Result<
                Box<dyn std::any::Any>,
                crush_frontend::language_walkers::WalkerError,
            > {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language($ts_lang)
                    .map_err(|e| crush_frontend::language_walkers::WalkerError::ParseError(e.to_string()))?;
                let tree = parser
                    .parse(source, None)
                    .ok_or_else(|| crush_frontend::language_walkers::WalkerError::ParseError("parse failed".into()))?;
                Ok(Box::new((
                    tree,
                    source.to_string(),
                    filename.unwrap_or("").to_string(),
                )))
            }
            fn walk(
                &self,
                ast: Box<dyn std::any::Any>,
            ) -> Result<
                crush_cast::Program,
                crush_frontend::language_walkers::WalkerError,
            > {
                let (tree, source, fname) = *ast
                    .downcast::<(tree_sitter::Tree, String, String)>()
                    .map_err(|_| crush_frontend::language_walkers::WalkerError::ParseError(
                        "impl_both_for_walker: invalid AST bundle (expected (Tree, String, String))".into(),
                    ))?;
                let walker: $wtype = $w_init(fname);
                $crate::Walker::walk(&walker, &tree, source.as_bytes())
                    .map_err(|e| crush_frontend::language_walkers::WalkerError::SemanticError(e.to_string()))
            }
        }
    };
}
