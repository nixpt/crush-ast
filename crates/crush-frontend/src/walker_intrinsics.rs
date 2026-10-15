//! Contract for compiler-owned calls emitted by language walkers.
//! Semantic analysis, optimization, and code generation must agree about
//! argument shapes and which arguments are bindings rather than values.

use anyhow::{Result, bail};
use crush_cast::Expression;

#[derive(Clone, Copy)]
pub(crate) enum ResultType {
    Null,
    Bool,
    First,
    Branches,
    Any,
}

#[derive(Clone, Copy)]
pub(crate) struct Intrinsic {
    argc: usize,
    pub variable_target: bool,
    pub writes_binding: bool,
    pub result: ResultType,
}

impl Intrinsic {
    pub fn validate(self, name: &str, args: &[Expression]) -> Result<()> {
        if args.len() != self.argc {
            bail!("{name} expects exactly {} arguments", self.argc);
        }
        if self.variable_target && !matches!(args.first(), Some(Expression::Var { .. })) {
            bail!("{name} target must be a variable");
        }
        Ok(())
    }
}

pub(crate) fn lookup(name: &str) -> Option<Intrinsic> {
    use ResultType::*;
    let (argc, variable_target, writes_binding, result) = match name {
        "__crush_assign__" => (2, true, true, Null),
        "__crush_rebind__" => (1, true, true, Null),
        "__crush_pre_inc__" | "__crush_post_inc__" | "__crush_pre_dec__" | "__crush_post_dec__" => {
            (1, true, true, First)
        }
        "__crush_not__" => (1, false, false, Bool),
        "__crush_neg__" | "__crush_pos__" | "__crush_bit_not__" => (1, false, false, First),
        "__crush_slice__" => (4, false, false, First),
        "__crush_setindex__" => (3, false, false, Any),
        "__crush_subscript__" => (2, false, false, Any),
        "__crush_ternary__" => (3, false, false, Branches),
        "__crush_js_or__" | "__crush_js_and__" => (2, false, false, Any),
        "__crush_deref__" | "__crush_addr_of__" | "__crush_unary__" => (1, false, false, Any),
        _ => return None,
    };
    Some(Intrinsic {
        argc,
        variable_target,
        writes_binding,
        result,
    })
}
