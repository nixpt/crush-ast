use std::collections::HashMap;

use brush_parser::ast::{self, AndOr, AndOrList, Command, CompoundCommand, CompoundList, Pipeline};
use crush_cast::{CastType, Expression, Function, Program, Statement};

/// Lower a brush-parser Program to a CAST Program.
pub fn lower_program(program: ast::Program) -> anyhow::Result<Program> {
    let mut functions: HashMap<String, Function> = HashMap::new();
    let mut main_body: Vec<Statement> = Vec::new();

    for complete in &program.complete_commands {
        for item in &complete.0 {
            let and_or = &item.0;
            // First pass: extract function definitions
            if let Some(func_stmt) = extract_function_def(and_or) {
                if let Statement::FunctionDef { name, params, body, .. } = func_stmt {
                    functions.insert(name, Function { params, body, meta: HashMap::new() });
                    continue;
                }
            }
            for stmt in lower_and_or_list(and_or)? {
                main_body.push(stmt);
            }
        }
    }

    if !main_body.is_empty() {
        functions.insert("main".to_string(), Function {
            params: vec![],
            body: main_body,
            meta: HashMap::new(),
        });
    }

    Ok(Program {
        cast_version: "0.2".to_string(),
        entry: "main".to_string(),
        lang: Some("bash".to_string()),
        functions,
        ai_meta: None,
    })
}

fn extract_function_def(and_or: &AndOrList) -> Option<Statement> {
    let cmd = and_or.first.seq.first()?;
    match cmd {
        Command::Function(func) => {
            let body_stmts = lower_compound_for_body(&func.body.0);
            Some(Statement::FunctionDef {
                name: func.fname.value.clone(),
                params: vec![],
                body: body_stmts,
                meta: HashMap::new(),
            })
        }
        _ => None,
    }
}

fn lower_and_or_list(and_or: &AndOrList) -> anyhow::Result<Vec<Statement>> {
    let mut stmts = Vec::new();
    let first_stmts = lower_pipeline(&and_or.first)?;
    let mut prev = if first_stmts.len() == 1 {
        let s = first_stmts.into_iter().next().unwrap();
        let expr = expr_from_statement(&s);
        stmts.push(s);
        Some(expr)
    } else {
        for s in first_stmts {
            stmts.push(s);
        }
        None
    };

    for additional in &and_or.additional {
        match additional {
            AndOr::And(pipeline) => {
                let cond = prev.take().unwrap_or(Expression::BoolLiteral { value: true, meta: HashMap::new() });
                let body = lower_pipeline(pipeline)?;
                stmts.push(Statement::If {
                    condition: cond,
                    then_body: body,
                    else_body: None,
                    meta: HashMap::new(),
                });
            }
            AndOr::Or(pipeline) => {
                let cond = prev.take().map(|e| {
                    Expression::UnaryOp {
                        operator: "!".to_string(),
                        operand: Box::new(e),
                        meta: HashMap::new(),
                    }
                }).unwrap_or(Expression::BoolLiteral { value: true, meta: HashMap::new() });
                stmts.push(Statement::If {
                    condition: cond,
                    then_body: lower_pipeline(pipeline)?,
                    else_body: None,
                    meta: HashMap::new(),
                });
            }
        }
        prev = None;
    }

    Ok(stmts)
}

fn lower_pipeline(pipeline: &Pipeline) -> anyhow::Result<Vec<Statement>> {
    if pipeline.seq.is_empty() {
        return Ok(Vec::new());
    }
    if pipeline.seq.len() == 1 {
        return lower_command(&pipeline.seq[0]);
    }
    let mut segments = Vec::new();
    for cmd in &pipeline.seq {
        let stmts = lower_command(cmd)?;
        for s in stmts {
            segments.push(expr_from_statement(&s));
        }
    }
    Ok(vec![Statement::ExprStmt {
        expr: Expression::Pipeline { segments, meta: HashMap::new() },
        meta: HashMap::new(),
    }])
}

fn lower_command(cmd: &Command) -> anyhow::Result<Vec<Statement>> {
    match cmd {
        Command::Simple(simple) => lower_simple_command(simple),
        Command::Compound(compound, _) => lower_compound_command(compound),
        Command::Function(_) => Ok(Vec::new()),
        Command::ExtendedTest(..) => Ok(Vec::new()),
    }
}

fn lower_simple_command(simple: &ast::SimpleCommand) -> anyhow::Result<Vec<Statement>> {
    let mut stmts: Vec<Statement> = Vec::new();

    if let Some(prefix) = &simple.prefix {
        for item in &prefix.0 {
            if let ast::CommandPrefixOrSuffixItem::AssignmentWord(assignment, _w) = item {
                let name = match &assignment.name {
                    ast::AssignmentName::VariableName(n) => n.clone(),
                    ast::AssignmentName::ArrayElementName(n, _) => n.clone(),
                };
                let val = match &assignment.value {
                    ast::AssignmentValue::Scalar(w) => word_to_expr(w),
                    ast::AssignmentValue::Array(..) => {
                        Expression::StringLiteral { value: String::new(), meta: HashMap::new() }
                    }
                };
                stmts.push(Statement::VarDecl {
                    name,
                    value: val,
                    type_hint: CastType::Any,
                    meta: HashMap::new(),
                });
            }
        }
    }

    let cmd_name = simple.word_or_name.as_ref().map(|w| w.value.as_str()).unwrap_or("").to_string();
    let mut args: Vec<Expression> = Vec::new();

    // Collect suffix words AND assignment words
    let mut suffix_assignments: Vec<(String, Expression)> = Vec::new();
    if let Some(suffix) = &simple.suffix {
        for item in &suffix.0 {
            match item {
                ast::CommandPrefixOrSuffixItem::Word(w) => {
                    args.push(word_to_expr(w));
                }
                ast::CommandPrefixOrSuffixItem::AssignmentWord(assignment, _w) => {
                    let name = match &assignment.name {
                        ast::AssignmentName::VariableName(n) => n.clone(),
                        ast::AssignmentName::ArrayElementName(n, _) => n.clone(),
                    };
                    let val = match &assignment.value {
                        ast::AssignmentValue::Scalar(w) => word_to_expr(w),
                        ast::AssignmentValue::Array(..) => {
                            Expression::StringLiteral { value: String::new(), meta: HashMap::new() }
                        }
                    };
                    suffix_assignments.push((name, val));
                }
                ast::CommandPrefixOrSuffixItem::IoRedirect(_) => {}
                ast::CommandPrefixOrSuffixItem::ProcessSubstitution(..) => {}
            }
        }
    }

    if cmd_name.is_empty() && args.is_empty() && suffix_assignments.is_empty() {
        return Ok(stmts);
    }

    match cmd_name.as_str() {
        "echo" | "printf" => {
            stmts.push(Statement::ExprStmt {
                expr: Expression::CapabilityCall {
                    name: "io.print".to_string(),
                    args,
                    meta: cap_meta("io", "print"),
                },
                meta: HashMap::new(),
            });
        }
        "read" => {
            stmts.push(Statement::ExprStmt {
                expr: Expression::CapabilityCall {
                    name: "io.readline".to_string(),
                    args,
                    meta: cap_meta("io", "readline"),
                },
                meta: HashMap::new(),
            });
        }
        "cat" | "head" | "tail" | "wc" | "sort" | "grep" => {
            stmts.push(Statement::ExprStmt {
                expr: Expression::CapabilityCall {
                    name: "fs.read".to_string(),
                    args,
                    meta: cap_meta("fs", "read"),
                },
                meta: HashMap::new(),
            });
        }
        "local" => {
            for (name, val) in suffix_assignments {
                stmts.push(Statement::VarDecl {
                    name,
                    value: val,
                    type_hint: CastType::Any,
                    meta: HashMap::new(),
                });
            }
        }
        "exit" => {
            stmts.push(Statement::Return {
                value: args.into_iter().next(),
                meta: HashMap::new(),
            });
        }
        "cd" => {
            stmts.push(Statement::ExprStmt {
                expr: Expression::CapabilityCall {
                    name: "env.set".to_string(),
                    args: vec![
                        Expression::StringLiteral { value: "PWD".to_string(), meta: HashMap::new() },
                        args.into_iter().next().unwrap_or(Expression::StringLiteral { value: "~".to_string(), meta: HashMap::new() }),
                    ],
                    meta: cap_meta("env", "set"),
                },
                meta: HashMap::new(),
            });
        }
        "export" => {
            if !suffix_assignments.is_empty() {
                for (name, val) in suffix_assignments {
                    stmts.push(Statement::Export {
                        name,
                        value: val,
                        meta: HashMap::new(),
                    });
                }
            } else if !args.is_empty() {
                stmts.push(Statement::Export {
                    name: String::new(),
                    value: args.into_iter().next().unwrap_or(Expression::NullLiteral { meta: HashMap::new() }),
                    meta: HashMap::new(),
                });
            }
        }
        "true" | ":" => {}
        _ => {
            stmts.push(Statement::ExprStmt {
                expr: Expression::Call {
                    function: cmd_name,
                    args,
                    meta: HashMap::new(),
                },
                meta: HashMap::new(),
            });
        }
    }

    Ok(stmts)
}

fn lower_compound_command(compound: &CompoundCommand) -> anyhow::Result<Vec<Statement>> {
    match compound {
        CompoundCommand::IfClause(if_cmd) => {
            let condition = compound_list_to_expr(&if_cmd.condition);
            let then_body = lower_compound_list(&if_cmd.then)?;
            let else_body = if let Some(elses) = &if_cmd.elses {
                if elses.is_empty() {
                    None
                } else {
                    let mut else_stmts = Vec::new();
                    for else_clause in elses {
                        if let Some(cond) = &else_clause.condition {
                            let cond_expr = compound_list_to_expr(cond);
                            let body = lower_compound_list(&else_clause.body)?;
                            else_stmts.push(Statement::If {
                                condition: cond_expr,
                                then_body: body,
                                else_body: None,
                                meta: HashMap::new(),
                            });
                        } else {
                            let body = lower_compound_list(&else_clause.body)?;
                            else_stmts.extend(body);
                        }
                    }
                    Some(else_stmts)
                }
            } else {
                None
            };
            Ok(vec![Statement::If {
                condition,
                then_body,
                else_body,
                meta: HashMap::new(),
            }])
        }
        CompoundCommand::WhileClause(cmd) | CompoundCommand::UntilClause(cmd) => {
            let condition = compound_list_to_expr(&cmd.0);
            let body = lower_compound_list(&cmd.1.list)?;
            Ok(vec![Statement::While {
                condition: Box::new(condition),
                body,
                meta: HashMap::new(),
            }])
        }
        CompoundCommand::ForClause(for_cmd) => {
            let iterable = if let Some(values) = &for_cmd.values {
                let elements: Vec<Expression> = values.iter().map(|w| word_to_expr(w)).collect();
                Expression::ArrayLiteral { elements, meta: HashMap::new() }
            } else {
                Expression::Var { name: "@".to_string(), meta: HashMap::new() }
            };
            let body = lower_compound_list(&for_cmd.body.list)?;
            Ok(vec![Statement::For {
                variable: for_cmd.variable_name.clone(),
                iterable: Box::new(iterable),
                body,
                meta: HashMap::new(),
            }])
        }
        CompoundCommand::BraceGroup(group) => {
            lower_compound_list(&group.list)
        }
        CompoundCommand::Subshell(ss) => {
            lower_compound_list(&ss.list)
        }
        _ => Ok(Vec::new()),
    }
}

fn lower_compound_list(list: &CompoundList) -> anyhow::Result<Vec<Statement>> {
    let mut stmts = Vec::new();
    for item in &list.0 {
        stmts.extend(lower_and_or_list(&item.0)?);
    }
    Ok(stmts)
}

fn compound_list_to_expr(list: &CompoundList) -> Expression {
    let mut last_expr = Expression::BoolLiteral { value: true, meta: HashMap::new() };
    for item in &list.0 {
        let and_or = &item.0;
        for cmd in &and_or.first.seq {
            if let Some(expr) = expr_from_command(cmd) {
                last_expr = expr;
            }
        }
        for additional in &and_or.additional {
            match additional {
                AndOr::And(p) | AndOr::Or(p) => {
                    for cmd in &p.seq {
                        if let Some(expr) = expr_from_command(cmd) {
                            last_expr = expr;
                        }
                    }
                }
            }
        }
    }
    last_expr
}

fn expr_from_command(cmd: &Command) -> Option<Expression> {
    match cmd {
        Command::Simple(simple) => expr_from_simple(simple),
        _ => None,
    }
}

fn expr_from_simple(simple: &ast::SimpleCommand) -> Option<Expression> {
    let cmd_name = simple.word_or_name.as_ref().map(|w| w.value.as_str()).unwrap_or("");
    let args: Vec<Expression> = simple.suffix.as_ref().map(|s| {
        s.0.iter().filter_map(|item| {
            match item {
                ast::CommandPrefixOrSuffixItem::Word(w) => Some(word_to_expr(w)),
                _ => None,
            }
        }).collect()
    }).unwrap_or_default();

    match cmd_name {
        "" if !args.is_empty() => Some(args.into_iter().next().unwrap()),
        "test" => {
            if args.len() == 3 {
                // Binary: test ARG1 OP ARG2
                Some(Expression::BinaryOp {
                    operator: match &args[1] {
                        Expression::StringLiteral { value, .. } => value.clone(),
                        _ => "==".to_string(),
                    },
                    left: Box::new(args[0].clone()),
                    right: Box::new(args[2].clone()),
                    meta: HashMap::new(),
                })
            } else if args.len() == 2 {
                // Unary: test OP ARG
                Some(Expression::Call {
                    function: format!("test_{}", match &args[0] {
                        Expression::StringLiteral { value, .. } => value.clone(),
                        _ => "?".to_string(),
                    }),
                    args: vec![args[1].clone()],
                    meta: HashMap::new(),
                })
            } else if args.len() == 1 {
                Some(args.into_iter().next().unwrap())
            } else {
                None
            }
        }
        "[" => {
            // [ ARG1 OP ARG2 ] or [ OP ARG ]
            let stripped: Vec<Expression> = args.into_iter().take_while(|a| {
                !matches!(a, Expression::StringLiteral { value, .. } if value == "]")
            }).collect();
            if stripped.len() == 3 {
                Some(Expression::BinaryOp {
                    operator: match &stripped[1] {
                        Expression::StringLiteral { value, .. } => value.clone(),
                        _ => "==".to_string(),
                    },
                    left: Box::new(stripped[0].clone()),
                    right: Box::new(stripped[2].clone()),
                    meta: HashMap::new(),
                })
            } else if stripped.len() == 2 {
                Some(Expression::Call {
                    function: format!("test_{}", match &stripped[0] {
                        Expression::StringLiteral { value, .. } => value.clone(),
                        _ => "?".to_string(),
                    }),
                    args: vec![stripped[1].clone()],
                    meta: HashMap::new(),
                })
            } else if stripped.len() == 1 {
                Some(stripped.into_iter().next().unwrap())
            } else {
                None
            }
        }
        _ => {
            if !cmd_name.is_empty() {
                Some(Expression::Call {
                    function: cmd_name.to_string(),
                    args,
                    meta: HashMap::new(),
                })
            } else {
                None
            }
        }
    }
}

fn expr_from_statement(stmt: &Statement) -> Expression {
    match stmt {
        Statement::ExprStmt { expr, .. } => expr.clone(),
        Statement::VarDecl { name, value, .. } => Expression::BinaryOp {
            operator: "=".to_string(),
            left: Box::new(Expression::Var { name: name.clone(), meta: HashMap::new() }),
            right: Box::new(value.clone()),
            meta: HashMap::new(),
        },
        _ => Expression::BoolLiteral { value: true, meta: HashMap::new() },
    }
}

/// Extract variable names from a string like "Hello $NAME, age $AGE".
/// Returns Vec of (literal_before, var_name) pairs, ending with a final literal.
fn extract_var_refs(s: &str) -> (Vec<(String, String)>, String) {
    let mut parts = Vec::new();
    let mut remaining = s;
    loop {
        let dollar_pos = match remaining.find('$') {
            Some(p) => p,
            None => return (parts, remaining.to_string()),
        };

        let literal_before = remaining[..dollar_pos].to_string();
        let after_dollar = &remaining[dollar_pos + 1..];

        if after_dollar.starts_with('{') {
            if let Some(close) = after_dollar.find('}') {
                let var_name = after_dollar[1..close].to_string();
                parts.push((literal_before, var_name));
                remaining = &after_dollar[close + 1..];
            } else {
                return (parts, remaining.to_string());
            }
        } else {
            let name_len = after_dollar.chars().take_while(|c| c.is_alphanumeric() || *c == '_').count();
            if name_len > 0 {
                let var_name = after_dollar[..name_len].to_string();
                parts.push((literal_before, var_name));
                remaining = &after_dollar[name_len..];
            } else {
                return (parts, remaining.to_string());
            }
        }
    }
}

pub fn word_to_expr(word: &ast::Word) -> Expression {
    let raw = &word.value;

    // Bare $VAR reference (unquoted)
    if raw.starts_with('$') && !raw.starts_with('"') && !raw.starts_with('\'') {
        let name = if raw.starts_with("${") && raw.ends_with('}') {
            raw[2..raw.len()-1].to_string()
        } else {
            raw[1..].to_string()
        };
        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Expression::Var { name, meta: HashMap::new() };
        }
        return Expression::StringLiteral { value: raw.clone(), meta: HashMap::new() };
    }

    // Handle quoted strings
    let is_single_quoted = raw.starts_with('\'') && raw.ends_with('\'') && raw.len() >= 2;
    let is_double_quoted = raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2;

    if is_single_quoted || is_double_quoted {
        let inner = &raw[1..raw.len()-1];

        if is_single_quoted {
            return Expression::StringLiteral { value: inner.to_string(), meta: HashMap::new() };
        }

        // Double-quoted: expand $VAR references
        if inner.contains('$') {
            let (var_parts, final_lit) = extract_var_refs(inner);

            if var_parts.is_empty() {
                return Expression::StringLiteral { value: inner.to_string(), meta: HashMap::new() };
            }

            // Build concatenation: parts + final
            if var_parts.len() == 1 && var_parts[0].0.is_empty() && final_lit.is_empty() {
                return Expression::Var { name: var_parts[0].1.clone(), meta: HashMap::new() };
            }

            let mut result: Option<Expression> = None;
            for (lit, var_name) in &var_parts {
                if !lit.is_empty() {
                    let lit_expr = Expression::StringLiteral { value: lit.clone(), meta: HashMap::new() };
                    let var_expr = Expression::Var { name: var_name.clone(), meta: HashMap::new() };
                    result = Some(match result {
                        Some(acc) => Expression::BinaryOp {
                            operator: "+".to_string(),
                            left: Box::new(acc),
                            right: Box::new(Expression::BinaryOp {
                                operator: "+".to_string(),
                                left: Box::new(lit_expr),
                                right: Box::new(var_expr),
                                meta: HashMap::new(),
                            }),
                            meta: HashMap::new(),
                        },
                        None => Expression::BinaryOp {
                            operator: "+".to_string(),
                            left: Box::new(lit_expr),
                            right: Box::new(var_expr),
                            meta: HashMap::new(),
                        },
                    });
                } else {
                    let var_expr = Expression::Var { name: var_name.clone(), meta: HashMap::new() };
                    result = Some(match result {
                        Some(acc) => Expression::BinaryOp {
                            operator: "+".to_string(),
                            left: Box::new(acc),
                            right: Box::new(var_expr),
                            meta: HashMap::new(),
                        },
                        None => var_expr,
                    });
                }
            }
            if !final_lit.is_empty() {
                let lit_expr = Expression::StringLiteral { value: final_lit, meta: HashMap::new() };
                result = Some(match result {
                    Some(acc) => Expression::BinaryOp {
                        operator: "+".to_string(),
                        left: Box::new(acc),
                        right: Box::new(lit_expr),
                        meta: HashMap::new(),
                    },
                    None => lit_expr,
                });
            }
            return result.unwrap_or(Expression::StringLiteral { value: inner.to_string(), meta: HashMap::new() });
        }

        return Expression::StringLiteral { value: inner.to_string(), meta: HashMap::new() };
    }

    Expression::StringLiteral { value: raw.to_string(), meta: HashMap::new() }
}

fn cap_meta(namespace: &str, method: &str) -> HashMap<String, serde_json::Value> {
    let mut meta = HashMap::new();
    meta.insert("capability".to_string(), serde_json::Value::Bool(true));
    meta.insert("namespace".to_string(), serde_json::Value::String(namespace.to_string()));
    meta.insert("method".to_string(), serde_json::Value::String(method.to_string()));
    meta
}

fn lower_compound_for_body(compound: &CompoundCommand) -> Vec<Statement> {
    match compound {
        CompoundCommand::BraceGroup(group) => {
            lower_compound_list(&group.list).unwrap_or_default()
        }
        _ => lower_compound_command(compound).unwrap_or_default(),
    }
}
