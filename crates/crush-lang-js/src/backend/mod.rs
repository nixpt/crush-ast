use anyhow::Result;

pub mod swc;

#[cfg(feature = "boa-backend")]
pub mod boa;

/// Parse JavaScript/TypeScript source into a swc Module.
///
/// Always uses swc (the default/primary backend). The boa backend is
/// a separate entry point (see `boa::parse`).
pub fn parse(source: &str, ext: &str) -> Result<swc_ecma_ast::Module> {
    swc::parse(source, ext)
}
