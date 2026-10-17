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
    Int,
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
        if name == "__crush_rust_int_cast__" {
            let width = match &args[1] {
                Expression::IntLiteral {
                    value: width @ (8 | 16 | 32 | 64),
                    ..
                } => *width,
                _ => bail!("{name} width must be a literal 8, 16, 32 or 64"),
            };
            let signed = match &args[2] {
                Expression::BoolLiteral { value, .. } => *value,
                _ => bail!("{name} signedness must be a literal bool"),
            };
            if width == 64 && !signed {
                bail!("{name} cannot represent u64");
            }
            if matches!(
                args[0],
                Expression::FloatLiteral { .. }
                    | Expression::StringLiteral { .. }
                    | Expression::BoolLiteral { .. }
                    | Expression::NullLiteral { .. }
                    | Expression::ArrayLiteral { .. }
            ) {
                bail!("{name} requires an integer operand");
            }
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
        "__crush_rust_int_cast__" => (3, false, false, Int),
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

/// Result type of a compiler-owned builtin call. `semantics` maps these to
/// its own `Type`; the table stays free of checker types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuiltinRet {
    Null,
    Bool,
    Int,
    Str,
    Any,
    ArrayAny,
    ArrayInt,
    ArrayStr,
}

/// A function name `Compiler::compile_call` lowers inline instead of emitting
/// a `call`. The type checker consults this table so it accepts exactly the
/// names the compiler owns (CRUSH-111): `compile_call` and the checker used to
/// keep separate lists, so a name the compiler handled (`make_range`, which
/// the Python `range()` walker emits) was refused by `compile_cast`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Builtin {
    pub name: &'static str,
    pub min_args: usize,
    pub max_args: usize,
    pub ret: BuiltinRet,
}

impl Builtin {
    pub fn check_arity(&self, got: usize) -> Result<()> {
        if got < self.min_args || got > self.max_args {
            let want = if self.min_args == self.max_args {
                format!("{}", self.min_args)
            } else {
                format!("{} to {}", self.min_args, self.max_args)
            };
            bail!("Function '{}' expects {} arguments, found {}", self.name, want, got);
        }
        Ok(())
    }
}

use BuiltinRet as R;

/// Every name `compile_call` special-cases inline, and nothing else. The
/// `compiler_and_table_agree` test in `tests/walker_intrinsics.rs` fails when
/// `compile_call` gains an arm that is not listed here (or the reverse).
///
/// Deliberately absent: the `__crush_*__` family, which is the intrinsic
/// registry above, and method-call / constructor / user-function fallthroughs,
/// which are not names.
pub(crate) const BUILTINS: &[Builtin] = &[
    Builtin { name: "len", min_args: 1, max_args: 1, ret: R::Int },
    Builtin { name: "print", min_args: 1, max_args: 1, ret: R::Null },
    Builtin { name: "parallel_map", min_args: 2, max_args: 2, ret: R::ArrayAny },
    Builtin { name: "make_range", min_args: 1, max_args: 3, ret: R::ArrayInt },
    Builtin { name: "range", min_args: 1, max_args: 3, ret: R::ArrayInt },
    Builtin { name: "arr_get", min_args: 2, max_args: 2, ret: R::Any },
    Builtin { name: "arr_set", min_args: 3, max_args: 3, ret: R::Any },
    Builtin { name: "array.push", min_args: 2, max_args: 2, ret: R::Null },
    Builtin { name: "array.pop", min_args: 1, max_args: 1, ret: R::Any },
    Builtin { name: "str.contains", min_args: 2, max_args: 2, ret: R::Bool },
    Builtin { name: "str.starts_with", min_args: 2, max_args: 2, ret: R::Bool },
    Builtin { name: "str.ends_with", min_args: 2, max_args: 2, ret: R::Bool },
    Builtin { name: "str.to_upper", min_args: 1, max_args: 1, ret: R::Str },
    Builtin { name: "str.to_lower", min_args: 1, max_args: 1, ret: R::Str },
    Builtin { name: "str.trim", min_args: 1, max_args: 1, ret: R::Str },
    Builtin { name: "str.split", min_args: 2, max_args: 2, ret: R::ArrayStr },
    Builtin { name: "str.replace", min_args: 3, max_args: 3, ret: R::Str },
    Builtin { name: "str.join", min_args: 2, max_args: 2, ret: R::Str },
    Builtin { name: "math.pow", min_args: 2, max_args: 2, ret: R::Any },
    Builtin { name: "math.sqrt", min_args: 1, max_args: 1, ret: R::Any },
    Builtin { name: "math.abs", min_args: 1, max_args: 1, ret: R::Any },
    Builtin { name: "math.round", min_args: 1, max_args: 1, ret: R::Any },
    Builtin { name: "math.floor", min_args: 1, max_args: 1, ret: R::Any },
    Builtin { name: "math.ceil", min_args: 1, max_args: 1, ret: R::Any },
    Builtin { name: "ai_synthesize", min_args: 1, max_args: 2, ret: R::Any },
    Builtin { name: "ai_semantic_match", min_args: 2, max_args: 3, ret: R::Any },
];

pub(crate) fn builtin(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `function == "<name>"` arm in `compile_call` must be a table row
    /// (or an intrinsic), and every row must have an arm: the two lists cannot
    /// drift apart again (CRUSH-111).
    #[test]
    fn compiler_and_table_agree() {
        let src = include_str!("compiler.rs");
        let mut arms = std::collections::BTreeSet::new();
        for part in src.split("function == \"").skip(1) {
            let name = part.split('"').next().unwrap();
            if !name.starts_with("__crush_") {
                arms.insert(name.to_string());
            }
        }
        let table: std::collections::BTreeSet<String> =
            BUILTINS.iter().map(|b| b.name.to_string()).collect();
        assert_eq!(arms, table, "compile_call arms vs walker_intrinsics::BUILTINS");
    }
}
