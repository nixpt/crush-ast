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
//! use walker_core::{Walker, BaseWalker};
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
/// use walker_core::LowerCtx;
///
/// fn lower(&self, ast: Box<dyn Any>) -> Result<Program> {
///     let (source, stmts) = *ast.downcast::<(String, MyAst)>()?;
///     let ctx = LowerCtx::new(&source, "input.py", "python");
///     // ... lower with ctx, using ctx.meta_at(offset) for position metadata
/// }
/// ```
pub trait Frontend {
    fn language_name(&self) -> &'static str;
    fn file_extensions(&self) -> &[&'static str];

    /// Parse source text into a language-specific AST (opaque).
    fn parse(&self, source: &str) -> Result<Box<dyn std::any::Any>>;

    /// Analyze the parsed AST for features and capability requirements.
    fn analyze(&self, ast: &Box<dyn std::any::Any>) -> Result<FeatureReport>;

    /// Lower the parsed AST to a CAST Program.
    fn lower(&self, ast: Box<dyn std::any::Any>) -> Result<Program>;
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
/// use walker_core::{TreeSitterFrontend, Walker, frontend_pipeline};
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

impl<W: Walker> Frontend for TreeSitterFrontend<W> {
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
/// use walker_core::run_walker_binary;
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
pub fn run_walker_binary<W: Walker>(
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
/// use walker_core::map_to_capability;
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
