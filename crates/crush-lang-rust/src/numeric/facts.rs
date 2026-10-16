//! Conservative numeric provenance for dynamic casts. Runs after lexical
//! resolution. A declared type never substitutes for a canonical-value proof.
use std::collections::{HashMap, HashSet};

use crush_cast::{Expression as E, Function, Pattern, Program, Statement as S};
use serde_json::{Value, json};

use super::{CAST, Integer, Meta};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Ty {
    Int(Integer),
    LiteralInt,
    Bool,
    F64,
    Text(Option<usize>),
    Seq(Box<Fact>, Option<i128>),
    Range(Box<Fact>),
    Other,
    Unknown,
}

impl Ty {
    fn numeric(&self) -> bool {
        match self {
            Ty::Int(_) | Ty::F64 => true,
            Ty::Seq(item, _) | Ty::Range(item) => item.ty.numeric(),
            _ => false,
        }
    }
    fn bounded(&self) -> bool {
        match self {
            Ty::Int(i) => i.bounded(),
            Ty::Seq(item, _) | Ty::Range(item) => item.ty.bounded(),
            _ => false,
        }
    }
}

fn hint(value: Option<&Value>) -> Ty {
    fn of_type(t: &syn::Type) -> Ty {
        match t {
            syn::Type::Array(a) => Ty::Seq(Box::new(Fact::parameter(of_type(&a.elem))), None),
            syn::Type::Reference(r) if matches!(r.elem.as_ref(), syn::Type::Path(p) if p.path.is_ident("str")) => {
                Ty::Text(None)
            }
            syn::Type::Path(p)
                if p.path.segments.len() == 1 && p.path.segments[0].ident == "Vec" =>
            {
                if let syn::PathArguments::AngleBracketed(a) = &p.path.segments[0].arguments
                    && let Some(syn::GenericArgument::Type(t)) = a.args.first()
                {
                    return Ty::Seq(Box::new(Fact::parameter(of_type(t))), None);
                }
                Ty::Unknown
            }
            syn::Type::Path(p) if p.path.segments.len() == 1 => {
                let name = p.path.segments[0].ident.to_string();
                Integer::parse(&name)
                    .map(Ty::Int)
                    .unwrap_or_else(|_| match name.as_str() {
                        "bool" => Ty::Bool,
                        "String" | "str" => Ty::Text(None),
                        "f64" => Ty::F64,
                        "char" => Ty::Other,
                        _ => Ty::Unknown,
                    })
            }
            _ => Ty::Unknown,
        }
    }
    match value.and_then(Value::as_str) {
        Some("bool") => Ty::Bool,
        Some("str" | "& str" | "String") => Ty::Text(None),
        Some("()" | "char") => Ty::Other,
        Some("f64") => Ty::F64,
        Some(name) => Integer::parse(name).map(Ty::Int).unwrap_or_else(|_| {
            syn::parse_str(name)
                .map(|t| of_type(&t))
                .unwrap_or(Ty::Unknown)
        }),
        None => Ty::Unknown,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Fact {
    ty: Ty,
    range: Option<(i128, i128)>,
    exact: bool,
}

impl Fact {
    fn unknown(ty: Ty) -> Self {
        Self {
            ty,
            range: None,
            exact: false,
        }
    }
    fn scalar(ty: Ty) -> Self {
        Self {
            ty,
            range: None,
            exact: true,
        }
    }
    fn parameter(ty: Ty) -> Self {
        match ty {
            Ty::Int(i) => Self {
                ty,
                range: Some(if i.bounded() {
                    (0, i64::MAX as i128)
                } else {
                    i.bounds()
                }),
                exact: true,
            },
            Ty::Unknown => Self::unknown(ty),
            _ => Self::scalar(ty),
        }
    }
    fn coerce(self, ty: &Ty) -> Self {
        if matches!((&self.ty, ty), (Ty::Text(_), Ty::Text(_))) {
            return self;
        }
        if let (Ty::Seq(item, len), Ty::Seq(want, _)) = (&self.ty, ty) {
            let item = (**item).clone().coerce(&want.ty);
            return Self {
                ty: Ty::Seq(Box::new(item.clone()), *len),
                range: None,
                exact: self.exact && (item.exact || *len == Some(0)),
            };
        }
        if let Ty::Int(i) = ty {
            let compatible = self.ty == *ty || self.ty == Ty::LiteralInt;
            let fits = self.range.is_some_and(|(lo, hi)| {
                lo >= i64::MIN as i128
                    && hi <= i64::MAX as i128
                    && if i.name == "usize" && self.ty == *ty {
                        lo >= 0
                    } else {
                        i.check(lo).is_ok() && i.check(hi).is_ok()
                    }
            });
            return Self {
                ty: ty.clone(),
                exact: self.exact && compatible && fits,
                range: self.range.filter(|_| compatible && fits),
            };
        }
        if self.ty == *ty {
            self
        } else {
            Self::unknown(ty.clone())
        }
    }
    fn join(self, other: Self) -> Self {
        if let (Ty::Seq(a, n), Ty::Seq(b, m)) = (&self.ty, &other.ty) {
            return Self {
                ty: Ty::Seq(
                    Box::new((**a).clone().join((**b).clone())),
                    if n == m { *n } else { None },
                ),
                range: None,
                exact: self.exact && other.exact,
            };
        }
        if let (Ty::Text(n), Ty::Text(m)) = (&self.ty, &other.ty) {
            return Self {
                ty: Ty::Text(if n == m { *n } else { None }),
                range: None,
                exact: self.exact && other.exact,
            };
        }
        if self.ty != other.ty {
            return Self::unknown(Ty::Unknown);
        }
        let range = match (self.range, other.range) {
            (Some((a, b)), Some((c, d))) => Some((a.min(c), b.max(d))),
            _ => None,
        };
        Self {
            ty: self.ty,
            range,
            exact: self.exact && other.exact,
        }
    }
    fn json(&self) -> Value {
        json!({"type": match &self.ty {
            Ty::Int(i) => i.name, Ty::LiteralInt => "unresolved integer literal",
            Ty::F64 => "f64", Ty::Bool => "bool", Ty::Other => "noninteger", Ty::Unknown => "unknown",
            Ty::Text(_) => "text", Ty::Seq(..) => "sequence", Ty::Range(_) => "range",
        }, "canonical": self.exact, "range": self.range})
    }
}

#[derive(Clone)]
struct Signature {
    params: Vec<Ty>,
    result: Ty,
}

fn signature(f: &Function) -> Signature {
    Signature {
        params: f
            .meta
            .get("rust_numeric_params")
            .and_then(Value::as_array)
            .map(|p| p.iter().map(|p| hint(Some(p))).collect())
            .unwrap_or_default(),
        result: hint(f.meta.get("rust_numeric_return")),
    }
}

// Inspect all lexical writes before proving any use, including writes hidden
// in a branch, loop or captured closure. Written storage is deliberately not
// treated as its initializer's constant value. Names are xps-6 binding IDs.
fn inspect(value: &Value, writes: &mut HashSet<String>, casts: &mut bool, methods: &mut bool) {
    fn wide(v: &Value, pointer: bool) -> bool {
        match v {
            Value::String(s) => s
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .any(|w| w == "u64" || pointer && w == "usize"),
            Value::Array(a) => a.iter().any(|v| wide(v, pointer)),
            _ => false,
        }
    }
    match value {
        Value::Object(o) => {
            if o.get("name").and_then(Value::as_str) == Some("__crush_rust_numeric_method__") {
                *methods = true;
            }
            if o.get("function").and_then(Value::as_str) == Some(CAST) {
                *casts = true;
            }
            if o.iter()
                .any(|(k, v)| k.starts_with("rust_numeric_") && wide(v, k == "rust_numeric_suffix"))
            {
                *casts = true;
            }
            if matches!(
                o.get("name").and_then(Value::as_str),
                Some("array.push" | "array.pop")
            ) && let Some(name) = o
                .get("args")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(|a| a.get("name"))
                .and_then(Value::as_str)
            {
                writes.insert(name.into());
            }
            if o.get("function").and_then(Value::as_str) == Some("__crush_assign__")
                && o.get("meta").and_then(|m| m.get("rust_source_call")) != Some(&json!(true))
                && let Some(name) = o
                    .get("args")
                    .and_then(Value::as_array)
                    .and_then(|a| a.first())
                    .and_then(|a| a.get("name"))
                    .and_then(Value::as_str)
            {
                writes.insert(name.into());
            }
            if let Some(target) = o.get("target").and_then(Value::as_str) {
                writes.insert(target.into());
            }
            for v in o.values() {
                inspect(v, writes, casts, methods);
            }
        }
        Value::Array(a) => {
            for v in a {
                inspect(v, writes, casts, methods);
            }
        }
        _ => {}
    }
}

pub(crate) fn check(program: &mut Program) -> anyhow::Result<()> {
    let mut active = false;
    let mut methods = false;
    let mut writes = HashMap::new();
    for (name, f) in &program.functions {
        let mut set = HashSet::new();
        inspect(
            &serde_json::to_value(&f.body)?,
            &mut set,
            &mut active,
            &mut methods,
        );
        inspect(
            &serde_json::to_value(&f.meta)?,
            &mut set,
            &mut active,
            &mut methods,
        );
        writes.insert(name.clone(), set);
    }
    // Keep the legacy arithmetic inventory unchanged when no dynamic cast
    // needs this proof domain. Literal casts were already evaluated exactly.
    if !active && !methods {
        return Ok(());
    }
    let signatures: HashMap<_, _> = program
        .functions
        .iter()
        .map(|(n, f)| (n.clone(), signature(f)))
        .collect();
    let mut summaries: HashMap<_, _> = signatures
        .iter()
        .map(|(n, s)| (n.clone(), Fact::unknown(s.result.clone())))
        .collect();
    let mut names: Vec<_> = program.functions.keys().cloned().collect();
    names.sort();
    // Least fixed point: recursive/unproven returns never start out trusted.
    // Each acyclic call edge can acquire a proof on the following iteration.
    for _ in 0..=names.len() {
        let old = summaries.clone();
        for name in &names {
            let mut f = program.functions[name].clone();
            let mut check = Check::new(&signatures, &old, &writes[name], false);
            let result = check.function(&mut f, &signatures[name])?;
            summaries.insert(name.clone(), result);
        }
        if summaries == old {
            break;
        }
    }
    for name in names {
        let mut check = Check::new(&signatures, &summaries, &writes[&name], active);
        check.final_pass = true;
        check.method_domain = methods;
        let f = program.functions.get_mut(&name).unwrap();
        let result = check.function(f, &signatures[&name])?;
        f.meta.insert("rust_numeric_result".into(), result.json());
    }
    Ok(())
}

fn error(meta: &Meta, reason: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "unsupported Rust numeric operation at {}:{}: {reason} (CRUSH-xps-8)",
        meta.get("line").unwrap_or(&Value::Null),
        meta.get("column").unwrap_or(&Value::Null)
    )
}

struct Check<'a> {
    signatures: &'a HashMap<String, Signature>,
    summaries: &'a HashMap<String, Fact>,
    writes: &'a HashSet<String>,
    locals: HashMap<String, Fact>,
    returns: Vec<Fact>,
    result: Ty,
    control_exact: bool,
    in_lambda: bool,
    strict: bool,
    final_pass: bool,
    method_domain: bool,
}

impl<'a> Check<'a> {
    fn new(
        signatures: &'a HashMap<String, Signature>,
        summaries: &'a HashMap<String, Fact>,
        writes: &'a HashSet<String>,
        strict: bool,
    ) -> Self {
        Self {
            signatures,
            summaries,
            writes,
            locals: HashMap::new(),
            returns: vec![],
            result: Ty::Unknown,
            control_exact: true,
            in_lambda: false,
            strict,
            final_pass: false,
            method_domain: false,
        }
    }
    fn bind(&mut self, name: &str, mut fact: Fact) -> Fact {
        if self.writes.contains(name) {
            fact.exact = false;
            fact.range = None;
            match &mut fact.ty {
                Ty::Seq(item, len) => {
                    *len = None;
                    **item = Fact::unknown(item.ty.clone());
                }
                Ty::Text(len) => *len = None,
                _ => {}
            }
        }
        self.locals.insert(name.into(), fact.clone());
        fact
    }
    fn function(&mut self, f: &mut Function, sig: &Signature) -> anyhow::Result<Fact> {
        self.result = sig.result.clone();
        let params: Vec<_> = f
            .params
            .iter()
            .zip(&sig.params)
            .map(|((name, _), ty)| {
                let fact = self.bind(name, Fact::parameter(ty.clone()));
                json!({"storage": name, "fact": fact.json()})
            })
            .collect();
        f.meta
            .insert("rust_numeric_parameters".into(), json!(params));
        self.statements(&mut f.body)?;
        let mut result = self
            .returns
            .drain(..)
            .reduce(Fact::join)
            .unwrap_or_else(|| Fact::scalar(Ty::Other));
        result.exact &= self.control_exact;
        Ok(result.coerce(&sig.result))
    }
    fn statements(&mut self, body: &mut [S]) -> anyhow::Result<()> {
        for s in body {
            match s {
                S::VarDecl {
                    name, value, meta, ..
                } => {
                    let ty = hint(meta.get("rust_numeric_hint"));
                    let expected = if let Ty::Int(i) = ty { Some(i) } else { None };
                    let mut fact = self.expr(value, expected)?;
                    if meta.get("rust_numeric_hint").is_some_and(|v| !v.is_null()) {
                        fact = fact.coerce(&ty);
                    }
                    if self.strict && fact.ty.bounded() && !fact.exact {
                        return Err(error(
                            meta,
                            "bounded unsigned binding needs a proven canonical integer value",
                        ));
                    }
                    self.control_exact &= fact.exact;
                    let fact = self.bind(name, fact);
                    meta.insert("rust_numeric_fact".into(), fact.json());
                }
                S::ExprStmt { expr, .. } => {
                    let fact = self.expr(expr, None)?;
                    self.control_exact &= fact.exact;
                }
                S::Return { value, meta } => {
                    let expected = if let Ty::Int(i) = self.result {
                        Some(i)
                    } else {
                        None
                    };
                    let fact = match value {
                        Some(e) => self.expr(e, expected)?,
                        None => Fact::scalar(Ty::Other),
                    };
                    let fact = fact.coerce(&self.result);
                    if self.strict && self.result.bounded() && !fact.exact {
                        return Err(error(
                            meta,
                            "bounded unsigned return needs a proven canonical integer value",
                        ));
                    }
                    self.returns.push(fact);
                }
                S::Assign { value, .. } => {
                    let fact = self.expr(value, None)?;
                    self.control_exact &= fact.exact;
                }
                S::If {
                    condition,
                    then_body,
                    else_body,
                    ..
                } => {
                    let cond = self.expr(condition, None)?;
                    self.control_exact &= cond.exact && cond.ty == Ty::Bool;
                    self.statements(then_body)?;
                    if let Some(body) = else_body {
                        self.statements(body)?;
                    }
                }
                S::While {
                    condition, body, ..
                } => {
                    self.expr(condition, None)?;
                    self.control_exact = false;
                    self.statements(body)?;
                }
                S::For {
                    variable,
                    iterable,
                    body,
                    ..
                } => {
                    let iterable = self.expr(iterable, None)?;
                    let item = match iterable.ty {
                        Ty::Range(item) | Ty::Seq(item, _) if iterable.exact => *item,
                        _ => Fact::unknown(Ty::Unknown),
                    };
                    self.bind(variable, item);
                    self.control_exact = false;
                    self.statements(body)?;
                }
                S::Break { .. } | S::Continue { .. } => {}
                _ => anyhow::bail!("unsupported Rust numeric statement proof (CRUSH-xps-8)"),
            }
        }
        Ok(())
    }
    fn peek(&self, e: &E) -> Option<Integer> {
        match e {
            E::Var { name, .. } => self
                .locals
                .get(name)
                .and_then(|f| if let Ty::Int(i) = f.ty { Some(i) } else { None }),
            E::IntLiteral { meta, .. } => meta
                .get("rust_numeric_suffix")
                .and_then(Value::as_str)
                .and_then(|s| Integer::parse(s).ok()),
            E::Call { function, meta, .. } => meta
                .get("rust_numeric_suffix")
                .and_then(Value::as_str)
                .and_then(|s| Integer::parse(s).ok())
                .or_else(|| {
                    self.signatures.get(function).and_then(|s| {
                        if let Ty::Int(i) = s.result {
                            Some(i)
                        } else {
                            None
                        }
                    })
                }),
            E::UnaryOp { operand, .. } => self.peek(operand),
            E::BinaryOp { left, right, .. } => self.peek(left).or_else(|| self.peek(right)),
            E::Index { target, .. } => {
                let ty = match target.as_ref() {
                    E::Var { name, .. } => self.locals.get(name).map(|f| &f.ty),
                    E::Call { function, .. } => self.signatures.get(function).map(|s| &s.result),
                    _ => None,
                };
                ty.and_then(|t| {
                    if let Ty::Seq(item, _) = t {
                        if let Ty::Int(i) = item.ty {
                            Some(i)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
            }
            _ => None,
        }
    }
    fn expr(&mut self, e: &mut E, expected: Option<Integer>) -> anyhow::Result<Fact> {
        Ok(match e {
            E::IntLiteral { value, meta } => {
                let ty = meta
                    .get("rust_numeric_suffix")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .and_then(|s| Integer::parse(s).ok())
                    .or(expected)
                    .map(Ty::Int)
                    .unwrap_or(Ty::LiteralInt);
                let fact = Fact {
                    ty: ty.clone(),
                    range: Some((*value as i128, *value as i128)),
                    exact: true,
                };
                if let Ty::Int(i) = ty
                    && i.name == "usize"
                    && i.check(*value as i128).is_err()
                {
                    return Ok(Fact::unknown(ty));
                }
                if let Ty::Int(_) = ty {
                    fact.coerce(&ty)
                } else {
                    fact
                }
            }
            E::BoolLiteral { .. } => Fact::scalar(Ty::Bool),
            E::StringLiteral { meta, .. }
                if meta.get("rust_debug_type").and_then(Value::as_str) == Some("char") =>
            {
                Fact::scalar(Ty::Other)
            }
            E::StringLiteral { value, .. } => Fact {
                ty: Ty::Text(Some(value.len())),
                range: None,
                exact: !value.contains('\0'),
            },
            E::FloatLiteral { .. } => Fact::scalar(Ty::F64),
            E::NullLiteral { .. } => Fact::scalar(Ty::Other),
            E::Var { name, .. } => self
                .locals
                .get(name)
                .cloned()
                .unwrap_or_else(|| Fact::unknown(Ty::Unknown)),
            E::UnaryOp {
                operator,
                operand,
                meta,
            } => {
                let fact = self.expr(operand, expected)?;
                if operator == "-" {
                    if fact.ty == Ty::F64 {
                        fact
                    } else if let (Ty::Int(i), Some((lo, hi))) = (&fact.ty, fact.range) {
                        Fact {
                            ty: fact.ty.clone(),
                            range: Some((-hi, -lo)),
                            exact: fact.exact,
                        }
                        .coerce(&Ty::Int(*i))
                    } else {
                        Fact::unknown(fact.ty)
                    }
                } else if operator == "!" && fact.ty == Ty::Bool {
                    fact
                } else {
                    if self.strict && fact.ty.bounded() {
                        return Err(error(
                            meta,
                            "bounded unsigned unary operation needs a proven canonical integer result",
                        ));
                    }
                    Fact::unknown(fact.ty)
                }
            }
            E::BinaryOp {
                operator,
                left,
                right,
                meta,
            } => {
                let ty = self.peek(left).or_else(|| self.peek(right)).or(expected);
                let l = self.expr(left, ty)?;
                let r = self.expr(right, ty)?;
                if meta.get("rust_format_piece") == Some(&json!(true)) {
                    Fact {
                        ty: Ty::Other,
                        range: None,
                        exact: l.exact && r.exact,
                    }
                } else {
                    let numeric = |f: &Fact| matches!(f.ty, Ty::Int(_) | Ty::LiteralInt | Ty::F64);
                    if self.method_domain
                        && self.final_pass
                        && matches!(operator.as_str(), "<" | "<=" | ">" | ">=")
                    {
                        let wide = [&l, &r].iter().any(|f| {
                            matches!(f.ty, Ty::Int(_) | Ty::LiteralInt)
                                && !f.range.is_some_and(|(lo, hi)| {
                                    lo >= -9007199254740992 && hi <= 9007199254740992
                                })
                        });
                        if wide {
                            return Err(crate::methods::error(
                                meta,
                                "ordered integer comparison needs an exact f64-representable range before typed method results can enter generic ordering",
                            ));
                        }
                    }
                    let numeric_operands = numeric(&l) || numeric(&r);
                    let float_divisor =
                        matches!(right.as_ref(), E::FloatLiteral { value,.. } if *value != 0.0);
                    let fact = if l.ty == Ty::F64
                        && r.ty == Ty::F64
                        && matches!(operator.as_str(), "/" | "%")
                        && float_divisor
                    {
                        Fact {
                            ty: Ty::F64,
                            range: None,
                            exact: l.exact && r.exact,
                        }
                    } else {
                        binary(operator, l, r, ty)
                    };
                    if self.method_domain && self.final_pass && numeric_operands && !fact.exact {
                        return Err(crate::methods::error(
                            meta,
                            "numeric method program needs canonical typed arithmetic; unproven overflow and division domains are unsupported",
                        ));
                    }
                    if self.strict && fact.ty.bounded() && !fact.exact {
                        return Err(error(
                            meta,
                            "bounded unsigned arithmetic needs a proven canonical integer result",
                        ));
                    }
                    fact
                }
            }
            E::Call {
                function,
                args,
                meta,
            } if function == CAST => {
                if meta.get("rust_source_call") == Some(&json!(true)) {
                    return Err(error(meta, "reserved Rust numeric compiler helper"));
                }
                let to = meta
                    .get("rust_numeric_suffix")
                    .and_then(Value::as_str)
                    .and_then(|s| Integer::parse(s).ok())
                    .ok_or_else(|| error(meta, "missing generated integer cast type"))?;
                let from = self.expr(&mut args[0], Some(to))?;
                let bound = from.range.is_some_and(|(lo, hi)| {
                    lo >= 0
                        && hi <= i64::MAX as i128
                        && (to.name != "usize" || from.ty == Ty::Int(to) || hi <= 65535)
                });
                let valid = from.exact
                    && from.range.is_some()
                    && matches!(from.ty, Ty::Int(_) | Ty::LiteralInt)
                    && !self.in_lambda
                    && (!to.bounded() || bound);
                if !valid && self.strict {
                    return Err(error(
                        meta,
                        if self.in_lambda {
                            "closure casts need numeric capture/call analysis"
                        } else if from.exact && to.bounded() && !bound {
                            if to.name == "usize" {
                                "usize cast needs a nonnegative portable bound (0..=65535), or existing usize provenance"
                            } else {
                                "u64 cast needs a nonnegative value representable in Crush Int"
                            }
                        } else {
                            "cast operand needs a proven canonical integer; written bindings, unsupported numeric producers and unproven arithmetic are refused"
                        },
                    ));
                }
                meta.insert("rust_numeric_cast_proof".into(), from.json());
                let range = from
                    .range
                    .filter(|(lo, hi)| {
                        to.bounded() || to.check(*lo).is_ok() && to.check(*hi).is_ok()
                    })
                    .or_else(|| {
                        from.range
                            .filter(|(lo, hi)| lo == hi)
                            .map(|(n, _)| (to.convert(n), to.convert(n)))
                    })
                    .unwrap_or_else(|| to.bounds());
                Fact {
                    ty: Ty::Int(to),
                    range: Some(range),
                    exact: valid,
                }
            }
            E::Call {
                function,
                args,
                meta,
            } if function == "__crush_ternary__"
                && meta.get("rust_source_call") != Some(&json!(true)) =>
            {
                let expected = self
                    .peek(&args[1])
                    .or_else(|| self.peek(&args[2]))
                    .or(expected);
                let cond = self.expr(&mut args[0], None)?;
                let yes = self.expr(&mut args[1], expected)?;
                let no = self.expr(&mut args[2], expected)?;
                let mut fact = if let Some(i) = expected {
                    yes.coerce(&Ty::Int(i)).join(no.coerce(&Ty::Int(i)))
                } else {
                    yes.join(no)
                };
                fact.exact &= cond.exact && cond.ty == Ty::Bool;
                fact
            }
            E::Call {
                function,
                args,
                meta,
            } if function == "len" && meta.get("rust_source_call") != Some(&json!(true)) => {
                let value = self.expr(&mut args[0], None)?;
                let length = match value.ty {
                    Ty::Seq(_, n) => Some(n),
                    Ty::Text(n) => Some(n.map(|n| n as i128)),
                    _ => None,
                };
                if self.strict && (!value.exact || length.is_none()) {
                    return Err(error(
                        meta,
                        "length source needs proven unmodified sequence or non-NUL text provenance",
                    ));
                }
                if value.exact
                    && let Some(n) = length
                {
                    Fact {
                        ty: Ty::Int(Integer::parse("usize")?),
                        range: Some(n.map(|n| (n, n)).unwrap_or((0, i64::MAX as i128))),
                        exact: true,
                    }
                } else {
                    Fact::unknown(Ty::Int(Integer::parse("usize")?))
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
                && meta.get("rust_source_call") != Some(&json!(true))
                && matches!(
                    meta.get("rust_value_method").and_then(Value::as_str),
                    Some("clone" | "to_owned" | "to_vec" | "as_slice" | "as_str" | "iter")
                ) =>
            {
                self.expr(&mut args[0], None)?
            }
            E::Call {
                function,
                args,
                meta,
            } => {
                let sig = self.signatures.get(function).cloned();
                if self.method_domain
                    && self.final_pass
                    && meta.get("rust_source_call") == Some(&json!(true))
                    && sig.is_none()
                {
                    return Err(crate::methods::error(
                        meta,
                        "indirect calls need numeric argument provenance in a typed method program",
                    ));
                }
                if self.strict
                    && meta.get("rust_source_call") == Some(&json!(true))
                    && sig.is_none()
                {
                    return Err(error(
                        meta,
                        "indirect calls need numeric argument provenance in a program with dynamic casts",
                    ));
                }
                let mut exact = true;
                for (n, arg) in args.iter_mut().enumerate() {
                    let ty = sig.as_ref().and_then(|s| s.params.get(n));
                    let expected = if let Some(Ty::Int(i)) = ty {
                        Some(*i)
                    } else {
                        None
                    };
                    let fact = self.expr(arg, expected)?;
                    let fact = if let Some(ty) = ty {
                        fact.coerce(ty)
                    } else {
                        fact
                    };
                    if self.method_domain
                        && self.final_pass
                        && ty.is_some_and(Ty::numeric)
                        && !fact.exact
                    {
                        return Err(crate::methods::error(
                            meta,
                            "numeric call argument needs canonical value provenance before a typed method; unproven arithmetic is unsupported",
                        ));
                    }
                    if ty.is_some() && !fact.exact && self.strict {
                        return Err(error(
                            meta,
                            if matches!(ty, Some(Ty::Int(_))) {
                                "integer call argument needs a canonical value proof in a program with dynamic casts"
                            } else {
                                "call argument needs a value provenance proof in a program with dynamic casts"
                            },
                        ));
                    }
                    exact &= fact.exact;
                }
                let mut fact = self
                    .summaries
                    .get(function)
                    .cloned()
                    .unwrap_or_else(|| Fact::unknown(Ty::Unknown));
                fact.exact &= exact;
                fact
            }
            E::CapabilityCall { name, args, meta } if name == "__crush_rust_numeric_method__" => {
                let method = meta
                    .get("rust_value_method")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                let receiver =
                    self.expr(&mut args[0], expected.or(Some(Integer::parse("i32")?)))?;
                let exp_type = Integer::parse(if method == "powi" { "i32" } else { "u32" })?;
                let arg = if args.len() == 2 {
                    Some(self.expr(
                        &mut args[1],
                        if method == "powf" {
                            None
                        } else {
                            Some(exp_type)
                        },
                    )?)
                } else {
                    None
                };
                let mut valid = receiver.exact && !self.in_lambda;
                let mut result = receiver.clone();
                match &receiver.ty {
                    Ty::F64 => {
                        valid &= match method.as_str() {
                            "abs" | "sqrt" | "floor" | "ceil" | "round" => true,
                            "powf" => arg.as_ref().is_some_and(|a| a.exact && a.ty == Ty::F64),
                            "powi" => arg
                                .as_ref()
                                .is_some_and(|a| a.exact && a.ty == Ty::Int(exp_type)),
                            _ => false,
                        };
                    }
                    Ty::Int(i) => {
                        valid &= method == "pow"
                            && arg
                                .as_ref()
                                .is_some_and(|a| a.exact && a.ty == Ty::Int(exp_type))
                            || method == "abs" && i.signed;
                        result.range = Some(i.bounds());
                        // Refine proven constants without folding away the runtime
                        // call or duplicating its evaluation/effects.
                        if let Some((a, b)) = receiver.range.filter(|(a, b)| a == b) {
                            let _ = b;
                            let value = if method == "abs" {
                                a.checked_abs()
                            } else {
                                arg.as_ref()
                                    .and_then(|f| f.range)
                                    .filter(|(a, b)| a == b)
                                    .and_then(|(e, _)| u32::try_from(e).ok())
                                    .and_then(|e| a.checked_pow(e))
                            };
                            if let Some(value) = value {
                                let n = i.convert(value);
                                result.range = Some((n, n));
                            }
                        }
                        // Bounded unsigned results need an exact evaluation or an
                        // identity proof before entering the guest signed Int.
                        if i.bounded() {
                            let exponent = arg
                                .as_ref()
                                .and_then(|a| a.range)
                                .filter(|(lo, hi)| lo == hi)
                                .map(|(n, _)| n);
                            let base = receiver.range.filter(|(lo, hi)| lo == hi).map(|(n, _)| n);
                            let exact = base
                                .zip(exponent)
                                .and_then(|(a, b)| {
                                    u32::try_from(b).ok().and_then(|e| a.checked_pow(e))
                                })
                                .filter(|n| {
                                    *n >= 0
                                        && *n <= i64::MAX as i128
                                        && (i.name != "usize" || *n <= 65535)
                                });
                            result.range = exact.map(|n| (n, n)).or_else(|| {
                                if exponent == Some(1) {
                                    receiver.range
                                } else if exponent == Some(0) {
                                    Some((1, 1))
                                } else {
                                    None
                                }
                            });
                            valid &= result.range.is_some_and(|(lo, hi)| {
                                lo >= 0
                                    && hi
                                        <= if i.name == "usize" {
                                            65535
                                        } else {
                                            i64::MAX as i128
                                        }
                            });
                        }
                    }
                    _ => valid = false,
                }
                result.exact = valid;
                if self.final_pass {
                    if !valid {
                        return Err(crate::methods::error(
                            meta,
                            "numeric method needs proven canonical receiver/exponent types and representable result; written/closure/unknown producers are unsupported",
                        ));
                    }
                    let descriptor = match &receiver.ty {
                        Ty::F64 => "f64",
                        Ty::Int(i) => i.name,
                        _ => unreachable!(),
                    };
                    let mut lowered = vec![
                        E::StringLiteral {
                            value: method,
                            meta: Meta::new(),
                        },
                        E::StringLiteral {
                            value: descriptor.into(),
                            meta: Meta::new(),
                        },
                    ];
                    lowered.append(args);
                    if lowered.len() == 3 {
                        lowered.push(E::NullLiteral { meta: Meta::new() });
                    }
                    *args = lowered;
                    *name = "rust.num".into();
                    meta.insert("rust_numeric_fact".into(), result.json());
                }
                result
            }
            E::CapabilityCall { name, args, meta } => {
                let mut exact = true;
                let mut text = None;
                for (n, arg) in args.iter_mut().enumerate() {
                    let fact = self.expr(arg, None)?;
                    if self.strict
                        && fact.ty.bounded()
                        && (!fact.exact
                            || !matches!(name.as_str(), "conv.text" | "io.print")
                            || !matches!(fact.ty, Ty::Int(_)))
                    {
                        return Err(error(
                            meta,
                            "bounded unsigned method needs a proven compatible numeric result",
                        ));
                    }
                    if n == 0 && matches!(fact.ty, Ty::Text(_)) {
                        text = Some(fact.clone());
                    }
                    exact &= fact.exact;
                }
                if matches!(
                    name.as_str(),
                    "str.contains" | "rust.str.starts_with" | "rust.str.ends_with"
                ) && text.is_some()
                    && exact
                {
                    Fact::scalar(Ty::Bool)
                } else if matches!(
                    name.as_str(),
                    "str.to_lower" | "str.to_upper" | "str.trim" | "str.replace"
                ) && text.is_some()
                    && exact
                {
                    Fact::scalar(Ty::Text(None))
                } else if name == "conv.text"
                    && let Some(mut fact) = text
                {
                    fact.exact &= exact;
                    fact
                } else if matches!(name.as_str(), "conv.text" | "io.print") {
                    Fact {
                        ty: Ty::Other,
                        range: None,
                        exact,
                    }
                } else {
                    Fact::unknown(Ty::Unknown)
                }
            }
            E::ArrayLiteral { elements, .. } => {
                let mut exact = true;
                let len = elements.len() as i128;
                let expected = elements.iter().find_map(|e| self.peek(e));
                let mut item: Option<Fact> = None;
                for e in elements {
                    let fact = self.expr(e, expected)?;
                    exact &= fact.exact;
                    item = Some(match item {
                        Some(old) => old.join(fact),
                        None => fact,
                    });
                }
                Fact {
                    ty: Ty::Seq(
                        Box::new(item.unwrap_or_else(|| Fact::unknown(Ty::Unknown))),
                        Some(len),
                    ),
                    range: None,
                    exact,
                }
            }
            E::Lambda { body, .. } => {
                let saved = (
                    self.locals.clone(),
                    std::mem::take(&mut self.returns),
                    self.control_exact,
                    self.in_lambda,
                );
                self.in_lambda = true;
                self.statements(body)?;
                (
                    self.locals,
                    self.returns,
                    self.control_exact,
                    self.in_lambda,
                ) = saved;
                Fact::unknown(Ty::Unknown)
            }
            E::Index { target, index, .. } => {
                let seq = self.expr(target, None)?;
                let idx = self.expr(index, Some(Integer::parse("usize")?))?;
                if let Ty::Seq(mut item, len) = seq.ty {
                    item.exact &= seq.exact
                        && idx.exact
                        && idx
                            .range
                            .is_some_and(|(lo, hi)| lo >= 0 && len.is_some_and(|n| hi < n));
                    *item
                } else {
                    Fact::unknown(Ty::Unknown)
                }
            }
            E::Range { start, end, .. } => {
                let start = self.expr(start, None)?;
                let end = self.expr(end, None)?;
                let ty = if matches!(end.ty, Ty::Int(_)) {
                    end.ty.clone()
                } else {
                    start.ty.clone()
                };
                let start = start.coerce(&ty);
                let end = end.coerce(&ty);
                let exact = start.exact && end.exact;
                let range = match (start.range, end.range) {
                    (Some((lo, _)), Some((_, hi))) => {
                        Some(if lo < hi { (lo, hi - 1) } else { (0, 0) })
                    }
                    _ => None,
                };
                Fact {
                    ty: Ty::Range(Box::new(Fact {
                        ty,
                        range,
                        exact: exact && range.is_some(),
                    })),
                    range: None,
                    exact,
                }
            }
            E::GetField { target, .. } => {
                self.expr(target, None)?;
                Fact::unknown(Ty::Unknown)
            }
            E::Match {
                expression, arms, ..
            } => {
                self.expr(expression, None)?;
                for arm in arms {
                    if let Pattern::Identifier { name } = &arm.pattern {
                        self.bind(name, Fact::unknown(Ty::Unknown));
                    }
                    self.statements(&mut arm.body)?;
                }
                Fact::unknown(Ty::Unknown)
            }
            _ => anyhow::bail!("unsupported Rust numeric expression proof (CRUSH-xps-8)"),
        })
    }
}

fn binary(op: &str, left: Fact, right: Fact, ty: Option<Integer>) -> Fact {
    if matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||") {
        return Fact {
            ty: Ty::Bool,
            range: None,
            exact: left.exact && right.exact,
        };
    }
    if left.ty == Ty::F64 && right.ty == Ty::F64 && matches!(op, "+" | "-" | "*") {
        return Fact {
            ty: Ty::F64,
            range: None,
            exact: left.exact && right.exact,
        };
    }
    let ty = ty.or_else(|| match (&left.ty, &right.ty) {
        (Ty::Int(i), _) | (_, Ty::Int(i)) => Some(*i),
        (Ty::LiteralInt, Ty::LiteralInt) => Integer::parse("i32").ok(),
        _ => None,
    });
    let Some(i) = ty else {
        return Fact::unknown(Ty::LiteralInt);
    };
    let l = left.coerce(&Ty::Int(i));
    let r = right.coerce(&Ty::Int(i));
    let (Some((a, b)), Some((c, d))) = (l.range, r.range) else {
        return Fact::unknown(Ty::Int(i));
    };
    if !l.exact || !r.exact {
        return Fact::unknown(Ty::Int(i));
    }
    let (min, _) = i.bounds();
    if matches!(op, "/" | "%") && (c <= 0 && d >= 0 || a <= min && b >= min && c <= -1 && d >= -1) {
        return Fact::unknown(Ty::Int(i));
    }
    let range = match op {
        "+" => (a + c, b + d),
        "-" => (a - d, b - c),
        "*" => {
            let v = [a * c, a * d, b * c, b * d];
            (*v.iter().min().unwrap(), *v.iter().max().unwrap())
        }
        "/" => {
            let v = [a / c, a / d, b / c, b / d];
            (*v.iter().min().unwrap(), *v.iter().max().unwrap())
        }
        "%" => {
            let limit = a.abs().max(b.abs()).min(c.abs().max(d.abs()) - 1);
            (
                if a < 0 { -limit } else { 0 },
                if b > 0 { limit } else { 0 },
            )
        }
        _ => return Fact::unknown(Ty::Int(i)),
    };
    if i.name == "usize" && matches!(op, "+" | "-" | "*") && (range.0 < 0 || range.1 > 65535) {
        return Fact::unknown(Ty::Int(i));
    }
    Fact {
        ty: Ty::Int(i),
        range: Some(range),
        exact: true,
    }
    .coerce(&Ty::Int(i))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_and_binding_facts_survive_ownership_checking() {
        let program = crate::rust_to_cast("fn cast(n: i64) -> u8 { n as u8 } fn main() { let n: i64 = 300; let byte: u8 = cast(n); println!(\"{}\", byte as i64); }").unwrap();
        let f = &program.functions["cast"];
        assert_eq!(f.meta["rust_numeric_result"]["type"], "u8");
        assert_eq!(f.meta["rust_numeric_result"]["canonical"], true);
        assert_eq!(
            f.meta["rust_numeric_parameters"][0]["storage"],
            f.params[0].0
        );
        assert_eq!(
            f.meta["rust_numeric_parameters"][0]["fact"]["canonical"],
            true
        );
        let S::VarDecl { meta, .. } = &program.functions["main"].body[1] else {
            panic!("declaration");
        };
        assert_eq!(meta["rust_numeric_fact"]["type"], "u8");
        assert_eq!(meta["rust_numeric_fact"]["canonical"], true);
        assert!(meta.contains_key("rust_binding"));
    }

    #[test]
    fn writes_in_a_shadow_scope_do_not_taint_the_outer_binding() {
        let program = crate::rust_to_cast("fn main() { let n: i64 = 300; { let mut n: i64 = 1; n = 2; println!(\"{}\", n); } println!(\"{}\", n as u8); }").unwrap();
        let body = &program.functions["main"].body;
        let S::VarDecl { meta: outer, .. } = &body[0] else {
            panic!("outer");
        };
        let S::If { then_body, .. } = &body[1] else {
            panic!("block");
        };
        let S::VarDecl { meta: inner, .. } = &then_body[0] else {
            panic!("inner");
        };
        assert_eq!(outer["rust_numeric_fact"]["canonical"], true);
        assert_eq!(inner["rust_numeric_fact"]["canonical"], false);
        assert_ne!(outer["rust_binding"]["id"], inner["rust_binding"]["id"]);
    }
}
