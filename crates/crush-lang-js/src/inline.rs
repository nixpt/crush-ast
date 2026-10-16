//! Inline lowering of a `@javascript { ... }` block (CRUSH-272).
//!
//! The JS twin of `crush_lang_python::inline`: crush-lang-sdk's
//! `PolyglotMode::Lower` compiles a block's body into the enclosing Crush
//! program instead of emitting `EXEC_LANG` (a `node` subprocess). The block
//! becomes one function — inputs as parameters, the body lowered through the
//! boa lowering (the one with the scope pass, so a `function` in the block is
//! a closure over the block's bindings), and `return [outputs...]` at the end.
//!
//! The body is wrapped in source, `function NAME(inputs) {` + body + `}`, with
//! the header on the body's first line, so a refusal's line inside the block
//! is the line the walker reports. The block must parse on its own first: a
//! `return` at the top of a script is a syntax error to node, and the wrapper
//! must not make it legal.

use std::collections::HashMap;

use crush_cast::{Function, Program};
use crush_walker_core::Frontend;

use crate::lower::Unsupported;
use crate::{JsFrontend, JsLowering};

/// The wrapper function's name in the generated source.
const WRAPPER: &str = "__crush_inline_block";

/// A lowered `@javascript` block: its function, and the functions it
/// declares that capture nothing, which the lowering lifts to the top level.
pub struct LoweredBlock {
    pub function: Function,
    /// `(name, function)`; each name starts with the `prefix` given to
    /// [`lower_block_function`], and the block's code calls them by it.
    pub helpers: Vec<(String, Function)>,
}

/// Lower a `@javascript` block body into a function taking `params` and
/// returning `[outputs...]`. Lifted helper functions are named
/// `{prefix}{name}`. A refusal is an [`Unsupported`] whose line is relative
/// to `code` (line 1 is the line the block's `{` is on).
pub fn lower_block_function(
    code: &str,
    params: &[String],
    outputs: &[String],
    prefix: &str,
) -> anyhow::Result<LoweredBlock> {
    if code.contains(WRAPPER) {
        anyhow::bail!("the block uses the reserved name `{WRAPPER}`");
    }
    // Parse alone first, as node would see it.
    crate::backend::boa::parse(code).map_err(|e| anyhow::anyhow!("JavaScript syntax error: {e:#}"))?;

    let header = format!("function {WRAPPER}({}) {{", params.join(", "));
    let source = format!("{header}{code}\n;return [{}];\n}}\n", outputs.join(", "));
    let frontend = JsFrontend::with_lowering("js", JsLowering::Boa);
    let program: Program = match frontend.parse(&source).and_then(|ast| frontend.lower(ast)) {
        Ok(program) => program,
        Err(e) => return Err(relative_to_block(e, header.chars().count() as u32)),
    };
    let mut functions = program.functions;
    let Some(function) = functions.remove(WRAPPER) else {
        anyhow::bail!("internal: the JS lowering produced no `{WRAPPER}` function");
    };
    if let Some(main) = functions.remove("main")
        && !main.body.is_empty()
    {
        anyhow::bail!("internal: the JS lowering put statements outside `{WRAPPER}`");
    }
    // The rest are lifted `function`s that capture nothing (`f__1`): give
    // them names no other block or Crush function has.
    let renames: HashMap<String, String> =
        functions.keys().map(|name| (name.clone(), format!("{prefix}{name}"))).collect();
    let rename = |f: Function| -> anyhow::Result<Function> {
        let mut v = serde_json::to_value(&f)?;
        rename_refs(&mut v, &renames);
        Ok(serde_json::from_value(v)?)
    };
    let helpers = functions
        .into_iter()
        .map(|(name, f)| Ok((renames[&name].clone(), rename(f)?)))
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(LoweredBlock { function: rename(function)?, helpers })
}

/// Rename every call of / reference to a lifted function (`Call.function`,
/// `Var.name`, …).
fn rename_refs(v: &mut serde_json::Value, renames: &HashMap<String, String>) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, x) in map.iter_mut() {
                if (k == "function" || k == "name" || k == "target")
                    && let Some(new) = x.as_str().and_then(|s| renames.get(s))
                {
                    *x = serde_json::Value::String(new.clone());
                } else {
                    rename_refs(x, renames);
                }
            }
        }
        serde_json::Value::Array(xs) => xs.iter_mut().for_each(|x| rename_refs(x, renames)),
        _ => {}
    }
}

/// Move a refusal's position from the wrapped source to the block's own.
fn relative_to_block(e: anyhow::Error, header_cols: u32) -> anyhow::Error {
    match e.downcast::<Unsupported>() {
        Ok(mut u) => {
            if let Some((line, col)) = u.at
                && line == 1
            {
                u.at = Some((1, col.saturating_sub(header_cols).max(1)));
            }
            u.into()
        }
        Err(e) => e,
    }
}
