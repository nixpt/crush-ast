use crush_cast::{CastType, Expression, Function, Program, Statement};
use std::collections::HashMap;
use zshrs_parse::lexer::untokenize;
use zshrs_parse::parser::*;

fn clean(s: &str) -> String {
    untokenize(s)
}

pub fn lower_program(program: &ZshProgram) -> anyhow::Result<Program> {
    let mut functions: HashMap<String, Function> = HashMap::new();
    let mut main_body: Vec<Statement> = Vec::new();

    for list in &program.lists {
        let (stmts, funcs) = lower_sublist(&list.sublist)?;
        for (name, func) in funcs {
            functions.entry(name).or_insert(func);
        }
        main_body.extend(stmts);
    }

    if !main_body.is_empty() {
        functions.insert(
            "main".to_string(),
            Function {
                params: vec![],
                body: main_body,
                meta: HashMap::new(),
                ..Default::default()
            },
        );
    }

    Ok(Program {
        cast_version: "0.2".to_string(),
        entry: "main".to_string(),
        lang: Some("zsh".to_string()),
        functions,
        ai_meta: None,
        ..Default::default()
    })
}

/// Lower a ZshSublist to statements. A sublist is pipelines connected
/// by && or ||. Returns extracted function definitions too.
fn lower_sublist(
    sublist: &ZshSublist,
) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let (stmts, funcs) = lower_pipe(&sublist.pipe)?;

    if let Some((op, next)) = &sublist.next {
        let (next_stmts, next_funcs) = lower_sublist(next)?;
        let mut all_funcs = funcs;
        all_funcs.extend(next_funcs);

        match op {
            SublistOp::And => {
                let cond = expr_from_last_stmt(&stmts);
                Ok((wrap_if(cond, next_stmts, None), all_funcs))
            }
            SublistOp::Or => {
                let cond = expr_from_last_stmt(&stmts);
                let not_cond = Expression::UnaryOp {
                    operator: "!".to_string(),
                    operand: Box::new(cond),
                    meta: HashMap::new(),
                };
                Ok((wrap_if(not_cond, next_stmts, None), all_funcs))
            }
        }
    } else {
        Ok((stmts, funcs))
    }
}

/// Lower a ZshPipe to statements. A pipe is commands connected by |.
/// For a single command, return its statements directly.
/// For a multi-command pipeline, wrap in a Pipeline expression.
fn lower_pipe(pipe: &ZshPipe) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    if pipe.next.is_none() {
        return lower_command(&pipe.cmd);
    }

    let mut segments: Vec<Expression> = Vec::new();
    let mut funcs = Vec::new();
    let mut current = Some(pipe);
    while let Some(p) = current {
        let (stmts, mut fs) = lower_command(&p.cmd)?;
        funcs.extend(fs.drain(..));
        for s in stmts {
            segments.push(expr_from_statement(&s));
        }
        current = p.next.as_deref();
    }

    Ok((
        vec![Statement::ExprStmt {
            expr: Expression::Pipeline {
                segments,
                meta: HashMap::new(),
            },
            meta: HashMap::new(),
        }],
        funcs,
    ))
}

fn lower_command(cmd: &ZshCommand) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    match cmd {
        ZshCommand::Simple(simple) => lower_simple(simple),
        ZshCommand::FuncDef(func) => lower_funcdef(func),
        ZshCommand::Subsh(prog) | ZshCommand::Cursh(prog) => {
            let lowered = lower_program(prog.as_ref())?;
            let body = lowered
                .functions
                .get("main")
                .map(|f| f.body.clone())
                .unwrap_or_default();
            Ok((body, Vec::new()))
        }
        ZshCommand::If(if_cmd) => lower_if(if_cmd),
        ZshCommand::While(w) => lower_while(w),
        ZshCommand::Until(w) => lower_until(w),
        ZshCommand::For(f) => lower_for(f),
        ZshCommand::Case(case) => lower_case(case),
        ZshCommand::Repeat(rep) => lower_repeat(rep),
        ZshCommand::Try(try_cmd) => lower_try(try_cmd),
        ZshCommand::Cond(cond) => lower_cond(cond),
        ZshCommand::Arith(expr) => lower_arith(expr),
        ZshCommand::Time(body) => {
            if let Some(sublist) = body {
                lower_sublist(sublist)
            } else {
                Ok((Vec::new(), Vec::new()))
            }
        }
        ZshCommand::Redirected(cmd_inner, _) => lower_command(cmd_inner),
    }
}

fn lower_simple(simple: &ZshSimple) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let mut stmts: Vec<Statement> = Vec::new();

    for assign in &simple.assigns {
        let value = assign_value_to_expr(&assign.value);
        stmts.push(Statement::VarDecl {
            name: assign.name.clone(),
            value,
            type_hint: CastType::Any,
            meta: HashMap::new(),
        });
    }

    if simple.words.is_empty() {
        return Ok((stmts, Vec::new()));
    }

    let cmd_name = &simple.words[0];
    let args: Vec<Expression> = simple.words[1..].iter().map(|w| word_to_expr(w)).collect();

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
        "exit" | "return" => {
            stmts.push(Statement::Return {
                value: args.into_iter().next(),
                meta: HashMap::new(),
            });
        }
        "unset" => {
            for arg in args {
                if let Expression::StringLiteral { value: name, .. } = &arg {
                    stmts.push(Statement::VarDecl {
                        name: name.clone(),
                        value: Expression::NullLiteral {
                            meta: HashMap::new(),
                        },
                        type_hint: CastType::Any,
                        meta: HashMap::new(),
                    });
                }
            }
        }
        "source" | "." => {
            stmts.push(Statement::ExprStmt {
                expr: Expression::CapabilityCall {
                    name: "bash.source".to_string(),
                    args,
                    meta: cap_meta("bash", "source"),
                },
                meta: HashMap::new(),
            });
        }
        "cd" => {
            stmts.push(Statement::ExprStmt {
                expr: Expression::CapabilityCall {
                    name: "env.set".to_string(),
                    args: vec![
                        Expression::StringLiteral {
                            value: "PWD".to_string(),
                            meta: HashMap::new(),
                        },
                        args.into_iter()
                            .next()
                            .unwrap_or(Expression::StringLiteral {
                                value: "~".to_string(),
                                meta: HashMap::new(),
                            }),
                    ],
                    meta: cap_meta("env", "set"),
                },
                meta: HashMap::new(),
            });
        }
        "export" => {
            for arg in args {
                stmts.push(Statement::Export {
                    name: String::new(),
                    value: arg,
                    meta: HashMap::new(),
                });
            }
        }
        "true" | ":" => {}
        "local" => {}
        _ => {
            stmts.push(Statement::ExprStmt {
                expr: Expression::Call {
                    function: cmd_name.clone(),
                    args,
                    meta: HashMap::new(),
                },
                meta: HashMap::new(),
            });
        }
    }

    Ok((stmts, Vec::new()))
}

fn lower_funcdef(func: &ZshFuncDef) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let body_prog = lower_program(func.body.as_ref())?;
    let body = body_prog
        .functions
        .get("main")
        .map(|f| f.body.clone())
        .unwrap_or_default();

    let mut funcs = Vec::new();
    for name in &func.names {
        funcs.push((
            name.clone(),
            Function {
                params: vec![],
                body: body.clone(),
                meta: HashMap::new(),
                ..Default::default()
            },
        ));
    }

    let mut stmts = Vec::new();
    if let Some(args) = &func.auto_call_args {
        let call_args: Vec<Expression> = args
            .iter()
            .map(|a| Expression::StringLiteral {
                value: a.clone(),
                meta: HashMap::new(),
            })
            .collect();
        if let Some(name) = func.names.first() {
            stmts.push(Statement::ExprStmt {
                expr: Expression::Call {
                    function: name.clone(),
                    args: call_args,
                    meta: HashMap::new(),
                },
                meta: HashMap::new(),
            });
        }
    }

    Ok((stmts, funcs))
}

fn lower_if(if_cmd: &ZshIf) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let condition = program_to_expr(&if_cmd.cond);
    let then_prog = lower_program(if_cmd.then.as_ref())?;
    let then_body = then_prog
        .functions
        .get("main")
        .map(|f| f.body.clone())
        .unwrap_or_default();

    let mut else_body: Option<Vec<Statement>> = None;
    if !if_cmd.elif.is_empty() || if_cmd.else_.is_some() {
        let mut combined = Vec::new();
        for (elif_cond, elif_then) in &if_cmd.elif {
            let elif_cond_expr = program_to_expr(elif_cond);
            let elif_then_prog = lower_program(elif_then)?;
            let elif_then_body = elif_then_prog
                .functions
                .get("main")
                .map(|f| f.body.clone())
                .unwrap_or_default();
            combined.push(Statement::If {
                condition: elif_cond_expr,
                then_body: elif_then_body,
                else_body: None,
                meta: HashMap::new(),
            });
        }
        if let Some(else_) = &if_cmd.else_ {
            let else_prog = lower_program(else_.as_ref())?;
            let else_body_stmts = else_prog
                .functions
                .get("main")
                .map(|f| f.body.clone())
                .unwrap_or_default();
            combined.extend(else_body_stmts);
        }
        else_body = Some(combined);
    }

    Ok((
        vec![Statement::If {
            condition,
            then_body,
            else_body,
            meta: HashMap::new(),
        }],
        Vec::new(),
    ))
}

fn lower_while(w: &ZshWhile) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let condition = program_to_expr(&w.cond);
    let body_prog = lower_program(w.body.as_ref())?;
    let body = body_prog
        .functions
        .get("main")
        .map(|f| f.body.clone())
        .unwrap_or_default();

    Ok((
        vec![Statement::While {
            condition: Box::new(condition),
            body,
            meta: HashMap::new(),
        }],
        Vec::new(),
    ))
}

fn lower_until(w: &ZshWhile) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let condition = Expression::UnaryOp {
        operator: "!".to_string(),
        operand: Box::new(program_to_expr(&w.cond)),
        meta: HashMap::new(),
    };
    let body_prog = lower_program(w.body.as_ref())?;
    let body = body_prog
        .functions
        .get("main")
        .map(|f| f.body.clone())
        .unwrap_or_default();

    Ok((
        vec![Statement::While {
            condition: Box::new(condition),
            body,
            meta: HashMap::new(),
        }],
        Vec::new(),
    ))
}

fn lower_for(f: &ZshFor) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let iterable = match &f.list {
        ForList::Words(words) => {
            let elements: Vec<Expression> = words.iter().map(|w| word_to_expr(w)).collect();
            Expression::ArrayLiteral {
                elements,
                meta: HashMap::new(),
            }
        }
        ForList::Positional => Expression::Var {
            name: "@".to_string(),
            meta: HashMap::new(),
        },
        ForList::CStyle { init, cond, step } => {
            let desc = format!("{}; {}; {}", init, cond, step);
            Expression::CapabilityCall {
                name: "bash.arithmetic_for".to_string(),
                args: vec![Expression::StringLiteral {
                    value: desc,
                    meta: HashMap::new(),
                }],
                meta: cap_meta("bash", "arithmetic_for"),
            }
        }
    };

    let body_prog = lower_program(f.body.as_ref())?;
    let body = body_prog
        .functions
        .get("main")
        .map(|f| f.body.clone())
        .unwrap_or_default();

    Ok((
        vec![Statement::For {
            variable: f.var.clone(),
            iterable: Box::new(iterable),
            body,
            meta: HashMap::new(),
        }],
        Vec::new(),
    ))
}

fn lower_case(case: &ZshCase) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let subject = word_to_expr(&case.word);
    let mut stmts = Vec::new();

    for arm in &case.arms {
        let condition = if arm.patterns.len() == 1 {
            Expression::BinaryOp {
                operator: "==".to_string(),
                left: Box::new(subject.clone()),
                right: Box::new(Expression::StringLiteral {
                    value: arm.patterns[0].clone(),
                    meta: HashMap::new(),
                }),
                meta: HashMap::new(),
            }
        } else {
            let mut or_chain = Expression::BinaryOp {
                operator: "==".to_string(),
                left: Box::new(subject.clone()),
                right: Box::new(Expression::StringLiteral {
                    value: arm.patterns[0].clone(),
                    meta: HashMap::new(),
                }),
                meta: HashMap::new(),
            };
            for pat in &arm.patterns[1..] {
                or_chain = Expression::BinaryOp {
                    operator: "or".to_string(),
                    left: Box::new(or_chain),
                    right: Box::new(Expression::BinaryOp {
                        operator: "==".to_string(),
                        left: Box::new(subject.clone()),
                        right: Box::new(Expression::StringLiteral {
                            value: pat.clone(),
                            meta: HashMap::new(),
                        }),
                        meta: HashMap::new(),
                    }),
                    meta: HashMap::new(),
                };
            }
            or_chain
        };

        let body_prog = lower_program(&arm.body)?;
        let body = body_prog
            .functions
            .get("main")
            .map(|f| f.body.clone())
            .unwrap_or_default();

        stmts.push(Statement::If {
            condition,
            then_body: body,
            else_body: None,
            meta: HashMap::new(),
        });
    }

    Ok((stmts, Vec::new()))
}

fn lower_repeat(rep: &ZshRepeat) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let count = word_to_expr(&rep.count);
    let body_prog = lower_program(rep.body.as_ref())?;
    let body = body_prog
        .functions
        .get("main")
        .map(|f| f.body.clone())
        .unwrap_or_default();

    let condition = Expression::BinaryOp {
        operator: ">".to_string(),
        left: Box::new(Expression::Var {
            name: "__repeat_i".to_string(),
            meta: HashMap::new(),
        }),
        right: Box::new(Expression::StringLiteral {
            value: "0".to_string(),
            meta: HashMap::new(),
        }),
        meta: HashMap::new(),
    };

    let mut loop_body = body;
    loop_body.push(Statement::ExprStmt {
        expr: Expression::BinaryOp {
            operator: "-=".to_string(),
            left: Box::new(Expression::Var {
                name: "__repeat_i".to_string(),
                meta: HashMap::new(),
            }),
            right: Box::new(Expression::StringLiteral {
                value: "1".to_string(),
                meta: HashMap::new(),
            }),
            meta: HashMap::new(),
        },
        meta: HashMap::new(),
    });

    Ok((
        vec![
            Statement::VarDecl {
                name: "__repeat_i".to_string(),
                value: count,
                type_hint: CastType::Any,
                meta: HashMap::new(),
            },
            Statement::While {
                condition: Box::new(condition),
                body: loop_body,
                meta: HashMap::new(),
            },
        ],
        Vec::new(),
    ))
}

fn lower_try(try_cmd: &ZshTry) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let try_prog = lower_program(try_cmd.try_block.as_ref())?;
    let try_body = try_prog
        .functions
        .get("main")
        .map(|f| f.body.clone())
        .unwrap_or_default();
    let always_prog = lower_program(try_cmd.always.as_ref())?;
    let always_body = always_prog
        .functions
        .get("main")
        .map(|f| f.body.clone())
        .unwrap_or_default();
    let mut body = try_body;
    body.extend(always_body);
    Ok((body, Vec::new()))
}

fn lower_cond(cond: &ZshCond) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let expr = cond_to_expr(cond);
    Ok((
        vec![Statement::ExprStmt {
            expr,
            meta: HashMap::new(),
        }],
        Vec::new(),
    ))
}

fn lower_arith(expr: &str) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    Ok((
        vec![Statement::ExprStmt {
            expr: Expression::CapabilityCall {
                name: "bash.arithmetic".to_string(),
                args: vec![Expression::StringLiteral {
                    value: expr.to_string(),
                    meta: HashMap::new(),
                }],
                meta: cap_meta("bash", "arithmetic"),
            },
            meta: HashMap::new(),
        }],
        Vec::new(),
    ))
}

fn cond_to_expr(cond: &ZshCond) -> Expression {
    match cond {
        ZshCond::Not(inner) => Expression::UnaryOp {
            operator: "!".to_string(),
            operand: Box::new(cond_to_expr(inner)),
            meta: HashMap::new(),
        },
        ZshCond::And(a, b) => Expression::BinaryOp {
            operator: "and".to_string(),
            left: Box::new(cond_to_expr(a)),
            right: Box::new(cond_to_expr(b)),
            meta: HashMap::new(),
        },
        ZshCond::Or(a, b) => Expression::BinaryOp {
            operator: "or".to_string(),
            left: Box::new(cond_to_expr(a)),
            right: Box::new(cond_to_expr(b)),
            meta: HashMap::new(),
        },
        ZshCond::Unary(op, val) => Expression::Call {
            function: format!("test_{}", op),
            args: vec![Expression::StringLiteral {
                value: val.clone(),
                meta: HashMap::new(),
            }],
            meta: HashMap::new(),
        },
        ZshCond::Binary(a, op, b) => Expression::BinaryOp {
            operator: op.clone(),
            left: Box::new(Expression::StringLiteral {
                value: a.clone(),
                meta: HashMap::new(),
            }),
            right: Box::new(Expression::StringLiteral {
                value: b.clone(),
                meta: HashMap::new(),
            }),
            meta: HashMap::new(),
        },
        ZshCond::Regex(val, pat) => Expression::Call {
            function: "test_regex".to_string(),
            args: vec![
                Expression::StringLiteral {
                    value: val.clone(),
                    meta: HashMap::new(),
                },
                Expression::StringLiteral {
                    value: pat.clone(),
                    meta: HashMap::new(),
                },
            ],
            meta: HashMap::new(),
        },
    }
}

fn program_to_expr(prog: &ZshProgram) -> Expression {
    let mut last_expr = Expression::BoolLiteral {
        value: true,
        meta: HashMap::new(),
    };
    for list in &prog.lists {
        last_expr = sublist_to_expr(&list.sublist);
    }
    last_expr
}

fn sublist_to_expr(sublist: &ZshSublist) -> Expression {
    pipe_to_expr(&sublist.pipe)
}

fn pipe_to_expr(pipe: &ZshPipe) -> Expression {
    command_to_expr(&pipe.cmd)
}

fn command_to_expr(cmd: &ZshCommand) -> Expression {
    match cmd {
        ZshCommand::Simple(simple) => {
            let first = simple.words.first().map(|s| s.as_str()).unwrap_or("");
            let args: Vec<Expression> = simple.words[1..].iter().map(|w| word_to_expr(w)).collect();

            match first {
                "true" | ":" => Expression::BoolLiteral {
                    value: true,
                    meta: HashMap::new(),
                },
                "false" => Expression::BoolLiteral {
                    value: false,
                    meta: HashMap::new(),
                },
                "test" => expr_from_test_call(args),
                "[" => expr_from_test_call(args),
                _ => {
                    if !first.is_empty() {
                        Expression::Call {
                            function: first.to_string(),
                            args,
                            meta: HashMap::new(),
                        }
                    } else {
                        Expression::BoolLiteral {
                            value: true,
                            meta: HashMap::new(),
                        }
                    }
                }
            }
        }
        ZshCommand::Subsh(prog) | ZshCommand::Cursh(prog) => program_to_expr(prog),
        ZshCommand::Cond(cond) => cond_to_expr(cond),
        ZshCommand::Arith(expr) => Expression::CapabilityCall {
            name: "bash.arithmetic".to_string(),
            args: vec![Expression::StringLiteral {
                value: expr.clone(),
                meta: HashMap::new(),
            }],
            meta: cap_meta("bash", "arithmetic"),
        },
        ZshCommand::Redirected(cmd_inner, _) => command_to_expr(cmd_inner),
        _ => Expression::BoolLiteral {
            value: true,
            meta: HashMap::new(),
        },
    }
}

fn expr_from_test_call(args: Vec<Expression>) -> Expression {
    if args.len() == 3 {
        Expression::BinaryOp {
            operator: match &args[1] {
                Expression::StringLiteral { value, .. } => value.clone(),
                _ => "==".to_string(),
            },
            left: Box::new(args[0].clone()),
            right: Box::new(args[2].clone()),
            meta: HashMap::new(),
        }
    } else if args.len() == 2 {
        Expression::Call {
            function: format!(
                "test_{}",
                match &args[0] {
                    Expression::StringLiteral { value, .. } if value != "]" => value.clone(),
                    _ => "?".to_string(),
                }
            ),
            args: vec![args[1].clone()],
            meta: HashMap::new(),
        }
    } else if args.len() == 1 {
        args.into_iter().next().unwrap()
    } else {
        Expression::BoolLiteral {
            value: true,
            meta: HashMap::new(),
        }
    }
}

fn assign_value_to_expr(value: &ZshAssignValue) -> Expression {
    match value {
        ZshAssignValue::Scalar(s) => word_to_expr(s),
        ZshAssignValue::Array(elems) => {
            let elements: Vec<Expression> = elems.iter().map(|e| word_to_expr(e)).collect();
            Expression::ArrayLiteral {
                elements,
                meta: HashMap::new(),
            }
        }
    }
}

fn word_to_expr(word: &str) -> Expression {
    let cleaned = clean(word);
    let word = cleaned.as_str();

    // Simple $VAR at start of word
    if word.starts_with('$') {
        let name = if word.starts_with("${") && word.ends_with('}') {
            word[2..word.len() - 1].to_string()
        } else {
            word[1..].to_string()
        };
        if !name.is_empty()
            && name.chars().all(|c| {
                c.is_alphanumeric()
                    || c == '_'
                    || c == '*'
                    || c == '@'
                    || c == '?'
                    || c == '$'
                    || c == '!'
                    || c == '#'
            })
        {
            return Expression::Var {
                name,
                meta: HashMap::new(),
            };
        }
    }

    // Expand $VAR references anywhere in the word
    if word.contains('$') {
        return expand_var_refs(word);
    }

    Expression::StringLiteral {
        value: word.to_string(),
        meta: HashMap::new(),
    }
}

fn expand_var_refs(s: &str) -> Expression {
    let mut parts: Vec<Expression> = Vec::new();
    let mut remaining = s;

    loop {
        let dollar_pos = match remaining.find('$') {
            Some(p) => p,
            None => {
                if !remaining.is_empty() {
                    parts.push(Expression::StringLiteral {
                        value: remaining.to_string(),
                        meta: HashMap::new(),
                    });
                }
                break;
            }
        };

        if dollar_pos > 0 {
            parts.push(Expression::StringLiteral {
                value: remaining[..dollar_pos].to_string(),
                meta: HashMap::new(),
            });
        }

        let after_dollar = &remaining[dollar_pos + 1..];
        if after_dollar.starts_with('{') {
            if let Some(close) = after_dollar.find('}') {
                let var_name = after_dollar[1..close].to_string();
                parts.push(Expression::Var {
                    name: var_name,
                    meta: HashMap::new(),
                });
                remaining = &after_dollar[close + 1..];
            } else {
                parts.push(Expression::StringLiteral {
                    value: remaining[dollar_pos..].to_string(),
                    meta: HashMap::new(),
                });
                break;
            }
        } else {
            let name_len = after_dollar
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .count();
            if name_len > 0 {
                let var_name = after_dollar[..name_len].to_string();
                parts.push(Expression::Var {
                    name: var_name,
                    meta: HashMap::new(),
                });
                remaining = &after_dollar[name_len..];
            } else {
                parts.push(Expression::StringLiteral {
                    value: remaining[dollar_pos..].to_string(),
                    meta: HashMap::new(),
                });
                break;
            }
        }
    }

    if parts.len() == 1 {
        return parts.into_iter().next().unwrap();
    }

    let mut result = parts.remove(0);
    for part in parts {
        result = Expression::BinaryOp {
            operator: "+".to_string(),
            left: Box::new(result),
            right: Box::new(part),
            meta: HashMap::new(),
        };
    }
    result
}

fn expr_from_last_stmt(stmts: &[Statement]) -> Expression {
    stmts
        .last()
        .map(|s| expr_from_statement(s))
        .unwrap_or(Expression::BoolLiteral {
            value: true,
            meta: HashMap::new(),
        })
}

fn expr_from_statement(stmt: &Statement) -> Expression {
    match stmt {
        Statement::ExprStmt { expr, .. } => expr.clone(),
        Statement::VarDecl { name, value, .. } => Expression::BinaryOp {
            operator: "=".to_string(),
            left: Box::new(Expression::Var {
                name: name.clone(),
                meta: HashMap::new(),
            }),
            right: Box::new(value.clone()),
            meta: HashMap::new(),
        },
        _ => Expression::BoolLiteral {
            value: true,
            meta: HashMap::new(),
        },
    }
}

fn wrap_if(
    cond: Expression,
    body: Vec<Statement>,
    else_body: Option<Vec<Statement>>,
) -> Vec<Statement> {
    vec![Statement::If {
        condition: cond,
        then_body: body,
        else_body,
        meta: HashMap::new(),
    }]
}

fn cap_meta(namespace: &str, method: &str) -> HashMap<String, serde_json::Value> {
    let mut meta = HashMap::new();
    meta.insert("capability".to_string(), serde_json::Value::Bool(true));
    meta.insert(
        "namespace".to_string(),
        serde_json::Value::String(namespace.to_string()),
    );
    meta.insert(
        "method".to_string(),
        serde_json::Value::String(method.to_string()),
    );
    meta
}
