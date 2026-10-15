#[cfg(feature = "swc-backend")]
use anyhow::Result;

#[cfg(feature = "swc-backend")]
pub mod swc;

#[cfg(feature = "js-boa")]
pub mod boa;

/// Parse JavaScript/TypeScript source into a swc Module.
///
/// The swc entry point; the boa one is [`boa::parse`].
#[cfg(feature = "swc-backend")]
pub fn parse(source: &str, ext: &str) -> Result<swc_ecma_ast::Module> {
    swc::parse(source, ext)
}
