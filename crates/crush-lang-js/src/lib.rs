pub mod analyzer;
pub mod backend;
pub mod lower_swc;
pub mod sdk;

#[cfg(feature = "boa-backend")]
pub mod analyzer_boa;
#[cfg(feature = "boa-backend")]
pub mod lower_boa;

use std::any::Any;

use crush_cast::Program;
use crush_walker_core::{FeatureReport, Frontend, LowerCtx};

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
        JsFrontend {
            ext: "js".to_string(),
        }
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
            return Ok(Box::new((source.to_string(), BoaParsed { ast })));
        }

        let parsed = backend::parse(source, ext)?;
        Ok(Box::new((source.to_string(), parsed)))
    }

    fn analyze(&self, ast: &Box<dyn Any>) -> anyhow::Result<FeatureReport> {
        let mut r = FeatureReport::default();
        r.lang = "javascript".to_string();

        #[cfg(feature = "boa-backend")]
        if let Some((_, parsed)) = ast.downcast_ref::<(String, BoaParsed)>() {
            crate::analyzer_boa::analyze(&parsed.ast, &mut r)?;
            return Ok(r);
        }

        let module = ast
            .downcast_ref::<(String, swc_ecma_ast::Module)>()
            .map(|(_, m)| m)
            .ok_or_else(|| anyhow::anyhow!("expected (String, swc Module)"))?;
        for item in &module.body {
            analyzer::analyze_item(item, &mut r);
        }
        Ok(r)
    }

    fn lower(&self, ast: Box<dyn Any>) -> anyhow::Result<Program> {
        #[cfg(feature = "boa-backend")]
        match ast.downcast::<(String, BoaParsed)>() {
            Ok(tuple_box) => {
                let (source, parsed) = *tuple_box;
                let ctx = LowerCtx::new(&source, "input.js", "javascript");
                return crate::lower_boa::lower_boa(parsed.ast, &ctx);
            }
            Err(a) => {
                let (source, module) = *a
                    .downcast::<(String, swc_ecma_ast::Module)>()
                    .map_err(|_| anyhow::anyhow!("expected (String, swc Module)"))?;
                let ctx = LowerCtx::new(&source, "input.js", "javascript");
                return lower_swc::lower_module(&module, &ctx);
            }
        }

        #[cfg(not(feature = "boa-backend"))]
        {
            let (source, module) = *ast
                .downcast::<(String, swc_ecma_ast::Module)>()
                .map_err(|_| anyhow::anyhow!("expected (String, swc Module)"))?;
            let ctx = LowerCtx::new(&source, "input.js", "javascript");
            lower_swc::lower_module(&module, &ctx)
        }
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
