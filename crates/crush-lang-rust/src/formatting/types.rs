//! Conservative typed Debug proofs over resolved lexical storage. This is not
//! a Rust type checker: writes, indirect calls and unproven producers refuse.
use crush_cast::{Expression as E, Function, Program, Statement as S};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
type Meta = HashMap<String, Value>;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Ty {
    Unknown,
    Unit,
    Bool,
    Int(String),
    F64,
    Str,
    Char,
    Seq(Box<Ty>),
}
impl Ty {
    fn descriptor(&self) -> Option<Value> {
        fn tree(ty: &Ty, depth: usize) -> Option<Value> {
            if depth > 16 {
                return None;
            }
            let kind = match ty {
                Ty::Unknown => return None,
                Ty::Unit => "unit",
                Ty::Bool => "bool",
                Ty::Int(n) => n,
                Ty::F64 => "f64",
                Ty::Str => "str",
                Ty::Char => "char",
                Ty::Seq(item) => return Some(json!({"kind":"seq", "item":tree(item, depth + 1)?})),
            };
            Some(json!({"kind":kind}))
        }
        tree(self, 0)
    }
    fn known(&self) -> bool {
        self.descriptor().is_some()
    }
}
fn hint(value: Option<&Value>) -> Ty {
    fn of_type(ty: &syn::Type) -> Ty {
        match ty {
            syn::Type::Path(p) if p.path.segments.len() == 1 => {
                let seg = &p.path.segments[0];
                let n = seg.ident.to_string();
                if n == "Vec"
                    && let syn::PathArguments::AngleBracketed(a) = &seg.arguments
                    && let Some(syn::GenericArgument::Type(t)) = a.args.first()
                {
                    return Ty::Seq(Box::new(of_type(t)));
                }
                primitive(&n)
            }
            syn::Type::Reference(r) if matches!(r.elem.as_ref(), syn::Type::Path(p) if p.path.is_ident("str")) => {
                Ty::Str
            }
            syn::Type::Array(a) => Ty::Seq(Box::new(of_type(&a.elem))),
            syn::Type::Tuple(t) if t.elems.is_empty() => Ty::Unit,
            _ => Ty::Unknown,
        }
    }
    value
        .and_then(Value::as_str)
        .and_then(|s| syn::parse_str(s).ok())
        .map_or(Ty::Unknown, |t| of_type(&t))
}
fn primitive(n: &str) -> Ty {
    match n {
        "bool" => Ty::Bool,
        "char" => Ty::Char,
        "str" | "String" => Ty::Str,
        "f64" => Ty::F64,
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "usize" => Ty::Int(n.into()),
        _ => Ty::Unknown,
    }
}
fn fits(n: i64, ty: &str) -> bool {
    match ty {
        "i8" => i8::try_from(n).is_ok(),
        "i16" => i16::try_from(n).is_ok(),
        "i32" => i32::try_from(n).is_ok(),
        "i64" => true,
        "u8" => u8::try_from(n).is_ok(),
        "u16" => u16::try_from(n).is_ok(),
        "u32" => u32::try_from(n).is_ok(),
        "u64" | "usize" => n >= 0,
        _ => false,
    }
}
fn err(meta: &Meta, message: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "unsupported Rust Debug at {}:{}: {message} (CRUSH-xps-9)",
        meta.get("line").unwrap_or(&Value::Null),
        meta.get("column").unwrap_or(&Value::Null)
    )
}
fn inspect(v: &Value, writes: &mut HashSet<String>, active: &mut bool) {
    match v {
        Value::Object(o) => {
            if o.get("rust_debug_pending") == Some(&json!(true)) {
                *active = true;
            }
            if let Some(t) = o.get("target").and_then(Value::as_str) {
                writes.insert(t.into());
            }
            let assignment = o.get("function").and_then(Value::as_str) == Some("__crush_assign__")
                && o.get("meta").and_then(|m| m.get("rust_source_call")) != Some(&json!(true));
            let mutation = matches!(
                o.get("name").and_then(Value::as_str),
                Some("array.push" | "array.pop")
            );
            if (assignment || mutation)
                && let Some(n) = o
                    .get("args")
                    .and_then(Value::as_array)
                    .and_then(|a| a.first())
                    .and_then(|a| a.get("name"))
                    .and_then(Value::as_str)
            {
                writes.insert(n.into());
            }
            for x in o.values() {
                inspect(x, writes, active);
            }
        }
        Value::Array(a) => {
            for x in a {
                inspect(x, writes, active);
            }
        }
        _ => {}
    }
}
#[derive(Clone)]
struct Sig {
    params: Vec<Ty>,
    result: Ty,
}
fn signature(f: &Function) -> Sig {
    Sig {
        params: f
            .meta
            .get("rust_numeric_params")
            .and_then(Value::as_array)
            .map(|p| p.iter().map(|p| hint(Some(p))).collect())
            .unwrap_or_default(),
        result: hint(f.meta.get("rust_numeric_return")),
    }
}
fn numeric_fact(value: Option<&Value>) -> Ty {
    let Some(v) = value.filter(|v| v.get("canonical") == Some(&json!(true))) else {
        return Ty::Unknown;
    };
    v.get("type")
        .and_then(Value::as_str)
        .map_or(Ty::Unknown, |n| match primitive(n) {
            t @ Ty::Int(_) => t,
            _ => Ty::Unknown,
        })
}

pub(crate) fn resolve(program: &mut Program) -> anyhow::Result<()> {
    let mut active = false;
    let mut writes = HashMap::new();
    for (name, f) in &program.functions {
        let mut set = HashSet::new();
        inspect(&serde_json::to_value(&f.body)?, &mut set, &mut active);
        writes.insert(name.clone(), set);
    }
    if !active {
        return Ok(());
    }
    let sigs: HashMap<_, _> = program
        .functions
        .iter()
        .map(|(n, f)| (n.clone(), signature(f)))
        .collect();
    let mut summaries: HashMap<_, _> = sigs.keys().map(|n| (n.clone(), Ty::Unknown)).collect();
    let mut names: Vec<_> = sigs.keys().cloned().collect();
    names.sort();
    for _ in 0..=names.len() {
        let old = summaries.clone();
        for n in &names {
            let mut f = program.functions[n].clone();
            let ty = Check::new(&sigs, &old, &writes[n], false).function(&mut f, &sigs[n])?;
            summaries.insert(n.clone(), ty);
        }
        if old == summaries {
            break;
        }
    }
    for n in names {
        Check::new(&sigs, &summaries, &writes[&n], true)
            .function(program.functions.get_mut(&n).unwrap(), &sigs[&n])?;
    }
    fn pending(v: &Value) -> Option<&Value> {
        match v {
            Value::Object(o) => {
                if let Some(m) = o
                    .get("meta")
                    .filter(|m| m.get("rust_debug_pending") == Some(&json!(true)))
                {
                    return Some(m);
                }
                o.values().find_map(pending)
            }
            Value::Array(a) => a.iter().find_map(pending),
            _ => None,
        }
    }
    if let Some(meta) = pending(&serde_json::to_value(&program.functions)?) {
        return Err(anyhow::anyhow!(
            "unsupported Rust Debug at {}:{}: unvisited formatting expression (CRUSH-xps-9)",
            meta["line"],
            meta["column"]
        ));
    }
    Ok(())
}
struct Check<'a> {
    sigs: &'a HashMap<String, Sig>,
    summaries: &'a HashMap<String, Ty>,
    writes: &'a HashSet<String>,
    locals: HashMap<String, Ty>,
    result: Ty,
    returns: Vec<Ty>,
    strict: bool,
    in_lambda: bool,
    control: bool,
}
impl<'a> Check<'a> {
    fn new(
        sigs: &'a HashMap<String, Sig>,
        summaries: &'a HashMap<String, Ty>,
        writes: &'a HashSet<String>,
        strict: bool,
    ) -> Self {
        Self {
            sigs,
            summaries,
            writes,
            locals: HashMap::new(),
            result: Ty::Unknown,
            returns: vec![],
            strict,
            in_lambda: false,
            control: true,
        }
    }
    fn bind(&mut self, name: &str, ty: Ty) {
        self.locals.insert(
            name.into(),
            if self.writes.contains(name) {
                Ty::Unknown
            } else {
                ty
            },
        );
    }
    fn function(&mut self, f: &mut Function, sig: &Sig) -> anyhow::Result<Ty> {
        self.result = sig.result.clone();
        for ((name, _), ty) in f.params.iter().zip(&sig.params) {
            self.bind(name, ty.clone());
        }
        self.statements(&mut f.body)?;
        let mut result = self
            .returns
            .drain(..)
            .reduce(|a, b| if a == b { a } else { Ty::Unknown })
            .unwrap_or(Ty::Unit);
        let canonical = numeric_fact(f.meta.get("rust_numeric_result"));
        if canonical.known() {
            result = canonical;
        }
        if !self.control || result != sig.result {
            result = Ty::Unknown;
        }
        Ok(result)
    }
    fn statements(&mut self, body: &mut [S]) -> anyhow::Result<()> {
        for s in body {
            match s {
                S::VarDecl {
                    name, value, meta, ..
                } => {
                    let annotation = hint(meta.get("rust_numeric_hint"));
                    let mut ty = self.expr(value, &annotation)?;
                    let fact = numeric_fact(meta.get("rust_numeric_fact"));
                    if self.control && fact.known() {
                        ty = fact;
                    }
                    if meta.get("rust_numeric_hint").is_some_and(|v| !v.is_null())
                        && ty != annotation
                    {
                        ty = Ty::Unknown;
                    }
                    self.bind(name, ty);
                }
                S::ExprStmt { expr, .. } => {
                    self.expr(expr, &Ty::Unknown)?;
                }
                S::Return { value, .. } => {
                    let expected = self.result.clone();
                    let ty = match value {
                        Some(v) => self.expr(v, &expected)?,
                        None => Ty::Unit,
                    };
                    self.returns.push(ty);
                }
                S::Assign { value, .. } => {
                    self.expr(value, &Ty::Unknown)?;
                }
                S::If {
                    condition,
                    then_body,
                    else_body,
                    ..
                } => {
                    self.control &= self.expr(condition, &Ty::Bool)? == Ty::Bool;
                    self.statements(then_body)?;
                    if let Some(b) = else_body {
                        self.statements(b)?;
                    }
                }
                S::While {
                    condition, body, ..
                } => {
                    self.expr(condition, &Ty::Bool)?;
                    self.control = false;
                    self.statements(body)?;
                }
                S::For {
                    variable,
                    iterable,
                    body,
                    ..
                } => {
                    self.expr(iterable, &Ty::Unknown)?;
                    self.control = false;
                    self.bind(variable, Ty::Unknown);
                    self.statements(body)?;
                }
                S::Break { .. } | S::Continue { .. } => {}
                _ => {}
            }
        }
        Ok(())
    }
    fn expr(&mut self, e: &mut E, expected: &Ty) -> anyhow::Result<Ty> {
        Ok(match e {
            E::IntLiteral { value, meta } => {
                let explicit = meta
                    .get("rust_numeric_suffix")
                    .and_then(Value::as_str)
                    .filter(|n| !n.is_empty());
                let name = explicit
                    .map(str::to_owned)
                    .or_else(|| {
                        if let Ty::Int(n) = expected {
                            Some(n.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| "i32".into());
                if fits(*value, &name) && (name != "usize" || *value <= 65535) {
                    Ty::Int(name)
                } else {
                    Ty::Unknown
                }
            }
            E::FloatLiteral { .. } => Ty::F64,
            E::BoolLiteral { .. } => Ty::Bool,
            E::NullLiteral { .. } => Ty::Unit,
            E::StringLiteral { meta, .. } => {
                if meta.get("rust_debug_type") == Some(&json!("char")) {
                    Ty::Char
                } else {
                    Ty::Str
                }
            }
            E::Var { name, .. } => self.locals.get(name).cloned().unwrap_or(Ty::Unknown),
            E::ArrayLiteral { elements, .. } => {
                let mut item = if let Ty::Seq(t) = expected {
                    (**t).clone()
                } else {
                    Ty::Unknown
                };
                if !item.known() {
                    for e in elements.iter_mut() {
                        let seed = match e {
                            E::IntLiteral { meta, .. } => meta
                                .get("rust_numeric_suffix")
                                .and_then(Value::as_str)
                                .map_or(Ty::Unknown, primitive),
                            E::FloatLiteral { .. } => Ty::F64,
                            E::BoolLiteral { .. } => Ty::Bool,
                            E::StringLiteral { meta, .. } => {
                                if meta.get("rust_debug_type") == Some(&json!("char")) {
                                    Ty::Char
                                } else {
                                    Ty::Str
                                }
                            }
                            E::Var { name, .. } => {
                                self.locals.get(name).cloned().unwrap_or(Ty::Unknown)
                            }
                            _ => Ty::Unknown,
                        };
                        if seed.known() {
                            item = seed;
                            break;
                        }
                    }
                }
                let mut result = item.clone();
                for e in elements {
                    let ty = self.expr(e, &item)?;
                    if !item.known() {
                        item = ty.clone();
                        result = ty.clone();
                    }
                    if ty != item {
                        result = Ty::Unknown;
                    }
                }
                Ty::Seq(Box::new(result))
            }
            E::BinaryOp {
                operator,
                left,
                right,
                meta,
            } => {
                let l = self.expr(left, expected)?;
                let r = self.expr(right, &l)?;
                if !l.known() || !r.known() {
                    Ty::Unknown
                } else if meta.get("rust_format_piece") == Some(&json!(true))
                    && l == Ty::Str
                    && r == Ty::Str
                {
                    Ty::Str
                } else if l == Ty::F64
                    && r == Ty::F64
                    && matches!(operator.as_str(), "+" | "-" | "*")
                {
                    Ty::F64
                } else if l == r
                    && l == Ty::Bool
                    && matches!(operator.as_str(), "==" | "!=" | "&&" | "||")
                {
                    Ty::Bool
                } else if l == r
                    && matches!(operator.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=")
                    && (matches!(&l, Ty::F64 | Ty::Str | Ty::Char)
                        || matches!(&l, Ty::Int(n) if matches!(n.as_str(), "i8" | "i16" | "i32" | "u8" | "u16" | "u32")))
                {
                    // Generic Crush ordering converts Int to f64; wider source
                    // integer comparisons need the later exact numeric kernel.
                    Ty::Bool
                } else {
                    Ty::Unknown
                }
            }
            E::UnaryOp {
                operator, operand, ..
            } => {
                let t = self.expr(operand, expected)?;
                match (operator.as_str(), t) {
                    ("-", Ty::F64) => Ty::F64,
                    ("!", Ty::Bool) => Ty::Bool,
                    _ => Ty::Unknown,
                }
            }
            E::Index { target, index, .. } => {
                let ty = self.expr(target, &Ty::Unknown)?;
                let index_ty = self.expr(index, &Ty::Int("usize".into()))?;
                if let Ty::Seq(t) = ty
                    && index_ty == Ty::Int("usize".into())
                {
                    *t
                } else {
                    Ty::Unknown
                }
            }
            E::CapabilityCall { name, args, meta }
                if name == "rust.debug" && meta.get("rust_debug_pending") == Some(&json!(true)) =>
            {
                let ty = self.expr(&mut args[0], &Ty::Unknown)?;
                if self.strict {
                    if !self.control {
                        return Err(err(
                            meta,
                            "Debug needs proven control flow; loops and unknown predicates are deferred",
                        ));
                    }
                    if self.in_lambda {
                        return Err(err(meta, "closure Debug needs typed capture/call analysis"));
                    }
                    let descriptor = ty.descriptor().ok_or_else(|| err(meta, "operand needs a proven Rust type and canonical producer; written bindings and unknown numeric results are refused"))?;
                    meta.remove("rust_debug_pending");
                    args[1] = E::StringLiteral {
                        value: descriptor.to_string(),
                        meta: meta.clone(),
                    };
                    meta.insert("rust_debug_descriptor".into(), descriptor);
                }
                if ty.known() { Ty::Str } else { Ty::Unknown }
            }
            E::Call {
                function,
                args,
                meta,
            } if function == "__crush_ternary__"
                && meta.get("rust_source_call") != Some(&json!(true)) =>
            {
                let condition = self.expr(&mut args[0], &Ty::Bool)?;
                self.control &= condition == Ty::Bool;
                let a = self.expr(&mut args[1], expected)?;
                let b = self.expr(&mut args[2], &a)?;
                if a == b { a } else { Ty::Unknown }
            }
            E::Call {
                function,
                args,
                meta,
            } if function == "len" && meta.get("rust_source_call") != Some(&json!(true)) => {
                let t = self.expr(&mut args[0], &Ty::Unknown)?;
                if t == Ty::Str || matches!(t, Ty::Seq(_)) && t.known() {
                    Ty::Int("usize".into())
                } else {
                    Ty::Unknown
                }
            }
            E::Call {
                function,
                args,
                meta,
            }
            | E::CapabilityCall {
                name: function,
                args,
                meta,
            } if function == "__crush_rust_value__"
                && meta.get("rust_source_call") != Some(&json!(true)) =>
            {
                let receiver = self.expr(&mut args[0], expected)?;
                if matches!(
                    meta.get("rust_value_method").and_then(Value::as_str),
                    Some("clone" | "to_owned" | "to_vec" | "as_slice" | "as_str")
                ) {
                    receiver
                } else {
                    Ty::Unknown
                }
            }
            E::Call {
                function,
                args,
                meta,
            } => {
                let sig = self.sigs.get(function).cloned();
                if self.strict
                    && meta.get("rust_source_call") == Some(&json!(true))
                    && sig.is_none()
                {
                    return Err(err(
                        meta,
                        "indirect calls need typed Debug argument provenance",
                    ));
                }
                if self.strict && sig.is_some() {
                    if function.starts_with("__crush_")
                        || function.starts_with("ai_")
                        || matches!(
                            function.as_str(),
                            "len"
                                | "print"
                                | "println"
                                | "make_range"
                                | "range"
                                | "arr_set"
                                | "arr_get"
                        )
                    {
                        return Err(err(
                            meta,
                            "builtin/helper named source calls need the integrated xps-8 call contract",
                        ));
                    }
                    if args.len() > 1
                        && args.iter().any(|a| {
                            !matches!(
                                a,
                                E::IntLiteral { .. }
                                    | E::FloatLiteral { .. }
                                    | E::BoolLiteral { .. }
                                    | E::StringLiteral { .. }
                                    | E::NullLiteral { .. }
                                    | E::Var { .. }
                            )
                        })
                    {
                        return Err(err(
                            meta,
                            "effectful multiargument source calls need the integrated xps-8 call contract",
                        ));
                    }
                }
                for (n, arg) in args.iter_mut().enumerate() {
                    let want = sig
                        .as_ref()
                        .and_then(|s| s.params.get(n))
                        .cloned()
                        .unwrap_or(Ty::Unknown);
                    let actual = self.expr(arg, &want)?;
                    if self.strict && want.known() && actual != want {
                        return Err(err(
                            meta,
                            "call argument needs a proven compatible Rust type",
                        ));
                    }
                }
                self.summaries.get(function).cloned().unwrap_or(Ty::Unknown)
            }
            E::CapabilityCall { name, args, meta } => {
                let mut types = Vec::new();
                for arg in args {
                    types.push(self.expr(arg, &Ty::Unknown)?);
                }
                if self.strict
                    && meta.get("rust_source_call") == Some(&json!(true))
                    && meta
                        .get("rust_source_name")
                        .and_then(Value::as_str)
                        .is_some_and(|n| self.sigs.contains_key(n))
                {
                    return Err(err(
                        meta,
                        "builtin/helper named source calls need the integrated xps-8 call contract",
                    ));
                }
                if name == "rust.num"
                    && meta
                        .get("rust_numeric_fact")
                        .and_then(|v| v.get("canonical"))
                        == Some(&json!(true))
                {
                    hint(meta.get("rust_numeric_fact").and_then(|v| v.get("type")))
                } else if matches!(
                    name.as_str(),
                    "str.contains" | "rust.str.starts_with" | "rust.str.ends_with"
                ) && types.len() == 2
                    && types.iter().all(|t| *t == Ty::Str)
                {
                    Ty::Bool
                } else if name == "str.replace"
                    && types.len() == 3
                    && types.iter().all(|t| *t == Ty::Str)
                {
                    Ty::Str
                } else if name == "io.print" {
                    Ty::Unit
                } else if (name == "rust.debug" && meta.contains_key("rust_debug_descriptor"))
                    || (name == "conv.text" && types.iter().all(Ty::known))
                    || (matches!(name.as_str(), "str.to_upper" | "str.to_lower" | "str.trim")
                        && types.len() == 1
                        && types.first() == Some(&Ty::Str))
                {
                    Ty::Str
                } else if name == "math.pow"
                    && types.len() == 2
                    && types.iter().all(|t| *t == Ty::F64)
                    && matches!(
                        meta.get("rust_value_method").and_then(Value::as_str),
                        Some("powf")
                    )
                {
                    Ty::F64
                } else {
                    Ty::Unknown
                }
            }
            E::Lambda { body, .. } => {
                let saved = (
                    self.locals.clone(),
                    std::mem::take(&mut self.returns),
                    self.result.clone(),
                    self.in_lambda,
                    self.control,
                );
                self.in_lambda = true;
                self.result = Ty::Unknown;
                self.statements(body)?;
                (
                    self.locals,
                    self.returns,
                    self.result,
                    self.in_lambda,
                    self.control,
                ) = saved;
                Ty::Unknown
            }
            E::Match {
                expression, arms, ..
            } => {
                self.control = false;
                self.expr(expression, &Ty::Unknown)?;
                for arm in arms {
                    self.statements(&mut arm.body)?;
                }
                Ty::Unknown
            }
            E::GetField { target, .. } => {
                self.expr(target, &Ty::Unknown)?;
                Ty::Unknown
            }
            E::Range { start, end, .. } => {
                self.expr(start, &Ty::Unknown)?;
                self.expr(end, &Ty::Unknown)?;
                Ty::Unknown
            }
            _ => Ty::Unknown,
        })
    }
}
