//! # crush-macros
//!
//! Proc macros for embedding Crush source code at Rust compile time.
//!
//! ## Macros
//!
//! - [`crush!`] — compile inline Crush source to CASM and execute at runtime via FastVM
//! - [`crush_file!`] — compile a `.crush` file from disk
//!
//! ## Usage
//!
//! ```
//! use crush_macros::crush;
//! use crush_vm::CrushResultExt;
//!
//! let x: i64 = crush!("fn main() { return 40 + 2; }").crush_unwrap_int();
//! assert_eq!(x, 42);
//! ```
//!
//! ## How it works
//!
//! At Rust compile time, `crush!`:
//! 1. Calls `crush_frontend::compile_crush_source()` — parses, type-checks, optimizes, compiles to CASM
//! 2. Serializes the CASM to JSON bytes, embedding them as a `const` in your binary
//! 3. At runtime, the embedded JSON is deserialized and executed via `crush_vm::run_casm_json()`

use proc_macro::TokenStream;
use quote::quote;

// ── Shared compilation helper ──────────────────────────────────────────────

fn compile_and_expand(source: &str) -> TokenStream {
    let casm_program = match crush_frontend::compile_crush_source(source) {
        Ok(p) => p,
        Err(e) => {
            let msg = format!("crush! compilation error: {e}");
            return quote! { compile_error!(#msg) }.into();
        }
    };

    let json_str = match serde_json::to_string(&casm_program) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("crush! serialization error: {e}");
            return quote! { compile_error!(#msg) }.into();
        }
    };

    let lit = proc_macro2::Literal::byte_string(json_str.as_bytes());

    let expanded = quote! {
        {
            const CASM_BYTES: &[u8] = #lit;
            ::crush_vm::run_casm_json(CASM_BYTES)
        }
    };

    expanded.into()
}

// ── crush! macro ───────────────────────────────────────────────────────────

/// Compile inline Crush source code and execute it via FastVM.
///
/// Accepts either a string literal or a raw token block:
///
/// ```
/// use crush_macros::crush;
/// use crush_vm::CrushResultExt;
///
/// // String literal form
/// let x = crush!("fn main() { return 42; }").crush_unwrap_int();
/// assert_eq!(x, 42);
///
/// // Raw block form (no quotes)
/// let y = crush!({
///     fn main() {
///         return 40 + 2;
///     }
/// }).crush_unwrap_int();
/// assert_eq!(y, 42);
/// ```
///
/// Returns `Result<FastYield, FastError>` from `crush_vm`.
/// Use `CrushResultExt` (from `crush_vm`) for convenient extraction:
/// `.crush_unwrap_int()`, `.crush_unwrap_float()`, `.crush_unwrap_bool()`, etc.
///
/// ## Compile errors
///
/// If the Crush source fails to compile, the Rust compiler will produce a
/// `compile_error!` with the Crush error message at the macro call site.
#[proc_macro]
pub fn crush(input: TokenStream) -> TokenStream {
    // Try to parse as a string literal first
    if let Ok(lit) = syn::parse::<syn::LitStr>(input.clone()) {
        return compile_and_expand(&lit.value());
    }

    // Try raw block form: `crush!({ ... })` — extract inner source from span
    if let Some(source) = extract_block_source(&input) {
        return compile_and_expand(&source);
    }

    // Fallback: use token stream display (may not preserve exact source)
    let source = input.to_string();
    compile_and_expand(&source)
}

/// Extract the inner source text from a braced block `{ ... }`.
/// Uses span source_text() to get the exact original source, then strips
/// the outer braces so the Crush parser sees valid top-level code.
fn extract_block_source(input: &TokenStream) -> Option<String> {
    let mut iter = input.clone().into_iter();
    if let Some(proc_macro::TokenTree::Group(g)) = iter.next() {
        if iter.next().is_none() {
            // Get source including outer braces via span
            if let Some(full) = g.span().source_text() {
                // Strip outer `{` and `}` (the macro delimiters)
                let inner = full.trim();
                if inner.starts_with('{') && inner.ends_with('}') {
                    return Some(inner[1..inner.len()-1].to_string());
                }
                return Some(inner.to_string());
            }
            // Span source not available �� use the group's inner token stream
            let inner = g.stream().to_string();
            return Some(inner);
        }
    }
    None
}

// ── crush_file! macro ──────────────────────────────────────────────────────

/// Compile a `.crush` file from disk and execute it via FastVM.
///
/// The path is relative to the crate root (the directory containing `Cargo.toml`),
/// matching `include_str!` semantics.
///
/// ```ignore
/// use crush_macros::crush_file;
/// use crush_vm::CrushResultExt;
///
/// let result = crush_file!("src/programs/hello.crush").crush_unwrap_string();
/// ```
///
/// Returns `Result<FastYield, FastError>` from `crush_vm`.
#[proc_macro]
pub fn crush_file(input: TokenStream) -> TokenStream {
    let lit = syn::parse_macro_input!(input as syn::LitStr);
    let path = lit.value();

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let full_path = std::path::Path::new(&manifest_dir).join(&path);

    let source = match std::fs::read_to_string(&full_path) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!(
                "crush_file! could not read '{}': {e}",
                full_path.display()
            );
            return quote! { compile_error!(#msg) }.into();
        }
    };

    compile_and_expand(&source)
}
