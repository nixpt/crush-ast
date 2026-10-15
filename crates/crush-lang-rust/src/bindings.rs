//! Rust lexical locals → distinct CAST storage names (CRUSH-xps-6).
//! Runs after macro/statement lowering, so format-string captures are ordinary
//! Vars too. Binding metadata is independent of the VM's local-slot layout.
use std::collections::HashMap;

use crush_cast::{Expression as E, Pattern, Program, Statement as S};
use serde_json::{Value, json};

type Meta = HashMap<String, Value>;

#[derive(Clone)]
struct Binding {
    storage: String,
    info: Value,
}

struct Scope {
    id: usize,
    names: HashMap<String, Binding>,
}

#[derive(Default)]
struct Resolver {
    prefix: String,
    function_name: String,
    scopes: Vec<Scope>,
    next_scope: usize,
    catalog: Vec<Value>,
}

pub(crate) fn resolve(program: &mut Program, source: &str) -> anyhow::Result<()> {
    // Choose a valid identifier prefix absent from the entire source (including
    // names captured inside format strings). This avoids both source collisions
    // and invalid names when the compiler incorporates a binding in a lambda's
    // function name.
    let mut prefix = "__crush_rust_binding_".to_string();
    let mut nonce = 0;
    while source.contains(&prefix) {
        prefix = format!("__crush_rust_binding_{nonce}_");
        nonce += 1;
    }
    for (name, function) in &mut program.functions {
        let mut resolver = Resolver {
            prefix: prefix.clone(),
            function_name: name.clone(),
            ..Default::default()
        };
        resolver.enter();
        for (name, _) in &mut function.params {
            *name = resolver.bind(name, "parameter").storage;
        }
        resolver.statements(&mut function.body)?;
        function
            .meta
            .insert("rust_bindings".into(), json!(resolver.catalog));
    }
    Ok(())
}

impl Resolver {
    fn enter(&mut self) {
        self.scopes.push(Scope {
            id: self.next_scope,
            names: HashMap::new(),
        });
        self.next_scope += 1;
    }

    fn bind(&mut self, name: &str, kind: &str) -> Binding {
        let scope = self.scopes.last_mut().expect("Rust lexical scope");
        let id = self.catalog.len();
        let binding = Binding {
            storage: format!("{}{id}", self.prefix),
            info: json!({"id": id, "source_name": name, "scope_id": scope.id, "kind": kind, "function": self.function_name}),
        };
        self.catalog.push(binding.info.clone());
        scope.names.insert(name.into(), binding.clone());
        binding
    }

    fn lookup(&self, name: &str) -> Option<Binding> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.names.get(name))
            .cloned()
    }

    fn reference(&self, name: &mut String, meta: &mut Meta) {
        if let Some(binding) = self.lookup(name) {
            *name = binding.storage;
            meta.insert("rust_binding".into(), binding.info);
        }
    }

    fn block(&mut self, body: &mut [S]) -> anyhow::Result<()> {
        self.enter();
        self.statements(body)?;
        self.scopes.pop();
        Ok(())
    }

    fn statements(&mut self, statements: &mut [S]) -> anyhow::Result<()> {
        for statement in statements {
            match statement {
                S::VarDecl {
                    name, value, meta, ..
                } => {
                    // A let's initializer still sees the previous binding.
                    self.expression(value)?;
                    let binding = self.bind(name, "let");
                    *name = binding.storage;
                    meta.insert("rust_binding".into(), binding.info);
                }
                S::Assign {
                    target,
                    value,
                    meta,
                } => {
                    self.expression(value)?;
                    self.reference(target, meta);
                }
                S::ExprStmt { expr, .. } => self.expression(expr)?,
                S::Return { value, .. } => {
                    if let Some(value) = value {
                        self.expression(value)?;
                    }
                }
                S::If {
                    condition,
                    then_body,
                    else_body,
                    ..
                } => {
                    self.expression(condition)?;
                    self.block(then_body)?;
                    if let Some(body) = else_body {
                        self.block(body)?;
                    }
                }
                S::While {
                    condition, body, ..
                } => {
                    self.expression(condition)?;
                    self.block(body)?;
                }
                S::For {
                    variable,
                    iterable,
                    body,
                    meta,
                } => {
                    self.expression(iterable)?;
                    self.enter();
                    let binding = self.bind(variable, "for");
                    *variable = binding.storage;
                    meta.insert("rust_binding".into(), binding.info);
                    self.statements(body)?;
                    self.scopes.pop();
                }
                S::Break { .. } | S::Continue { .. } => {}
                S::FunctionDef { meta, .. } => anyhow::bail!(
                    "unsupported Rust nested function at {}:{}: block-local item resolution is not implemented (CRUSH-xps-6)",
                    meta.get("line").unwrap_or(&Value::Null),
                    meta.get("column").unwrap_or(&Value::Null)
                ),
                other => anyhow::bail!(
                    "unsupported Rust binding resolution for CAST statement: {other:?}"
                ),
            }
        }
        Ok(())
    }

    fn expression(&mut self, expression: &mut E) -> anyhow::Result<()> {
        match expression {
            E::Var { name, meta } => self.reference(name, meta),
            E::BinaryOp { left, right, .. }
            | E::Range {
                start: left,
                end: right,
                ..
            }
            | E::Index {
                target: left,
                index: right,
                ..
            } => {
                self.expression(left)?;
                self.expression(right)?;
            }
            E::UnaryOp { operand, .. }
            | E::GetField {
                target: operand, ..
            } => self.expression(operand)?,
            E::Call {
                function,
                args,
                meta,
            } => {
                // Source callable variables resolve lexically. Compiler-owned
                // intrinsics and mapped primitive methods keep their names.
                if meta.get("rust_source_call") == Some(&Value::Bool(true)) {
                    self.reference(function, meta);
                }
                for arg in args {
                    self.expression(arg)?;
                }
            }
            E::CapabilityCall { args, meta, .. } => {
                // Legacy function-form print/println were mapped before local
                // resolution. A source callable with that name wins; macro
                // calls have no source-call marker and keep their capability.
                let local_call = meta
                    .get("rust_source_name")
                    .and_then(Value::as_str)
                    .filter(|name| self.lookup(name).is_some())
                    .map(str::to_owned);
                if let Some(function) = local_call {
                    *expression = E::Call {
                        function,
                        args: std::mem::take(args),
                        meta: std::mem::take(meta),
                    };
                    return self.expression(expression);
                }
                // Rust lowering has already resolved these global capability
                // names; every receiver is an explicit argument.
                for arg in args {
                    self.expression(arg)?;
                }
            }
            E::ArrayLiteral { elements, .. } => {
                for element in elements {
                    self.expression(element)?;
                }
            }
            E::Lambda { params, body, meta } => {
                self.enter();
                meta.insert(
                    "rust_scope_id".into(),
                    json!(self.scopes.last().unwrap().id),
                );
                let mut bindings = Vec::new();
                for (name, _) in params {
                    let binding = self.bind(name, "closure parameter");
                    *name = binding.storage;
                    bindings.push(binding.info);
                }
                meta.insert("rust_params".into(), json!(bindings));
                self.statements(body)?;
                self.scopes.pop();
            }
            E::Match {
                expression, arms, ..
            } => {
                self.expression(expression)?;
                for arm in arms {
                    self.enter();
                    match &mut arm.pattern {
                        Pattern::Identifier { name } => *name = self.bind(name, "match").storage,
                        Pattern::Literal { value } => self.expression(value)?,
                        Pattern::Wildcard => {}
                        other => anyhow::bail!("unsupported Rust binding pattern: {other:?}"),
                    }
                    self.statements(&mut arm.body)?;
                    self.scopes.pop();
                }
            }
            E::IntLiteral { .. }
            | E::FloatLiteral { .. }
            | E::BoolLiteral { .. }
            | E::StringLiteral { .. }
            | E::NullLiteral { .. } => {}
            other => {
                anyhow::bail!("unsupported Rust binding resolution for CAST expression: {other:?}")
            }
        }
        Ok(())
    }
}
