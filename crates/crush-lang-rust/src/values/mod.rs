//! CRUSH-xps-7: prove the small value subset, copy flat sequences, reject
//! transfers/borrows that need ownership analysis. No VM ownership model.
mod types;
pub(crate) use types::{hint, signature};

use crush_cast::{Expression as E, Pattern, Program, Statement as S};
use serde_json::{Value, json};
use std::collections::HashMap;
use types::{Kind, decode};
type Meta = HashMap<String, Value>;

#[derive(Clone, Copy, PartialEq)]
enum Use {
    Borrow,
    Take,
    Discard,
}

#[derive(Clone)]
struct Binding {
    kind: Kind,
    depth: usize,
}

struct Check<'a> {
    functions: &'a HashMap<String, Kind>,
    locals: HashMap<String, Binding>,
    depth: usize,
}

fn error(meta: &Meta, reason: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "unsupported Rust value semantics at {}:{}: {reason} (CRUSH-xps-7)",
        meta.get("line").unwrap_or(&Value::Null),
        meta.get("column").unwrap_or(&Value::Null)
    )
}

pub(crate) fn check(program: &mut Program) -> anyhow::Result<()> {
    let functions: HashMap<_, _> = program
        .functions
        .iter()
        .map(|(name, f)| {
            let params = f
                .meta
                .get("rust_value_params")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            (
                name.clone(),
                Kind::Callable(params, Box::new(decode(f.meta.get("rust_value_return")))),
            )
        })
        .collect();
    for (name, f) in &mut program.functions {
        let Kind::Callable(params, _) = &functions[name] else {
            unreachable!()
        };
        let mut c = Check {
            functions: &functions,
            locals: HashMap::new(),
            depth: 0,
        };
        for ((name, _), kind) in f.params.iter().zip(params) {
            if matches!(kind, Kind::Ref(_)) {
                return Err(error(
                    &f.meta,
                    "reference parameters other than &str need borrow checking",
                ));
            }
            c.bind(name, kind.clone());
        }
        c.statements(&mut f.body)?;
    }
    Ok(())
}

impl Check<'_> {
    fn bind(&mut self, name: &str, kind: Kind) {
        self.locals
            .entry(name.into())
            .and_modify(|binding| binding.kind = kind.clone())
            .or_insert(Binding {
                kind,
                depth: self.depth,
            });
    }

    fn statements(&mut self, body: &mut [S]) -> anyhow::Result<Kind> {
        self.statements_with_tail(body, Use::Discard)
    }

    fn statements_with_tail(&mut self, body: &mut [S], tail: Use) -> anyhow::Result<Kind> {
        let mut result = Kind::Copy;
        let len = body.len();
        for (i, s) in body.iter_mut().enumerate() {
            let use_ = if i + 1 == len { tail } else { Use::Discard };
            match s {
                S::VarDecl {
                    name, value, meta, ..
                } => {
                    let mut kind = self.expression(value, Use::Take)?;
                    if matches!(
                        kind,
                        Kind::View(_) | Kind::Iter(_) | Kind::StrView | Kind::Ref(_)
                    ) {
                        return Err(error(
                            meta,
                            "storing borrowed views/iterators needs borrow checking",
                        ));
                    }
                    let hint = decode(meta.get("rust_value_hint"));
                    if let (Kind::Vec(item) | Kind::Array(item), Kind::Vec(_) | Kind::Array(_)) =
                        (&kind, &hint)
                        && **item == Kind::Unknown
                    {
                        kind = hint;
                    }
                    self.bind(name, kind);
                }
                S::ExprStmt { expr, .. } => {
                    result = self.expression(expr, use_)?;
                }
                S::Return { value, meta } => {
                    result = match value {
                        Some(e) => self.expression(e, Use::Take)?,
                        None => Kind::Copy,
                    };
                    if matches!(
                        result,
                        Kind::View(_) | Kind::Iter(_) | Kind::StrView | Kind::Ref(_)
                    ) {
                        return Err(error(
                            meta,
                            "returning borrowed views needs borrow checking",
                        ));
                    }
                }
                S::If {
                    condition,
                    then_body,
                    else_body,
                    ..
                } => {
                    self.expression(condition, Use::Borrow)?;
                    let saved = self.locals.clone();
                    let yes = self.statements_with_tail(then_body, use_)?;
                    self.locals = saved.clone();
                    let no = match else_body {
                        Some(b) => self.statements_with_tail(b, use_)?,
                        None => Kind::Copy,
                    };
                    self.locals = saved;
                    result = if yes == no { yes } else { Kind::Unknown };
                }
                S::While {
                    condition, body, ..
                } => {
                    self.expression(condition, Use::Borrow)?;
                    let saved = self.locals.clone();
                    self.statements(body)?;
                    self.locals = saved;
                }
                S::For {
                    variable,
                    iterable,
                    body,
                    meta,
                } => {
                    let kind = self.expression(iterable, Use::Take)?;
                    let item = match kind {
                        Kind::Vec(k) | Kind::Array(k) | Kind::View(k) | Kind::Iter(k) => *k,
                        Kind::Range => Kind::Copy,
                        _ => return Err(error(meta, "for loops need a proven sequence or range")),
                    };
                    let saved = self.locals.clone();
                    self.bind(variable, item);
                    self.statements(body)?;
                    self.locals = saved;
                }
                S::Break { .. } | S::Continue { .. } => {}
                other => anyhow::bail!("unsupported Rust value statement: {other:?}"),
            }
        }
        Ok(result)
    }

    fn expression(&mut self, expr: &mut E, use_: Use) -> anyhow::Result<Kind> {
        let method = match expr {
            E::Call { meta, .. } | E::CapabilityCall { meta, .. } => meta
                .get("rust_value_method")
                .and_then(Value::as_str)
                .map(str::to_owned),
            _ => None,
        };
        if let Some(method) = method {
            return self.method(expr, &method, use_);
        }
        match expr {
            E::Var { name, meta } => {
                if let Some(binding) = self.locals.get(name) {
                    if binding.depth < self.depth && binding.kind.transfer_needs_proof() {
                        return Err(error(
                            meta,
                            "capturing owned/unknown values needs closure ownership analysis",
                        ));
                    }
                    if use_ == Use::Take && binding.kind.transfer_needs_proof() {
                        let original = meta
                            .get("rust_binding")
                            .and_then(|v| v.get("source_name"))
                            .and_then(Value::as_str)
                            .unwrap_or(name);
                        return Err(error(
                            meta,
                            &format!(
                                "by-value transfer of `{original}` needs move/copy analysis; use an explicit supported clone"
                            ),
                        ));
                    }
                    Ok(binding.kind.clone())
                } else {
                    Ok(self.functions.get(name).cloned().unwrap_or(Kind::Unknown))
                }
            }
            E::IntLiteral { .. }
            | E::FloatLiteral { .. }
            | E::BoolLiteral { .. }
            | E::NullLiteral { .. } => Ok(Kind::Copy),
            E::StringLiteral { meta, .. } => Ok(if meta.get("rust_debug_type") == Some(&Value::String("char".into())) { Kind::Copy } else { Kind::Str }),
            E::ArrayLiteral { elements, meta } => {
                let mut kind = Kind::Unknown;
                for e in elements {
                    let next = self.expression(e, Use::Take)?;
                    if kind == Kind::Unknown {
                        kind = next;
                    } else if kind != next {
                        kind = Kind::Unknown;
                    }
                }
                if meta.get("rust_vec") == Some(&Value::Bool(true)) {
                    Ok(Kind::Vec(Box::new(kind)))
                } else {
                    Ok(Kind::Array(Box::new(kind)))
                }
            }
            E::BinaryOp {
                operator,
                left,
                right,
                meta,
            } => {
                // Remember the source shape before lowering a clone into its
                // receiver. Re-visiting that rewritten node would turn a fresh
                // String clone into an apparent move of its original binding.
                let consumes_binding = matches!(left.as_ref(), E::Var { .. });
                let left_kind = self.expression(left, Use::Borrow)?;
                let right_kind = self.expression(right, Use::Borrow)?;
                if operator == "+" && matches!(left_kind, Kind::Str | Kind::String) {
                    // Format-string assembly only borrows its converted pieces;
                    // a source String + &str consumes the left binding.
                    if left_kind == Kind::String
                        && consumes_binding
                        && meta.get("rust_format_piece") != Some(&Value::Bool(true))
                    {
                        return Err(error(
                            meta,
                            "String + &str consumes its binding; use an explicit supported clone",
                        ));
                    }
                    Ok(Kind::String)
                } else if left_kind.transfer_needs_proof() && left_kind != Kind::Unknown
                    || right_kind.transfer_needs_proof() && right_kind != Kind::Unknown
                {
                    Err(error(
                        meta,
                        "operators on owned values require explicit value lowering",
                    ))
                } else {
                    Ok(Kind::Copy)
                }
            }
            E::UnaryOp {
                operator,
                operand,
                meta,
            } => {
                if operator == "*" {
                    return Err(error(
                        meta,
                        "dereferencing needs borrow/reference semantics",
                    ));
                }
                self.expression(operand, Use::Borrow)?;
                Ok(Kind::Copy)
            }
            E::Range { start, end, .. } => {
                self.expression(start, Use::Borrow)?;
                self.expression(end, Use::Borrow)?;
                Ok(Kind::Range)
            }
            E::Index {
                target,
                index,
                meta,
            } => {
                let k = self.expression(target, Use::Borrow)?;
                self.expression(index, Use::Borrow)?;
                let item = k.sequence().cloned().unwrap_or(Kind::Unknown);
                if use_ == Use::Take && item.transfer_needs_proof() {
                    return Err(error(
                        meta,
                        "moving an owned/unknown sequence element needs ownership analysis",
                    ));
                }
                Ok(item)
            }
            E::Call { function, args, .. } if function == crate::numeric::CAST => {
                self.expression(&mut args[0], Use::Borrow)?;
                Ok(Kind::Copy)
            }
            E::Call {
                function,
                args,
                meta,
            } if function == "__crush_assign__"
                && meta.get("rust_source_call") != Some(&Value::Bool(true)) =>
            {
                let Some(E::Var { name, .. }) = args.first() else {
                    return Err(error(meta, "indexed/field assignment needs value lowering"));
                };
                let name = name.clone();
                if self
                    .locals
                    .get(&name)
                    .is_some_and(|b| b.depth < self.depth && b.kind.transfer_needs_proof())
                {
                    return Err(error(
                        meta,
                        "assigning to a closure capture needs mutable capture semantics",
                    ));
                }
                let old = self
                    .locals
                    .get(&name)
                    .map(|b| b.kind.clone())
                    .unwrap_or(Kind::Unknown);
                let rhs = &mut args[1];
                let kind = if old == Kind::String
                    && meta.get("rust_compound_assign") == Some(&Value::Bool(true))
                {
                    let E::BinaryOp {
                        operator,
                        left,
                        right,
                        ..
                    } = rhs
                    else {
                        unreachable!()
                    };
                    if operator != "+" {
                        return Err(error(meta, "only String += is supported"));
                    }
                    self.expression(left, Use::Borrow)?;
                    self.expression(right, Use::Borrow)?;
                    Kind::String
                } else {
                    self.expression(rhs, Use::Take)?
                };
                if matches!(
                    kind,
                    Kind::View(_) | Kind::Iter(_) | Kind::StrView | Kind::Ref(_)
                ) {
                    return Err(error(
                        meta,
                        "assigning borrowed views needs borrow checking",
                    ));
                }
                self.bind(&name, kind.clone());
                // The shared assignment intrinsic yields null (Rust unit),
                // regardless of the value stored in the target binding.
                Ok(Kind::Copy)
            }
            E::Call {
                function,
                args,
                meta,
            } if function == "__crush_ternary__"
                && meta.get("rust_source_call") != Some(&Value::Bool(true)) =>
            {
                self.expression(&mut args[0], Use::Borrow)?;
                let saved = self.locals.clone();
                let yes = self.expression(&mut args[1], use_)?;
                self.locals = saved.clone();
                let no = self.expression(&mut args[2], use_)?;
                self.locals = saved;
                Ok(if yes == no { yes } else { Kind::Unknown })
            }
            E::Call {
                function,
                args,
                meta,
            } => {
                let callable = self
                    .locals
                    .get(function)
                    .map(|b| b.kind.clone())
                    .or_else(|| self.functions.get(function).cloned());
                let (params, result) = match callable {
                    Some(Kind::Callable(p, r)) => (p, *r),
                    _ => (vec![], Kind::Unknown),
                };
                for (i, arg) in args.iter_mut().enumerate() {
                    let kind = self.expression(arg, Use::Take)?;
                    if matches!(
                        kind,
                        Kind::View(_) | Kind::Iter(_) | Kind::StrView | Kind::Ref(_)
                    ) {
                        return Err(error(meta, "passing borrowed views needs borrow checking"));
                    }
                    if matches!(kind, Kind::Vec(_) | Kind::Array(_) | Kind::String)
                        && params.get(i) != Some(&kind)
                    {
                        return Err(error(
                            meta,
                            "owned arguments need a known compatible parameter type",
                        ));
                    }
                }
                Ok(result)
            }
            E::CapabilityCall { name, args, .. } => {
                for arg in args {
                    self.expression(arg, Use::Borrow)?;
                }
                Ok(if name == "conv.text" || name == "rust.debug" {
                    Kind::String
                } else {
                    Kind::Copy
                })
            }
            E::Lambda { params, body, meta } => {
                let kinds: Vec<Kind> = meta
                    .get("rust_value_params")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let saved = self.locals.clone();
                self.depth += 1;
                for ((name, _), kind) in params.iter().zip(&kinds) {
                    if matches!(kind, Kind::Ref(_)) {
                        return Err(error(
                            meta,
                            "reference closure parameters need borrow checking",
                        ));
                    }
                    self.bind(name, kind.clone());
                }
                let result = self.statements(body)?;
                self.depth -= 1;
                self.locals = saved;
                Ok(Kind::Callable(kinds, Box::new(result)))
            }
            E::Match {
                expression, arms, ..
            } => {
                let subject = self.expression(expression, Use::Take)?;
                let saved = self.locals.clone();
                let mut result = None;
                for arm in arms {
                    if let Pattern::Identifier { name } = &arm.pattern {
                        self.bind(name, subject.clone());
                    }
                    let next = self.statements_with_tail(&mut arm.body, use_)?;
                    result = Some(match result {
                        Some(prev) if prev != next => Kind::Unknown,
                        _ => next,
                    });
                    self.locals = saved.clone();
                }
                Ok(result.unwrap_or(Kind::Copy))
            }
            E::GetField { meta, .. } => Err(error(
                meta,
                "field value semantics need Rust object lowering",
            )),
            other => anyhow::bail!("unsupported Rust value expression: {other:?}"),
        }
    }

    fn method(&mut self, expr: &mut E, method: &str, use_: Use) -> anyhow::Result<Kind> {
        let (args, meta) = match expr {
            E::Call { args, meta, .. } | E::CapabilityCall { args, meta, .. } => (args, meta),
            _ => unreachable!(),
        };
        let mode = if method == "into_iter" {
            Use::Take
        } else {
            Use::Borrow
        };
        let receiver = self.expression(&mut args[0], mode)?;
        let kind = match method {
            "clone" | "to_owned" | "to_vec" => {
                let output = match &receiver {
                    Kind::Copy | Kind::Str if method == "clone" => receiver.clone(),
                    Kind::Str | Kind::String if method != "to_vec" => Kind::String,
                    Kind::StrView if method == "to_owned" => Kind::String,
                    Kind::Vec(item) | Kind::Array(item) if item.flat() => {
                        if method == "to_vec" {
                            Kind::Vec(item.clone())
                        } else {
                            receiver.clone()
                        }
                    }
                    Kind::View(item) if item.flat() && method != "clone" => Kind::Vec(item.clone()),
                    _ => {
                        return Err(error(
                            meta,
                            "clone/copy needs a proven scalar, string, or flat sequence; nested/unknown/reference values are unsupported",
                        ));
                    }
                };
                let mut receiver_expr = args.remove(0);
                if matches!(output, Kind::Vec(_) | Kind::Array(_)) {
                    let mut metadata = meta.clone();
                    metadata.remove("rust_value_method");
                    metadata.insert("resolved_capability".into(), json!(true));
                    let null = || E::NullLiteral { meta: Meta::new() };
                    receiver_expr = E::CapabilityCall {
                        name: "seq.slice".into(),
                        args: vec![receiver_expr, null(), null(), null()],
                        meta: metadata,
                    };
                }
                *expr = receiver_expr;
                return Ok(output);
            }
            "iter" | "as_slice" | "into_iter" => {
                let Some(item) = receiver.sequence() else {
                    return Err(error(meta, "iterator adapters need a known sequence"));
                };
                let kind = if method == "as_slice" {
                    Kind::View(Box::new(item.clone()))
                } else {
                    Kind::Iter(Box::new(if method == "iter" && *item == Kind::String {
                        Kind::StrView
                    } else {
                        item.clone()
                    }))
                };
                *expr = args.remove(0);
                return Ok(kind);
            }
            "as_str" if receiver == Kind::String => {
                *expr = args.remove(0);
                return Ok(Kind::StrView);
            }
            "reference" => return Err(error(meta, "explicit references need borrow checking")),
            "borrow" | "into" | "copied" | "cloned" | "unwrap" => {
                return Err(error(
                    meta,
                    "this ownership/iterator adapter has no proven value semantics",
                ));
            }
            "to_string" => Kind::String,
            "push" => {
                if !matches!(receiver, Kind::Vec(_)) || use_ != Use::Discard {
                    return Err(error(
                        meta,
                        "Vec.push is supported only as a statement on a known Vec",
                    ));
                }
                let item = self.expression(&mut args[1], Use::Take)?;
                if !item.flat() {
                    return Err(error(
                        meta,
                        "pushing non-flat/unknown values needs element ownership analysis",
                    ));
                }
                Kind::Copy
            }
            "pop" => return Err(error(meta, "Vec.pop needs Option value semantics")),
            "len"
                if matches!(
                    receiver,
                    Kind::Vec(_)
                        | Kind::Array(_)
                        | Kind::View(_)
                        | Kind::Iter(_)
                        | Kind::Str
                        | Kind::String
                        | Kind::StrView
                ) =>
            {
                Kind::Copy
            }
            "contains" | "starts_with" | "ends_with"
                if matches!(receiver, Kind::Str | Kind::String | Kind::StrView) =>
            {
                Kind::Copy
            }
            "to_uppercase" | "to_lowercase" | "replace"
                if matches!(receiver, Kind::Str | Kind::String | Kind::StrView) =>
            {
                Kind::String
            }
            "trim" if receiver == Kind::Str => Kind::Str,
            "trim" if matches!(receiver, Kind::String | Kind::StrView) => Kind::StrView,
            "abs" | "sqrt" | "floor" | "ceil" | "round" | "pow" | "powi" | "powf"
                if matches!(receiver, Kind::Copy | Kind::Unknown) =>
            {
                Kind::Copy
            }
            _ => {
                return Err(error(
                    meta,
                    "method receiver type is not proven for the supported value subset",
                ));
            }
        };
        if method != "push" {
            for arg in args.iter_mut().skip(1) {
                self.expression(arg, Use::Borrow)?;
            }
        }
        Ok(kind)
    }
}
