pub mod analyzer;
pub mod backend;
pub mod lower_swc;

#[cfg(feature = "boa-backend")]
pub mod analyzer_boa;
#[cfg(feature = "boa-backend")]
pub mod lower_boa;

use std::any::Any;

use crush_cast::Program;
use walker_core::{FeatureReport, Frontend};

/// Wrapper for Boa-parsed AST + interner, transported via `Box<dyn Any>`.
#[cfg(feature = "boa-backend")]
pub struct BoaParsed {
    pub ast: crate::backend::boa::BoaAst,
}

pub struct JsFrontend {
    pub ext: String,
}

impl JsFrontend {
    pub fn new(ext: impl Into<String>) -> Self {
        JsFrontend { ext: ext.into() }
    }
}

impl Default for JsFrontend {
    fn default() -> Self {
        JsFrontend { ext: "js".to_string() }
    }
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

        #[cfg(feature = "boa-backend")]
        if ext == "js" || ext == "mjs" || ext == "cjs" {
            let ast = crate::backend::boa::parse(source)?;
            return Ok(Box::new(BoaParsed { ast }));
        }

        let parsed = backend::parse(source, ext)?;
        Ok(Box::new(parsed))
    }

    fn analyze(&self, ast: &Box<dyn Any>) -> anyhow::Result<FeatureReport> {
        let mut r = FeatureReport::default();
        r.lang = "javascript".to_string();

        #[cfg(feature = "boa-backend")]
        if let Some(parsed) = ast.downcast_ref::<BoaParsed>() {
            crate::analyzer_boa::analyze(&parsed.ast, &mut r)?;
            return Ok(r);
        }

        let module = ast
            .downcast_ref::<swc_ecma_ast::Module>()
            .ok_or_else(|| anyhow::anyhow!("expected swc Module"))?;
        for item in &module.body {
            analyzer::analyze_item(item, &mut r);
        }
        Ok(r)
    }

    fn lower(&self, ast: Box<dyn Any>) -> anyhow::Result<Program> {
        #[cfg(feature = "boa-backend")]
        match ast.downcast::<BoaParsed>() {
            Ok(parsed) => {
                return crate::lower_boa::lower_boa(parsed.ast);
            }
            Err(a) => {
                let module = a
                    .downcast::<swc_ecma_ast::Module>()
                    .map_err(|_| anyhow::anyhow!("expected swc Module"))?;
                return lower_swc::lower_module(&module);
            }
        }

        #[cfg(not(feature = "boa-backend"))]
        {
            let module = ast
                .downcast::<swc_ecma_ast::Module>()
                .map_err(|_| anyhow::anyhow!("expected swc Module"))?;
            lower_swc::lower_module(&module)
        }
    }
}

/// Parse JS/TS source and lower to CAST.
///
/// `ext` is the file extension (e.g. "js", "ts", "tsx") used to select
/// the parser backend and syntax mode.
pub fn js_to_cast(source: &str, ext: &str) -> anyhow::Result<Program> {
    let frontend = JsFrontend::new(ext);
    let (_, program) = walker_core::frontend_pipeline(&frontend, source)?;
    Ok(program)
}
