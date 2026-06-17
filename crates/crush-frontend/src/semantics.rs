use crush_cast::*;
use crate::types::Type;
use anyhow::{Result, bail};
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
        self.functions.insert("len".to_string(), (vec![Type::Any], Type::Int));
        self.functions.insert("print".to_string(), (vec![Type::Any], Type::Null));

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

        // Return-type inference visits functions in HashMap order, so a caller
        // inferred before its callee sees the callee's placeholder Null type.
        // Seed with an error-tolerant pass (restoring scope depth on bail) so
        // single-level call dependencies resolve regardless of iteration order,
        // then run the authoritative pass where errors surface.
        for (name, func) in &program.functions {
            let depth = self.scopes.len();
            match self.infer_function_return_type(func) {
                Ok(inferred) => {
                    if let Some((_, ret)) = self.functions.get_mut(name) {
                        *ret = inferred;
                    }
                }
                Err(_) => self.scopes.truncate(depth),
            }
        }
        for (name, func) in &program.functions {
            let inferred = self.infer_function_return_type(func)?;
            if let Some((_, ret)) = self.functions.get_mut(name) {
                *ret = inferred;
            }
        }

        // Fixed-point iteration for recursive functions: keep re-inferring until
        // return types stabilize (or max iterations reached).
        for _ in 0..10 {
            let mut changed = false;
            for (name, func) in &program.functions {
                let depth = self.scopes.len();
                match self.infer_function_return_type(func) {
                    Ok(inferred) => {
                        if let Some((_, ret)) = self.functions.get_mut(name) {
                            if *ret != inferred {
                                *ret = inferred;
                                changed = true;
                            }
                        }
                    }
                    Err(_) => self.scopes.truncate(depth),
                }
            }
            if !changed {
                break;
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
                        } else {
                            bail!("Invalid binary op + for types {} and {}", l_type, r_type)
                        }
                    }
                    "-" | "*" | "/" | "%" => {
                        if self.is_numeric(&l_type) && self.is_numeric(&r_type) {
                            Ok(self.numeric_result_type(&l_type, &r_type))
                        } else if l_type == Type::Any || r_type == Type::Any {
                            Ok(Type::Any)
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
                if let Some((arg_types, ret_type)) = self.functions.get(function).cloned() {
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
                _ => {}
            }
        }
        Ok(())
    }
}
