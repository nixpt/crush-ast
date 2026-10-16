#[cfg(feature = "swc-backend")]
pub mod analyzer;
pub mod backend;
pub mod freevars;
mod lower_common;
#[cfg(feature = "swc-backend")]
pub mod lower_swc;
pub mod sdk;

#[cfg(feature = "js-boa")]
pub mod analyzer_boa;
#[cfg(feature = "js-boa")]
pub mod lower;
#[cfg(feature = "js-boa")]
pub mod inline;

use std::any::Any;

use crush_cast::Program;
use crush_walker_core::{FeatureReport, Frontend, LowerCtx};

/// Wrapper for Boa-parsed AST + interner, transported via `Box<dyn Any>`.
#[cfg(feature = "js-boa")]
pub struct BoaParsed {
    pub ast: crate::backend::boa::BoaAst,
}

/// Which parser + lowering turns a source file into CAST.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsLowering {
    /// swc parser + `lower_swc.rs`: JS (incl. JSX) and, with `ts`, TypeScript.
    Swc,
    /// `boa_parser` 0.22 + `lower_boa.rs`: plain JS only (CRUSH-250).
    Boa,
}

impl JsLowering {
    /// The lowering a file with this extension gets by default, or an error
    /// naming the cargo feature that is missing.
    ///
    /// `.js` stays on swc while `swc-backend` is compiled in, so turning
    /// `js-boa` on somewhere in a build (crush-lang-sdk's `polyglot-javascript`
    /// does) changes nothing until CRUSH-250 flips the default.
    pub fn for_ext(ext: &str) -> anyhow::Result<Self> {
        match ext {
            "ts" | "tsx" | "mts" => {
                if cfg!(feature = "ts") {
                    Ok(JsLowering::Swc)
                } else {
                    anyhow::bail!("`.{ext}` needs crush-lang-js's `ts` feature (TypeScript)")
                }
            }
            "jsx" => {
                if cfg!(feature = "swc-backend") {
                    Ok(JsLowering::Swc)
                } else {
                    anyhow::bail!("`.jsx` needs crush-lang-js's `swc-backend` feature (boa has no JSX)")
                }
            }
            _ => {
                if cfg!(feature = "swc-backend") {
                    Ok(JsLowering::Swc)
                } else if cfg!(feature = "js-boa") {
                    Ok(JsLowering::Boa)
                } else {
                    anyhow::bail!("crush-lang-js was built without a JS parser: enable `js-boa` or `swc-backend`")
                }
            }
        }
    }
}

pub struct JsFrontend {
    pub ext: String,
    /// `None`: [`JsLowering::for_ext`].
    pub lowering: Option<JsLowering>,
}

impl JsFrontend {
    pub fn new(ext: impl Into<String>) -> Self {
        JsFrontend { ext: ext.into(), lowering: None }
    }

    /// Force a lowering, e.g. to run the boa frontend while swc is the default.
    pub fn with_lowering(ext: impl Into<String>, lowering: JsLowering) -> Self {
        JsFrontend { ext: ext.into(), lowering: Some(lowering) }
    }
}

impl Default for JsFrontend {
    fn default() -> Self {
        JsFrontend::new("js")
    }
}

/// The error for a lowering this build doesn't carry.
#[allow(dead_code)]
fn not_built(lowering: JsLowering) -> anyhow::Error {
    let feature = match lowering {
        JsLowering::Swc => "swc-backend",
        JsLowering::Boa => "js-boa",
    };
    anyhow::anyhow!("crush-lang-js was built without the `{feature}` feature")
}

impl Frontend for JsFrontend {
    fn language_name(&self) -> &'static str {
        "javascript"
    }

    fn file_extensions(&self) -> &[&'static str] {
        &[".js", ".mjs", ".cjs", ".jsx", ".ts", ".tsx", ".mts"]
    }

    fn parse(&self, source: &str) -> anyhow::Result<Box<dyn Any>> {
        let ext = self.ext.as_str();
        let lowering = match self.lowering {
            Some(l) => l,
            None => JsLowering::for_ext(ext)?,
        };
        match lowering {
            #[cfg(feature = "js-boa")]
            JsLowering::Boa => {
                let ast = crate::backend::boa::parse(source)?;
                Ok(Box::new((source.to_string(), BoaParsed { ast })))
            }
            #[cfg(feature = "swc-backend")]
            JsLowering::Swc => {
                let parsed = backend::swc::parse(source, ext)?;
                Ok(Box::new((source.to_string(), parsed)))
            }
            #[allow(unreachable_patterns)]
            other => {
                let _ = source;
                Err(not_built(other))
            }
        }
    }

    fn analyze(&self, ast: &Box<dyn Any>) -> anyhow::Result<FeatureReport> {
        let mut r = FeatureReport::default();
        r.lang = "javascript".to_string();

        #[cfg(feature = "js-boa")]
        if let Some((_, parsed)) = ast.downcast_ref::<(String, BoaParsed)>() {
            crate::analyzer_boa::analyze(&parsed.ast, &mut r)?;
            return Ok(r);
        }

        #[cfg(feature = "swc-backend")]
        if let Some((_, module)) = ast.downcast_ref::<(String, swc_ecma_ast::Module)>() {
            for item in &module.body {
                analyzer::analyze_item(item, &mut r);
            }
            return Ok(r);
        }

        let _ = ast;
        anyhow::bail!("expected a parsed JS module")
    }

    fn lower(&self, ast: Box<dyn Any>) -> anyhow::Result<Program> {
        #[cfg(feature = "js-boa")]
        let ast = match ast.downcast::<(String, BoaParsed)>() {
            Ok(tuple_box) => {
                let (source, parsed) = *tuple_box;
                let ctx = LowerCtx::new(&source, "input.js", "javascript");
                return crate::lower::lower_boa(parsed.ast, &ctx);
            }
            Err(a) => a,
        };

        #[cfg(feature = "swc-backend")]
        let ast = match ast.downcast::<(String, swc_ecma_ast::Module)>() {
            Ok(tuple_box) => {
                let (source, module) = *tuple_box;
                let ctx = LowerCtx::new(&source, "input.js", "javascript");
                return lower_swc::lower_module(&module, &ctx);
            }
            Err(a) => a,
        };

        let _ = (ast, LowerCtx::new);
        anyhow::bail!("expected a parsed JS module")
    }
}

/// Parse JS/TS source and lower to CAST.
///
/// `ext` is the file extension (e.g. "js", "ts", "tsx") used to select
/// the parser backend and syntax mode.
pub fn js_to_cast(source: &str, ext: &str) -> anyhow::Result<Program> {
    let frontend = JsFrontend::new(ext);
    let (_, program) = crush_walker_core::frontend_pipeline(&frontend, source)?;
    Ok(program)
}

// ── Adapter ──────────────────────────────────────────────────────────────────

use crush_walker_core::LanguageAdapter;


/// JS/TS adapter — handles .js/.mjs/.cjs/.ts/.tsx/.mts
pub struct JsAdapter;

impl LanguageAdapter for JsAdapter {
    fn language_name(&self) -> &'static str { "javascript" }
    fn file_extensions(&self) -> &[&'static str] { &["js", "mjs", "cjs", "ts", "tsx", "mts"] }
    fn walk(&self, source: &str, filename: &str) -> anyhow::Result<(FeatureReport, Program)> {
        let ext = std::path::Path::new(filename).extension()
            .and_then(|e| e.to_str()).unwrap_or("js");
        let program = crate::js_to_cast(source, ext)
            .map_err(|e| anyhow::anyhow!("javascript@CAST: {e}"))?;
        Ok((FeatureReport { lang: "javascript".to_string(), ..Default::default() }, program))
    }
}

#[cfg(test)]
mod tests {
    use super::JsLowering;

    #[test]
    fn js_lowering_per_extension() {
        let js = JsLowering::for_ext("js");
        if cfg!(feature = "swc-backend") {
            assert_eq!(js.unwrap(), JsLowering::Swc, "swc keeps `.js` until CRUSH-250 flips it");
        } else if cfg!(feature = "js-boa") {
            assert_eq!(js.unwrap(), JsLowering::Boa);
        } else {
            assert!(js.is_err());
        }
        let ts = JsLowering::for_ext("ts");
        if cfg!(feature = "ts") {
            assert_eq!(ts.unwrap(), JsLowering::Swc);
        } else {
            assert!(format!("{:#}", ts.unwrap_err()).contains("`ts` feature"));
        }
    }
}
