use crush_cast::*;
use std::collections::{HashMap, HashSet};

pub struct Optimizer;

impl Optimizer {
    pub fn optimize(program: &mut Program) {
        for func in program.functions.values_mut() {
            Self::optimize_function(func);
        }
    }

    fn optimize_function(func: &mut Function) {
        let mut consts = HashMap::new();
        Self::optimize_block_with_consts(&mut func.body, &mut consts);
    }

    fn optimize_block_with_consts(
        stmts: &mut Vec<Statement>,
        consts: &mut HashMap<String, Expression>,
    ) {
        let mut out = Vec::new();
        let input = std::mem::take(stmts);

        for stmt in input {
            let optimized = Self::optimize_stmt_owned(stmt, consts);
            for s in optimized {
                let terminal = matches!(s, Statement::Return { .. } | Statement::Throw { .. });
                out.push(s);
                if terminal {
                    *stmts = out;
                    return;
                }
            }
        }

        *stmts = out;
    }

    fn optimize_stmt_owned(
        stmt: Statement,
        consts: &mut HashMap<String, Expression>,
    ) -> Vec<Statement> {
        match stmt {
            Statement::VarDecl {
                name,
                mut value,
                type_hint,
                meta,
            } => {
                Self::replace_vars_expr(&mut value, consts);
                Self::optimize_expr(&mut value);
                if let Some(const_expr) = Self::as_constant_expr(&value) {
                    consts.insert(name.clone(), const_expr);
                } else {
                    consts.remove(&name);
                }

                vec![Statement::VarDecl {
                    name,
                    value,
                    type_hint,
                    meta,
                }]
            }
            Statement::Assign {
                target,
                mut value,
                meta,
            } => {
                Self::replace_vars_expr(&mut value, consts);
                Self::optimize_expr(&mut value);
                consts.remove(&target);
                vec![Statement::Assign {
                    target,
                    value,
                    meta,
                }]
            }
            Statement::Export {
                name,
                mut value,
                meta,
            } => {
                Self::replace_vars_expr(&mut value, consts);
                Self::optimize_expr(&mut value);
                vec![Statement::Export { name, value, meta }]
            }
            Statement::ExprStmt { mut expr, meta } => {
                Self::replace_vars_expr(&mut expr, consts);
                Self::optimize_expr(&mut expr);
                vec![Statement::ExprStmt { expr, meta }]
            }
            Statement::Return { mut value, meta } => {
                if let Some(v) = &mut value {
                    Self::replace_vars_expr(v, consts);
                    Self::optimize_expr(v);
                }
                vec![Statement::Return { value, meta }]
            }
            Statement::Throw { mut value, meta } => {
                Self::replace_vars_expr(&mut value, consts);
                Self::optimize_expr(&mut value);
                vec![Statement::Throw { value, meta }]
            }
            Statement::If {
                mut condition,
                mut then_body,
                mut else_body,
                meta,
            } => {
                Self::replace_vars_expr(&mut condition, consts);
                Self::optimize_expr(&mut condition);

                let parent_consts = consts.clone();
                let mut then_consts = parent_consts.clone();
                Self::optimize_block_with_consts(&mut then_body, &mut then_consts);

                let mut else_consts = parent_consts.clone();
                if let Some(eb) = &mut else_body {
                    Self::optimize_block_with_consts(eb, &mut else_consts);
                }

                *consts = parent_consts;

                if let Expression::BoolLiteral { value, .. } = condition {
                    if value {
                        then_body
                    } else {
                        else_body.unwrap_or_default()
                    }
                } else {
                    vec![Statement::If {
                        condition,
                        then_body,
                        else_body,
                        meta,
                    }]
                }
            }
            Statement::While {
                mut condition,
                mut body,
                meta,
            } => {
                let mut mutated = HashSet::new();
                Self::collect_mutated_vars(&body, &mut mutated);
                for var in mutated {
                    consts.remove(&var);
                }

                Self::replace_vars_expr(&mut condition, consts);
                Self::optimize_expr(&mut condition);
                let mut loop_consts = consts.clone();
                Self::optimize_block_with_consts(&mut body, &mut loop_consts);
                consts.clear();

                vec![Statement::While {
                    condition,
                    body,
                    meta,
                }]
            }
            Statement::For {
                variable,
                mut iterable,
                mut body,
                meta,
            } => {
                Self::replace_vars_expr(&mut iterable, consts);
                Self::optimize_expr(&mut iterable);
                let mut for_consts = consts.clone();
                for_consts.remove(&variable);
                Self::optimize_block_with_consts(&mut body, &mut for_consts);
                consts.clear();

                vec![Statement::For {
                    variable,
                    iterable,
                    body,
                    meta,
                }]
            }
            Statement::SetField {
                mut target,
                field,
                mut value,
                meta,
            } => {
                Self::replace_vars_expr(&mut target, consts);
                Self::replace_vars_expr(&mut value, consts);
                Self::optimize_expr(&mut target);
                Self::optimize_expr(&mut value);
                vec![Statement::SetField {
                    target,
                    field,
                    value,
                    meta,
                }]
            }
            Statement::TryCatch {
                mut body,
                error_var,
                mut handler,
                meta,
            } => {
                let mut body_consts = consts.clone();
                Self::optimize_block_with_consts(&mut body, &mut body_consts);
                let mut handler_consts = consts.clone();
                handler_consts.remove(&error_var);
                Self::optimize_block_with_consts(&mut handler, &mut handler_consts);
                consts.clear();

                vec![Statement::TryCatch {
                    body,
                    error_var,
                    handler,
                    meta,
                }]
            }
            other => vec![other],
        }
    }

    fn as_constant_expr(expr: &Expression) -> Option<Expression> {
        match expr {
            Expression::IntLiteral { .. }
            | Expression::FloatLiteral { .. }
            | Expression::StringLiteral { .. }
            | Expression::BoolLiteral { .. }
            | Expression::NullLiteral { .. } => Some(expr.clone()),
            _ => None,
        }
    }

    fn replace_vars_expr(expr: &mut Expression, consts: &HashMap<String, Expression>) {
        match expr {
            Expression::Var { name, .. } => {
                if let Some(value) = consts.get(name) {
                    *expr = value.clone();
                }
            }
            Expression::BinaryOp { left, right, .. } => {
                Self::replace_vars_expr(left, consts);
                Self::replace_vars_expr(right, consts);
            }
            Expression::UnaryOp { operand, .. } => {
                Self::replace_vars_expr(operand, consts);
            }
            Expression::Call { args, .. } | Expression::CapabilityCall { args, .. } => {
                for arg in args {
                    Self::replace_vars_expr(arg, consts);
                }
            }
            Expression::ArrayLiteral { elements, .. } => {
                for el in elements {
                    Self::replace_vars_expr(el, consts);
                }
            }
            Expression::ObjectLiteral { properties, .. } => {
                for (_, value) in properties {
                    Self::replace_vars_expr(value, consts);
                }
            }
            Expression::GetField { target, .. } => {
                Self::replace_vars_expr(target, consts);
            }
            Expression::Index { target, index, .. } => {
                Self::replace_vars_expr(target, consts);
                Self::replace_vars_expr(index, consts);
            }
            Expression::Await { expression, .. } => {
                Self::replace_vars_expr(expression, consts);
            }
            _ => {}
        }
    }

    fn optimize_expr(expr: &mut Expression) {
        match expr {
            Expression::BinaryOp {
                operator,
                left,
                right,
                meta,
            } => {
                Self::optimize_expr(left);
                Self::optimize_expr(right);

                // Strength reduction and identities for integer literals.
                if operator == "*" {
                    if let Expression::IntLiteral { value: 0, .. } = &**left {
                        *expr = Expression::IntLiteral {
                            value: 0,
                            meta: meta.clone(),
                        };
                        return;
                    }
                    if let Expression::IntLiteral { value: 0, .. } = &**right {
                        *expr = Expression::IntLiteral {
                            value: 0,
                            meta: meta.clone(),
                        };
                        return;
                    }
                    if let Expression::IntLiteral { value: 1, .. } = &**left {
                        *expr = (**right).clone();
                        return;
                    }
                    if let Expression::IntLiteral { value: 1, .. } = &**right {
                        *expr = (**left).clone();
                        return;
                    }
                    if let Expression::IntLiteral { value: 2, .. } = &**left {
                        let rhs = (**right).clone();
                        *expr = Expression::BinaryOp {
                            operator: "+".to_string(),
                            left: Box::new(rhs.clone()),
                            right: Box::new(rhs),
                            meta: meta.clone(),
                        };
                        return;
                    }
                    if let Expression::IntLiteral { value: 2, .. } = &**right {
                        let lhs = (**left).clone();
                        *expr = Expression::BinaryOp {
                            operator: "+".to_string(),
                            left: Box::new(lhs.clone()),
                            right: Box::new(lhs),
                            meta: meta.clone(),
                        };
                        return;
                    }
                }
                if operator == "+" {
                    if let Expression::IntLiteral { value: 0, .. } = &**left {
                        *expr = (**right).clone();
                        return;
                    }
                    if let Expression::IntLiteral { value: 0, .. } = &**right {
                        *expr = (**left).clone();
                        return;
                    }
                }

                // Constant folding for Ints.
                if let (
                    Expression::IntLiteral { value: l_val, .. },
                    Expression::IntLiteral { value: r_val, .. },
                ) = (&**left, &**right)
                {
                    let folded = match operator.as_str() {
                        "+" => Some(l_val + r_val),
                        "-" => Some(l_val - r_val),
                        "*" => Some(l_val * r_val),
                        "/" if *r_val != 0 => Some(l_val / r_val),
                        "%" if *r_val != 0 => Some(l_val % r_val),
                        _ => None,
                    };

                    if let Some(val) = folded {
                        *expr = Expression::IntLiteral {
                            value: val,
                            meta: meta.clone(),
                        };
                        return;
                    }
                }

                // Constant folding for floats.
                if let (
                    Expression::FloatLiteral { value: l_val, .. },
                    Expression::FloatLiteral { value: r_val, .. },
                ) = (&**left, &**right)
                {
                    let folded = match operator.as_str() {
                        "+" => Some(l_val + r_val),
                        "-" => Some(l_val - r_val),
                        "*" => Some(l_val * r_val),
                        "/" if *r_val != 0.0 => Some(l_val / r_val),
                        _ => None,
                    };

                    if let Some(val) = folded {
                        *expr = Expression::FloatLiteral {
                            value: val,
                            meta: meta.clone(),
                        };
                        return;
                    }
                }

                // Bool folding.
                if let (
                    Expression::BoolLiteral { value: l_val, .. },
                    Expression::BoolLiteral { value: r_val, .. },
                ) = (&**left, &**right)
                {
                    let folded = match operator.as_str() {
                        "&&" => Some(*l_val && *r_val),
                        "||" => Some(*l_val || *r_val),
                        _ => None,
                    };
                    if let Some(val) = folded {
                        *expr = Expression::BoolLiteral {
                            value: val,
                            meta: meta.clone(),
                        };
                        return;
                    }
                }

                // String concat folding.
                if let (
                    Expression::StringLiteral { value: l_val, .. },
                    Expression::StringLiteral { value: r_val, .. },
                ) = (&**left, &**right)
                    && operator == "+"
                {
                    *expr = Expression::StringLiteral {
                        value: format!("{}{}", l_val, r_val),
                        meta: meta.clone(),
                    };
                }
            }
            Expression::UnaryOp {
                operator,
                operand,
                meta,
            } => {
                Self::optimize_expr(operand);
                if let Expression::IntLiteral { value, .. } = &**operand
                    && operator == "-"
                {
                    *expr = Expression::IntLiteral {
                        value: -value,
                        meta: meta.clone(),
                    };
                    return;
                }
                if let Expression::BoolLiteral { value, .. } = &**operand
                    && operator == "!"
                {
                    *expr = Expression::BoolLiteral {
                        value: !value,
                        meta: meta.clone(),
                    };
                }
            }
            Expression::Call { args, .. } => {
                for arg in args {
                    Self::optimize_expr(arg);
                }
            }
            Expression::CapabilityCall { args, .. } => {
                for arg in args {
                    Self::optimize_expr(arg);
                }
            }
            Expression::ArrayLiteral { elements, .. } => {
                for el in elements {
                    Self::optimize_expr(el);
                }
            }
            Expression::ObjectLiteral { properties, .. } => {
                for (_, value) in properties {
                    Self::optimize_expr(value);
                }
            }
            Expression::GetField { target, .. } => {
                Self::optimize_expr(target);
            }
            Expression::Index { target, index, .. } => {
                Self::optimize_expr(target);
                Self::optimize_expr(index);
            }
            _ => {}
        }
    }

    fn collect_mutated_vars(stmts: &[Statement], out: &mut HashSet<String>) {
        for stmt in stmts {
            match stmt {
                Statement::Assign { target, .. } => {
                    out.insert(target.clone());
                }
                Statement::If { then_body, else_body, .. } => {
                    Self::collect_mutated_vars(then_body, out);
                    if let Some(eb) = else_body {
                        Self::collect_mutated_vars(eb, out);
                    }
                }
                Statement::While { body, .. } => {
                    Self::collect_mutated_vars(body, out);
                }
                Statement::For { body, .. } => {
                    Self::collect_mutated_vars(body, out);
                }
                Statement::TryCatch { body, handler, .. } => {
                    Self::collect_mutated_vars(body, out);
                    Self::collect_mutated_vars(handler, out);
                }
                _ => {}
            }
        }
    }
}
