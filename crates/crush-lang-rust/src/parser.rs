//! Rust source parser — wraps syn.

use syn::parse_file;

/// Parse Rust source code into a syn::File.
pub fn parse_source(source: &str) -> anyhow::Result<syn::File> {
    parse_file(source).map_err(|e| anyhow::anyhow!("Rust parse error: {}", e))
}
