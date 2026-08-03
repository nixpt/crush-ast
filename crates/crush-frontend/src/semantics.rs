use crate::types::Type;
use anyhow::{Result, bail};
use crush_cast::*;
use std::collections::HashMap;

pub struct SemanticAnalyzer {
    structs: HashMap<String, HashMap<String, Type>>,
    functions: HashMap<String, (Vec<Type>, Type)>,
    scopes: Vec<HashMap<String, Type>>,
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        let mut global = HashMap::new();
        // CLI args injected by VmRunner — accessible via `load args`
        global.insert("args".to_string(), Type::Array(Box::new(Type::String)));
        Self {
            structs: HashMap::new(),
            functions: HashMap::new(),
            scopes: vec![global],
        }
    }

    pub fn check(&mut self, program: &Program) -> Result<()> {
        // Register built-in functions
        self.functions
            .insert("len".to_string(), (vec![Type::Any], Type::Int));
        self.functions
            .insert("print".to_string(), (vec![Type::Any], Type::Null));

        // Pass 1: Collect definitions
        self.collect_definitions(program)?;

        // Pass 2: Check function bodies
        for func in program.functions.values() {
            self.check_function(func)?;
        }

        Ok(())
    }

    /// Infer the type of an expression within the context of an existing program.
    pub fn infer_expression_type(&mut self, program: &Program, expr: &Expression) -> Result<Type> {
        self.structs.clear();
        self.functions.clear();
        self.scopes.clear();
        self.scopes.push(HashMap::new());

        self.collect_definitions(program)?;

        if let Some(main) = program.functions.get("main") {
            for stmt in &main.body {
                if let Statement::VarDecl { name, value, .. } = stmt {
                    let ty = self.check_expr(value)?;
                    self.define_var(name, ty);
                }
            }
        }

        self.check_expr(expr)
    }

    fn collect_definitions(&mut self, program: &Program) -> Result<()> {
        for func in program.functions.values() {
            for stmt in &func.body {
                if let Statement::StructDef { name, fields, .. } = stmt {
                    let mut field_map = HashMap::new();
                    for (f_name, f_cast_type) in fields {
                        field_map.insert(f_name.clone(), self.parse_cast_type(f_cast_type)?);
                    }
                    self.structs.insert(name.clone(), field_map);
                }
                // Pre-collect top-level function signatures if we had them outside program.functions
                // For now, program.functions is the source.
            }
        }

        for (name, func) in &program.functions {
            let mut arg_types = Vec::new();
            for (_name, cast_type) in &func.params {
                arg_types.push(self.parse_cast_type(cast_type)?);
            }
            // Start with placeholder return types, infer in a second pass.
            self.functions.insert(name.clone(), (arg_types, Type::Null));
        }

        // Return-type inference order matters: a caller inferred before its
        // callee sees the callee's placeholder Null type. Build the call
        // graph, condense it into strongly-connected components (Tarjan),
        // and infer in reverse topological order so every callee's return
        // type is final before its callers run. A non-recursive function
        // needs exactly one inference walk; only genuinely recursive SCCs
        // iterate to a fixed point, scoped to their own members. (The
        // previous whole-program fixed point was capped at 10 iterations,
        // so a call chain deeper than ~12 functions could nondeterministically
        // fail to converge depending on HashMap order.)
        let mut names: Vec<&String> = program.functions.keys().collect();
        names.sort();
        let index_of: HashMap<&str, usize> = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.as_str(), i))
            .collect();
        let mut edges: Vec<Vec<usize>> = vec![Vec::new(); names.len()];
        for (i, name) in names.iter().enumerate() {
            let mut callees = Vec::new();
            collect_called_functions(&program.functions[*name].body, &mut callees);
            for callee in callees {
                if let Some(&j) = index_of.get(callee)
                    && !edges[i].contains(&j)
                {
                    edges[i].push(j);
                }
            }
        }

        for scc in tarjan_sccs(&edges) {
            if scc.len() == 1 && !edges[scc[0]].contains(&scc[0]) {
                // Non-recursive: every callee is already final, so a single
                // walk is authoritative — errors surface here.
                let name = names[scc[0]].as_str();
                let inferred = self.infer_function_return_type(&program.functions[name])?;
                if let Some((_, ret)) = self.functions.get_mut(name) {
                    *ret = inferred;
                }
            } else {
                // Recursive SCC: error-tolerant seed (restoring scope depth
                // on bail), authoritative pass where errors surface, then a
                // fixed point over just these members.
                //
                // Pre-seed members to Any, not the Null placeholder: merge_types
                // absorbs Any into the concrete side (merge(Bool, Any) = Bool),
                // so base-case branches anchor the fixed point. With Null the
                // seed yields Optional(base) and the fixed point can only widen,
                // never narrow — mutual recursion then types as optional<T>
                // forever.
                for &i in &scc {
                    if let Some((_, ret)) = self.functions.get_mut(names[i].as_str()) {
                        *ret = Type::Any;
                    }
                }
                for &i in &scc {
                    let depth = self.scopes.len();
                    match self.infer_function_return_type(&program.functions[names[i].as_str()]) {
                        Ok(inferred) => {
                            if let Some((_, ret)) = self.functions.get_mut(names[i].as_str()) {
                                *ret = inferred;
                            }
                        }
                        Err(_) => self.scopes.truncate(depth),
                    }
                }
                for &i in &scc {
                    let inferred =
                        self.infer_function_return_type(&program.functions[names[i].as_str()])?;
                    if let Some((_, ret)) = self.functions.get_mut(names[i].as_str()) {
                        *ret = inferred;
                    }
                }
                for _ in 0..10 {
                    let mut changed = false;
                    for &i in &scc {
                        let depth = self.scopes.len();
                        match self
                            .infer_function_return_type(&program.functions[names[i].as_str()])
                        {
                            Ok(inferred) => {
                                if let Some((_, ret)) = self.functions.get_mut(names[i].as_str())
                                    && *ret != inferred
                                {
                                    *ret = inferred;
                                    changed = true;
                                }
                            }
                            Err(_) => self.scopes.truncate(depth),
                        }
                    }
                    if !changed {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    fn parse_cast_type(&self, cast_type: &CastType) -> Result<Type> {
        match cast_type {
            CastType::Int => Ok(Type::Int),
            CastType::Float => Ok(Type::Float),
            CastType::Bool => Ok(Type::Bool),
            CastType::String => Ok(Type::String),
            CastType::Null => Ok(Type::Null),
            CastType::Array(inner) => Ok(Type::Array(Box::new(self.parse_cast_type(inner)?))),
            CastType::Tuple(types) => {
                let parsed: Result<Vec<_>> = types.iter().map(|t| self.parse_cast_type(t)).collect();
                Ok(Type::Tuple(parsed?))
            }
            CastType::List(inner) => Ok(Type::List(Box::new(self.parse_cast_type(inner)?))),
            CastType::Vector(inner) => Ok(Type::Vector(Box::new(self.parse_cast_type(inner)?))),
            CastType::Set(inner) => Ok(Type::Set(Box::new(self.parse_cast_type(inner)?))),
            CastType::Map(value) => Ok(Type::Map(
                Box::new(Type::String),
                Box::new(self.parse_cast_type(value)?),
            )),
            CastType::Lambda { params, returns } => {
                let param_types = params
                    .iter()
                    .map(|p| self.parse_cast_type(p))
                    .collect::<Result<Vec<_>>>()?;
                let ret = self.parse_cast_type(returns)?;
                Ok(Type::Function(param_types, Box::new(ret)))
            }
            CastType::F32 => Ok(Type::Float),
            CastType::BigInt => Ok(Type::Int),
            CastType::Complex => Ok(Type::Float),
            CastType::Tensor(inner) => Ok(Type::Array(Box::new(self.parse_cast_type(inner)?))),
            CastType::Any => Ok(Type::Any),
            CastType::TypeRef(name) | CastType::Struct(name) => {
                if self.structs.contains_key(name) {
                    Ok(Type::Struct(name.to_string()))
                } else {
                    bail!("Unknown type: {}", name)
                }
            }
        }
    }

    fn check_function(&mut self, func: &Function) -> Result<()> {
        self.enter_scope();
        // Add params to scope
        for (param_name, cast_type) in &func.params {
            let ty = self.parse_cast_type(cast_type)?;
            self.define_var(param_name, ty);
        }

        for stmt in &func.body {
            self.check_stmt(stmt)?;
        }

        self.exit_scope();
        Ok(())
    }

    fn check_stmt(&mut self, stmt: &Statement) -> Result<()> {
        match stmt {
            Statement::VarDecl {
                name,
                value,
                type_hint,
                ..
            } => {
                let expr_type = self.check_expr(value)?;
                if *type_hint != CastType::Any {
                    let hinted_type = self.parse_cast_type(type_hint)?;
                    if !self.is_assignable(&hinted_type, &expr_type) {
                        bail!(
                            "Type mismatch for variable '{}': expected {}, found {}",
                            name,
                            hinted_type,
                            expr_type
                        );
                    }
                }
                self.define_var(name, expr_type);
            }
            Statement::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                let cond_type = self.check_expr(condition)?;
                if cond_type != Type::Bool {
                    bail!("If condition must be bool, found {}", cond_type);
                }
                self.check_block(then_body)?;
                if let Some(eb) = else_body {
                    self.check_block(eb)?;
                }
            }
            Statement::While {
                condition, body, ..
            } => {
                let cond_type = self.check_expr(condition)?;
                if cond_type != Type::Bool {
                    bail!("While condition must be bool, found {}", cond_type);
                }
                self.check_block(body)?;
            }
            Statement::ExprStmt { expr, .. } => {
                self.check_expr(expr)?;
            }
            Statement::Return { value, .. } => {
                if let Some(expr) = value {
                    self.check_expr(expr)?;
                }
            }
            Statement::StructDef { .. } => {} // Already handled in Pass 1
            Statement::LangBlock { meta, .. } => {
                // A polyglot block's own body is opaque to Crush's checker
                // (it's a different language's source text), but if a
                // free-variable analysis pass (crush-lang-sdk::compile, for
                // @python blocks) determined an output variable and
                // recorded it in meta["polyglot_output"], that name is
                // real and must be declared here — otherwise every read of
                // it later in the same scope reports as undefined, even
                // though the compiler emits a `store` into exactly that
                // name right after `exec_lang`.
                if let Some(output_var) = meta.get("polyglot_output").and_then(|v| v.as_str()) {
                    self.define_var(output_var, Type::Any);
                }
            }
            _ => {
                // TODO: Implement remaining statements
            }
        }
        Ok(())
    }

    fn check_block(&mut self, stmts: &[Statement]) -> Result<()> {
        self.enter_scope();
        for stmt in stmts {
            self.check_stmt(stmt)?;
        }
        self.exit_scope();
        Ok(())
    }

    fn check_expr(&mut self, expr: &Expression) -> Result<Type> {
        match expr {
            Expression::IntLiteral { .. } => Ok(Type::Int),
            Expression::FloatLiteral { .. } => Ok(Type::Float),
            Expression::StringLiteral { .. } => Ok(Type::String),
            Expression::BoolLiteral { .. } => Ok(Type::Bool),
            Expression::NullLiteral { .. } => Ok(Type::Null),
            Expression::Var { name, .. } => self
                .resolve_var(name)
                .ok_or_else(|| anyhow::anyhow!("Undefined variable: {}", name)),
            Expression::BinaryOp {
                operator,
                left,
                right,
                ..
            } => {
                let l_type = self.check_expr(left)?;
                let r_type = self.check_expr(right)?;
                match operator.as_str() {
                    "+" => {
                        if l_type == Type::String && r_type == Type::String {
                            Ok(Type::String)
                        } else if self.is_numeric(&l_type) && self.is_numeric(&r_type) {
                            Ok(self.numeric_result_type(&l_type, &r_type))
                        } else if l_type == Type::String || r_type == Type::String {
                            Ok(Type::String)
                        } else if l_type == Type::Any || r_type == Type::Any {
                            Ok(Type::Any)
                        } else if l_type == Type::Null || r_type == Type::Null {
                            // During return-type inference, a `Null` type means "not yet
                            // inferred" (placeholder for recursive/forward calls). Allow the
                            // fixed-point iteration to converge by returning the non-null type
                            // when one side is known and the other is Null, or Any when both
                            // are Null.
                            Ok(if l_type == Type::Null && r_type == Type::Null {
                                Type::Any
                            } else if l_type == Type::Null {
                                r_type
                            } else {
                                l_type
                            })
                        } else {
                            bail!("Invalid binary op + for types {} and {}", l_type, r_type)
                        }
                    }
                    "-" | "*" | "/" | "%" => {
                        if self.is_numeric(&l_type) && self.is_numeric(&r_type) {
                            Ok(self.numeric_result_type(&l_type, &r_type))
                        } else if l_type == Type::Any || r_type == Type::Any {
                            Ok(Type::Any)
                        } else if l_type == Type::Null || r_type == Type::Null {
                            // Lenient Null handling during return-type inference.
                            Ok(if l_type == Type::Null && r_type == Type::Null {
                                Type::Any
                            } else if l_type == Type::Null {
                                r_type
                            } else {
                                l_type
                            })
                        } else {
                            bail!(
                                "Invalid binary op {} for types {} and {}",
                                operator,
                                l_type,
                                r_type
                            )
                        }
                    }
                    "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                        if l_type == r_type
                            || (self.is_numeric(&l_type) && self.is_numeric(&r_type))
                            || l_type == Type::Any
                            || r_type == Type::Any
                        {
                            Ok(Type::Bool)
                        } else {
                            bail!("Cannot compare types {} and {}", l_type, r_type)
                        }
                    }
                    "&&" | "||" => {
                        if l_type == Type::Bool && r_type == Type::Bool {
                            Ok(Type::Bool)
                        } else if l_type == Type::Any || r_type == Type::Any {
                            Ok(Type::Bool)
                        } else {
                            bail!(
                                "Logical operator {} requires bool operands, found {} and {}",
                                operator,
                                l_type,
                                r_type
                            )
                        }
                    }
                    _ => bail!("Unknown operator: {}", operator),
                }
            }
            Expression::Call { function, args, .. } => {
                let func_type = if let Some((arg_types, ret_type)) = self.functions.get(function).cloned() {
                    Some((arg_types, ret_type))
                } else if let Some(Type::Function(arg_types, ret_type)) = self.resolve_var(function) {
                    Some((arg_types, *ret_type))
                } else {
                    None
                };

                if let Some((arg_types, ret_type)) = func_type {
                    if args.len() != arg_types.len() {
                        bail!(
                            "Function '{}' expects {} arguments, found {}",
                            function,
                            arg_types.len(),
                            args.len()
                        );
                    }
                    for (i, arg) in args.iter().enumerate() {
                        let actual_type = self.check_expr(arg)?;
                        if !self.is_assignable(&arg_types[i], &actual_type) {
                            bail!(
                                "Argument {} to '{}' has wrong type: expected {}, found {}",
                                i,
                                function,
                                arg_types[i],
                                actual_type
                            );
                        }
                    }
                    Ok(ret_type)
                } else if let Some(Type::Any) = self.resolve_var(function) {
                    for arg in args {
                        self.check_expr(arg)?;
                    }
                    Ok(Type::Any)
                } else {
                    bail!("Undefined function: {}", function)
                }
            }
            Expression::ArrayLiteral { elements, .. } => {
                if elements.is_empty() {
                    return Ok(Type::Array(Box::new(Type::Any)));
                }
                let mut current = self.check_expr(&elements[0])?;
                for elem in elements.iter().skip(1) {
                    let elem_ty = self.check_expr(elem)?;
                    current = self.merge_types(&current, &elem_ty).ok_or_else(|| {
                        anyhow::anyhow!(
                            "Array elements must have compatible types, found {} and {}",
                            current,
                            elem_ty
                        )
                    })?;
                }
                Ok(Type::Array(Box::new(current)))
            }
            Expression::ObjectLiteral { properties, .. } => {
                // Object literals are used for dynamic JSON construction; allow mixed types.
                for (_, value) in properties {
                    self.check_expr(value)?;
                }
                Ok(Type::Map(Box::new(Type::String), Box::new(Type::Any)))
            }
            Expression::NewStruct { name, .. } => {
                if self.structs.contains_key(name) {
                    Ok(Type::Struct(name.clone()))
                } else {
                    bail!("Unknown struct: {}", name)
                }
            }
            Expression::GetField { target, field, .. } => {
                let target_type = self.check_expr(target)?;
                if let Type::Struct(struct_name) = target_type {
                    if let Some(f_map) = self.structs.get(&struct_name) {
                        if let Some(f_type) = f_map.get(field) {
                            Ok(f_type.clone())
                        } else {
                            bail!("Struct '{}' has no field '{}'", struct_name, field)
                        }
                    } else {
                        bail!(
                            "Inconsistent state: Struct '{}' not found in definitions",
                            struct_name
                        )
                    }
                } else if matches!(target_type, Type::Map(_, _)) {
                    // Field access on maps returns the value type (Any for now)
                    Ok(Type::Any)
                } else {
                    bail!(
                        "Cannot access field '{}' on non-struct type {}",
                        field,
                        target_type
                    )
                }
            }
            Expression::CapabilityCall { name, args, .. } => {
                // Type-check argument expressions
                for arg in args {
                    self.check_expr(arg)?;
                }
                Ok(self.capability_return_type(name))
            }
            _ => Ok(Type::Any), // Default for complex expressions (capabilities, index, etc.)
        }
    }

    /// Return the known return type for a built-in capability name
    fn capability_return_type(&self, name: &str) -> Type {
        match name {
            "io.print" | "array.push" => Type::Null,
            "arr_slice" => Type::Array(Box::new(Type::Any)),
            "str.contains" => Type::Bool,
            "str.split" => Type::Array(Box::new(Type::String)),
            "str.replace" | "str.join" => Type::String,
            "str.len" | "len" => Type::Int,
            _ => Type::Any,
        }
    }

    fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    fn define_var(&mut self, name: &str, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
        }
    }

    fn resolve_var(&self, name: &str) -> Option<Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        None
    }

    fn is_numeric(&self, ty: &Type) -> bool {
        matches!(ty, Type::Int | Type::Float)
    }

    fn numeric_result_type(&self, left: &Type, right: &Type) -> Type {
        if left == &Type::Float || right == &Type::Float {
            Type::Float
        } else {
            Type::Int
        }
    }

    fn is_assignable(&self, expected: &Type, actual: &Type) -> bool {
        if expected == &Type::Any || actual == &Type::Any {
            return true;
        }
        if expected == actual {
            return true;
        }
        match (expected, actual) {
            (Type::Float, Type::Int) => true,
            (Type::Optional(_), Type::Null) => true,
            (Type::Optional(inner), other) => self.is_assignable(inner, other),
            _ => false,
        }
    }

    fn merge_types(&self, a: &Type, b: &Type) -> Option<Type> {
        if a == b {
            return Some(a.clone());
        }
        match (a, b) {
            (Type::Int, Type::Float) | (Type::Float, Type::Int) => Some(Type::Float),
            (Type::Optional(inner), Type::Null) | (Type::Null, Type::Optional(inner)) => {
                Some(Type::Optional(inner.clone()))
            }
            (Type::Optional(inner), other) | (other, Type::Optional(inner)) => self
                .merge_types(inner, other)
                .map(|merged| Type::Optional(Box::new(merged))),
            (Type::Null, other) | (other, Type::Null) => {
                Some(Type::Optional(Box::new(other.clone())))
            }
            // Any is compatible with everything — merge to the concrete type
            // (or keep Any if both are Any, already handled by the top eq check).
            (Type::Any, concrete) | (concrete, Type::Any) => Some(concrete.clone()),
            _ => None,
        }
    }

    fn infer_function_return_type(&mut self, func: &Function) -> Result<Type> {
        self.enter_scope();
        for (param_name, cast_type) in &func.params {
            let ty = self.parse_cast_type(cast_type)?;
            self.define_var(param_name, ty);
        }

        let mut return_types = Vec::new();
        self.collect_return_types_in_order(&func.body, &mut return_types)?;
        self.exit_scope();

        if return_types.is_empty() {
            return Ok(Type::Null);
        }

        let mut current = return_types[0].clone();
        for ty in return_types.iter().skip(1) {
            current = self.merge_types(&current, ty).ok_or_else(|| {
                anyhow::anyhow!("Conflicting return types: {} and {}", current, ty)
            })?;
        }
        Ok(current)
    }

    fn collect_return_types_in_order(
        &mut self,
        stmts: &[Statement],
        out: &mut Vec<Type>,
    ) -> Result<()> {
        for stmt in stmts {
            match stmt {
                Statement::VarDecl { name, value, .. } => {
                    let ty = self.check_expr(value)?;
                    self.define_var(name, ty);
                }
                Statement::Assign { target, value, .. } => {
                    let ty = self.check_expr(value)?;
                    // In a stricter lang we'd check if target is defined and matches ty.
                }
                Statement::ExprStmt { expr, .. } => {
                    self.check_expr(expr)?;
                }
                Statement::Return { value, .. } => {
                    let ty = match value {
                        Some(expr) => self.check_expr(expr)?,
                        None => Type::Null,
                    };
                    out.push(ty);
                }
                Statement::If {
                    condition,
                    then_body,
                    else_body,
                    ..
                } => {
                    let cond_type = self.check_expr(condition)?;
                    if cond_type != Type::Bool {
                        bail!("If condition must be bool, found {}", cond_type);
                    }
                    self.enter_scope();
                    self.collect_return_types_in_order(then_body, out)?;
                    self.exit_scope();
                    if let Some(else_body) = else_body {
                        self.enter_scope();
                        self.collect_return_types_in_order(else_body, out)?;
                        self.exit_scope();
                    }
                }
                Statement::While {
                    condition, body, ..
                } => {
                    let cond_type = self.check_expr(condition)?;
                    if cond_type != Type::Bool {
                        bail!("While condition must be bool, found {}", cond_type);
                    }
                    self.enter_scope();
                    self.collect_return_types_in_order(body, out)?;
                    self.exit_scope();
                }
                Statement::For {
                    variable,
                    iterable,
                    body,
                    ..
                } => {
                    self.check_expr(iterable)?;
                    self.enter_scope();
                    self.define_var(variable, Type::Any);
                    self.collect_return_types_in_order(body, out)?;
                    self.exit_scope();
                }
                Statement::TryCatch {
                    body,
                    error_var,
                    handler,
                    ..
                } => {
                    self.enter_scope();
                    self.collect_return_types_in_order(body, out)?;
                    self.exit_scope();
                    self.enter_scope();
                    self.define_var(error_var, Type::Any);
                    self.collect_return_types_in_order(handler, out)?;
                    self.exit_scope();
                }
                Statement::LangBlock { meta, .. } => {
                    // See the matching arm in `check_stmt` for why this is
                    // needed: a polyglot block's marshaled output variable
                    // (meta["polyglot_output"]) is real and must be
                    // declared, or any read of it later in this same
                    // return-type-inference walk reports as undefined.
                    if let Some(output_var) = meta.get("polyglot_output").and_then(|v| v.as_str())
                    {
                        self.define_var(output_var, Type::Any);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Collect the names of every function called (by name) anywhere in `stmts`,
/// for call-graph construction. This is a superset of the positions the
/// inference walk actually visits — extra edges only add harmless ordering
/// constraints, whereas a missed edge would let a caller be inferred before
/// its callee. `AI` nodes are skipped because inference skips them too.
fn collect_called_functions<'a>(stmts: &'a [Statement], out: &mut Vec<&'a str>) {
    for stmt in stmts {
        match stmt {
            Statement::VarDecl { value, .. }
            | Statement::Assign { value, .. }
            | Statement::Export { value, .. }
            | Statement::Throw { value, .. } => collect_called_in_expr(value, out),
            Statement::ExprStmt { expr, .. } => collect_called_in_expr(expr, out),
            Statement::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                collect_called_in_expr(condition, out);
                collect_called_functions(then_body, out);
                if let Some(eb) = else_body {
                    collect_called_functions(eb, out);
                }
            }
            Statement::While {
                condition, body, ..
            } => {
                collect_called_in_expr(condition, out);
                collect_called_functions(body, out);
            }
            Statement::For { iterable, body, .. } => {
                collect_called_in_expr(iterable, out);
                collect_called_functions(body, out);
            }
            Statement::Return { value, .. } => {
                if let Some(expr) = value {
                    collect_called_in_expr(expr, out);
                }
            }
            Statement::TryCatch { body, handler, .. } => {
                collect_called_functions(body, out);
                collect_called_functions(handler, out);
            }
            Statement::FunctionDef { body, .. } => collect_called_functions(body, out),
            Statement::SetField { target, value, .. } => {
                collect_called_in_expr(target, out);
                collect_called_in_expr(value, out);
            }
            Statement::DomMutate {
                target,
                value,
                value2,
                ..
            } => {
                collect_called_in_expr(target, out);
                if let Some(v) = value {
                    collect_called_in_expr(v, out);
                }
                if let Some(v) = value2 {
                    collect_called_in_expr(v, out);
                }
            }
            Statement::DomEventListener {
                target, callback, ..
            } => {
                collect_called_in_expr(target, out);
                collect_called_in_expr(callback, out);
            }
            Statement::LangBlock { .. }
            | Statement::Import { .. }
            | Statement::StructDef { .. }
            | Statement::Break { .. }
            | Statement::Continue { .. }
            | Statement::AI(_) => {}
        }
    }
}

fn collect_called_in_expr<'a>(expr: &'a Expression, out: &mut Vec<&'a str>) {
    match expr {
        Expression::Call { function, args, .. } | Expression::Spawn { function, args, .. } => {
            out.push(function.as_str());
            for arg in args {
                collect_called_in_expr(arg, out);
            }
        }
        Expression::BinaryOp { left, right, .. } => {
            collect_called_in_expr(left, out);
            collect_called_in_expr(right, out);
        }
        Expression::UnaryOp { operand, .. } => collect_called_in_expr(operand, out),
        Expression::CapabilityCall { args, .. } | Expression::VectorMath { args, .. } => {
            for arg in args {
                collect_called_in_expr(arg, out);
            }
        }
        Expression::Pipeline { segments, .. }
        | Expression::ArrayLiteral {
            elements: segments, ..
        }
        | Expression::TupleLiteral {
            elements: segments, ..
        }
        | Expression::ListLiteral {
            elements: segments, ..
        }
        | Expression::VectorLiteral {
            elements: segments, ..
        }
        | Expression::SetLiteral {
            elements: segments, ..
        } => {
            for seg in segments {
                collect_called_in_expr(seg, out);
            }
        }
        Expression::Lambda { body, .. } => collect_called_functions(body, out),
        Expression::GetField { target, .. } => collect_called_in_expr(target, out),
        Expression::Range { start, end, .. } => {
            collect_called_in_expr(start, out);
            collect_called_in_expr(end, out);
        }
        Expression::Await { expression, .. } => collect_called_in_expr(expression, out),
        Expression::ObjectLiteral { properties, .. } => {
            for (_, value) in properties {
                collect_called_in_expr(value, out);
            }
        }
        Expression::Index { target, index, .. } => {
            collect_called_in_expr(target, out);
            collect_called_in_expr(index, out);
        }
        Expression::DomQuery { selector, .. } => collect_called_in_expr(selector, out),
        Expression::Match {
            expression, arms, ..
        } => {
            collect_called_in_expr(expression, out);
            for arm in arms {
                collect_called_functions(&arm.body, out);
            }
        }
        Expression::IntLiteral { .. }
        | Expression::FloatLiteral { .. }
        | Expression::StringLiteral { .. }
        | Expression::BoolLiteral { .. }
        | Expression::NullLiteral { .. }
        | Expression::Var { .. }
        | Expression::Yield { .. }
        | Expression::NewStruct { .. }
        | Expression::AI(_) => {}
    }
}

/// Iterative Tarjan SCC. Returns components in reverse topological order of
/// the condensation: with edges pointing caller → callee, every callee's
/// component is emitted before any of its callers'.
fn tarjan_sccs(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    const UNVISITED: usize = usize::MAX;
    let n = edges.len();
    let mut index = vec![UNVISITED; n];
    let mut lowlink = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut next_index = 0usize;
    let mut sccs: Vec<Vec<usize>> = Vec::new();
    let mut dfs: Vec<(usize, usize)> = Vec::new(); // (node, next edge offset)

    for start in 0..n {
        if index[start] != UNVISITED {
            continue;
        }
        index[start] = next_index;
        lowlink[start] = next_index;
        next_index += 1;
        stack.push(start);
        on_stack[start] = true;
        dfs.push((start, 0));

        while let Some(&mut (v, ref mut ei)) = dfs.last_mut() {
            if *ei < edges[v].len() {
                let w = edges[v][*ei];
                *ei += 1;
                if index[w] == UNVISITED {
                    index[w] = next_index;
                    lowlink[w] = next_index;
                    next_index += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    dfs.push((w, 0));
                } else if on_stack[w] {
                    lowlink[v] = lowlink[v].min(index[w]);
                }
            } else {
                dfs.pop();
                if let Some(&mut (parent, _)) = dfs.last_mut() {
                    lowlink[parent] = lowlink[parent].min(lowlink[v]);
                }
                if lowlink[v] == index[v] {
                    let mut scc = Vec::new();
                    loop {
                        let w = stack.pop().expect("Tarjan stack cannot underflow");
                        on_stack[w] = false;
                        scc.push(w);
                        if w == v {
                            break;
                        }
                    }
                    sccs.push(scc);
                }
            }
        }
    }
    sccs
}
