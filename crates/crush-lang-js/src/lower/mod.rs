//! The boa lowering: `boa_ast` 0.22 → CAST (CRUSH-250).
//!
//! One JS frontend, shared by the generic walker, crush-aotc and the browser
//! (bozo). It differs from the name-based swc lowering in two ways:
//!
//! - **Scope pass** ([`scope`]): every binding is resolved to the crush local
//!   that holds it. Crush locals are name-keyed per function, so a block-scoped
//!   binding (`let`/`const`/`class`/a block `function`, a `for`/`catch` head)
//!   gets a fresh name that collides with nothing, and the code that sees it
//!   uses that name. Shadowing (`let x = 1; { let x = 2; }`) is therefore sound
//!   instead of refused. `var` and function-level bindings keep their names.
//! - **Typed refusals** ([`Unsupported`]): every construct the lowering can't
//!   express fails at walk time with its name and source position. No arm
//!   lowers to a placeholder `null` or drops what it can't handle.
//!
//! Everything else follows `lower_swc.rs` (statement shapes, the `js.*`
//! capabilities, CRUSH-251 block bindings) so the parity harness can compare
//! the two. Classes, `new` and `this` stay loud until COSM's JS slice.

mod builtins;
mod expr;
mod scope;
mod stmt;

use std::collections::HashMap;
use std::fmt;

use boa_ast::Span;
use boa_interner::{Interner, Sym};
use crush_cast::{Function, Program, Statement as CastStmt};
use crush_walker_core::LowerCtx;

use crate::backend::boa::BoaAst;
use scope::Scopes;

/// A JS construct the lowering can't express (yet), and where it is.
#[derive(Debug, Clone)]
pub struct Unsupported {
    pub construct: String,
    /// 1-based line and column, when known.
    pub at: Option<(u32, u32)>,
}

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} is not supported by the JS walker yet",
            self.construct
        )?;
        if let Some((line, col)) = self.at {
            write!(f, " (line {line}, column {col})")?;
        }
        Ok(())
    }
}

impl std::error::Error for Unsupported {}

type Meta = HashMap<String, serde_json::Value>;

pub(crate) struct Lower<'a> {
    interner: &'a Interner,
    ctx: &'a LowerCtx<'a>,
    scopes: Scopes,
    /// Top-level `function` declarations: their own crush functions.
    functions: HashMap<String, Function>,
    /// Source of unique ids for hidden locals (`__sw_val_N`, ...).
    next_id: u32,
    /// The script's top-level `function` names: globals, not captures.
    toplevel_fns: std::collections::HashSet<Sym>,
    /// Every name the script's top level binds (`main`'s locals).
    script_names: std::collections::HashSet<Sym>,
}

impl<'a> Lower<'a> {
    fn new(interner: &'a Interner, ctx: &'a LowerCtx<'a>) -> Self {
        Lower {
            interner,
            ctx,
            scopes: Scopes::default(),
            functions: HashMap::new(),
            toplevel_fns: Default::default(),
            script_names: Default::default(),
            next_id: 0,
        }
    }

    fn name(&self, sym: Sym) -> String {
        self.interner.resolve_expect(sym).to_string()
    }

    /// Is `s` spelled anywhere in the source (as an identifier or string)?
    fn in_source(&self, s: &str) -> bool {
        self.interner.get(s).is_some()
    }

    /// A fresh id for hidden locals.
    fn id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// A hidden local name `{prefix}{id}` that the source doesn't use.
    fn hidden(&mut self, prefix: &str) -> String {
        loop {
            let n = format!("{prefix}{}", self.id());
            if !self.in_source(&n) {
                return n;
            }
        }
    }

    fn meta(&self, span: Option<Span>) -> Meta {
        match span {
            Some(s) => self.ctx.meta_lc(
                s.start().line_number() as usize,
                s.start().column_number() as usize,
            ),
            None => self.ctx.meta_at(0),
        }
    }

    fn unsupported(&self, construct: impl Into<String>, span: Option<Span>) -> anyhow::Error {
        let at = span.map(|s| (s.start().line_number(), s.start().column_number()));
        Unsupported {
            construct: construct.into(),
            at,
        }
        .into()
    }
}

/// Lower a parsed script or module to a CAST program.
pub fn lower_boa(ast: BoaAst, ctx: &LowerCtx) -> anyhow::Result<Program> {
    let (interner, items) = match &ast {
        BoaAst::Script(script, interner) => (
            interner,
            script
                .statements()
                .iter()
                .map(stmt::TopItem::Stmt)
                .collect::<Vec<_>>(),
        ),
        BoaAst::Module(module, interner) => (
            interner,
            module
                .items()
                .items()
                .iter()
                .map(stmt::TopItem::Module)
                .collect(),
        ),
    };
    let mut lower = Lower::new(interner, ctx);
    let main_body = lower.top_level(&items)?;
    let mut functions = std::mem::take(&mut lower.functions);
    if !main_body.is_empty() {
        functions.insert(
            "main".to_string(),
            Function {
                params: vec![],
                body: main_body,
                ..Default::default()
            },
        );
    }
    Ok(Program {
        cast_version: "0.2".to_string(),
        entry: "main".to_string(),
        lang: Some("javascript".to_string()),
        functions,
        ai_meta: None,
        ..Default::default()
    })
}

/// `if (true) { stmts }`: several statements where one is expected. The
/// compiler doesn't scope an `if` body, so this is only grouping.
fn group(stmts: Vec<CastStmt>, meta: Meta) -> CastStmt {
    CastStmt::If {
        condition: crush_cast::Expression::BoolLiteral {
            value: true,
            meta: meta.clone(),
        },
        then_body: stmts,
        else_body: None,
        meta,
    }
}
