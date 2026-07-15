use crush_cast::{CastType, Expression, Function, Program, Statement};
use std::collections::HashMap;
use crush_walker_core::LowerCtx;
use zshrs_parse::lexer::untokenize;
use zshrs_parse::parser::*;

fn clean(s: &str) -> String {
    untokenize(s)
}

pub fn lower_program(program: &ZshProgram, ctx: &LowerCtx) -> anyhow::Result<Program> {
    let mut functions: HashMap<String, Function> = HashMap::new();
    let mut main_body: Vec<Statement> = Vec::new();

    for list in &program.lists {
        let (stmts, funcs) = lower_sublist(&list.sublist, ctx)?;
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
                meta: ctx.meta_at(0),
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
    ctx: &LowerCtx) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let (stmts, funcs) = lower_pipe(&sublist.pipe, ctx)?;

    if let Some((op, next)) = &sublist.next {
        let (next_stmts, next_funcs) = lower_sublist(next, ctx)?;
        let mut all_funcs = funcs;
        all_funcs.extend(next_funcs);

        match op {
            SublistOp::And => {
                let cond = expr_from_last_stmt(&stmts, ctx);
                Ok((wrap_if(cond, next_stmts, None, ctx), all_funcs))
            }
            SublistOp::Or => {
                let cond = expr_from_last_stmt(&stmts, ctx);
                let not_cond = Expression::UnaryOp {
                    operator: "!".to_string(),
                    operand: Box::new(cond),
                    meta: ctx.meta_at(0),
                };
                Ok((wrap_if(not_cond, next_stmts, None, ctx), all_funcs))
            }
        }
    } else {
        Ok((stmts, funcs))
    }
}

/// Lower a ZshPipe to statements. A pipe is commands connected by |.
/// For a single command, return its statements directly.
/// For a multi-command pipeline, wrap in a Pipeline expression.
fn lower_pipe(pipe: &ZshPipe, ctx: &LowerCtx) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    if pipe.next.is_none() {
        return lower_command(&pipe.cmd, ctx);
    }

    let mut segments: Vec<Expression> = Vec::new();
    let mut funcs = Vec::new();
    let mut current = Some(pipe);
    while let Some(p) = current {
        let (stmts, mut fs) = lower_command(&p.cmd, ctx)?;
        funcs.extend(fs.drain(..));
        for s in stmts {
            segments.push(expr_from_statement(&s, ctx));
        }
        current = p.next.as_deref();
    }

    Ok((
        vec![Statement::ExprStmt {
            expr: Expression::Pipeline {
                segments,
                meta: ctx.meta_at(0),
            },
            meta: ctx.meta_at(0),
        }],
        funcs,
    ))
}

fn lower_command(cmd: &ZshCommand, ctx: &LowerCtx) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    match cmd {
        ZshCommand::Simple(simple) => lower_simple(simple, ctx),
        ZshCommand::FuncDef(func) => lower_funcdef(func, ctx),
        ZshCommand::Subsh(prog) | ZshCommand::Cursh(prog) => {
            let lowered = lower_program(prog.as_ref(), ctx)?;
            let body = lowered
                .functions
                .get("main")
                .map(|f| f.body.clone())
                .unwrap_or_default();
            Ok((body, Vec::new()))
        }
        ZshCommand::If(if_cmd) => lower_if(if_cmd, ctx),
        ZshCommand::While(w) => lower_while(w, ctx),
        ZshCommand::Until(w) => lower_until(w, ctx),
        ZshCommand::For(f) => lower_for(f, ctx),
        ZshCommand::Case(case) => lower_case(case, ctx),
        ZshCommand::Repeat(rep) => lower_repeat(rep, ctx),
        ZshCommand::Try(try_cmd) => lower_try(try_cmd, ctx),
        ZshCommand::Cond(cond) => lower_cond(cond, ctx),
        ZshCommand::Arith(expr) => lower_arith(expr, ctx),
        ZshCommand::Time(body) => {
            if let Some(sublist) = body {
                lower_sublist(sublist, ctx)
            } else {
                Ok((Vec::new(), Vec::new()))
            }
        }
        ZshCommand::Redirected(cmd_inner, _) => lower_command(cmd_inner, ctx),
    }
}

fn lower_simple(simple: &ZshSimple, ctx: &LowerCtx) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let mut stmts: Vec<Statement> = Vec::new();

    for assign in &simple.assigns {
        let value = assign_value_to_expr(&assign.value, ctx);
        stmts.push(Statement::VarDecl {
            name: assign.name.clone(),
            value,
            type_hint: CastType::Any,
            meta: ctx.meta_at(0),
        });
    }

    if simple.words.is_empty() {
        return Ok((stmts, Vec::new()));
    }

    let cmd_name = &simple.words[0];
    let args: Vec<Expression> = simple.words[1..].iter().map(|w| word_to_expr(w, ctx)).collect();

    match cmd_name.as_str() {
        "echo" | "printf" => {
            stmts.push(Statement::ExprStmt {
                expr: Expression::CapabilityCall {
                    name: "io.print".to_string(),
                    args,
                    meta: cap_meta("io", "print"),
                },
                meta: ctx.meta_at(0),
            });
        }
        "read" => {
            stmts.push(Statement::ExprStmt {
                expr: Expression::CapabilityCall {
                    name: "io.readline".to_string(),
                    args,
                    meta: cap_meta("io", "readline"),
                },
                meta: ctx.meta_at(0),
            });
        }
        "cat" | "head" | "tail" | "wc" | "sort" | "grep" => {
            stmts.push(Statement::ExprStmt {
                expr: Expression::CapabilityCall {
                    name: "fs.read".to_string(),
                    args,
                    meta: cap_meta("fs", "read"),
                },
                meta: ctx.meta_at(0),
            });
        }
        "exit" | "return" => {
            stmts.push(Statement::Return {
                value: args.into_iter().next(),
                meta: ctx.meta_at(0),
            });
        }
        "unset" => {
            for arg in args {
                if let Expression::StringLiteral { value: name, .. } = &arg {
                    stmts.push(Statement::VarDecl {
                        name: name.clone(),
                        value: Expression::NullLiteral {
                            meta: ctx.meta_at(0),
                        },
                        type_hint: CastType::Any,
                        meta: ctx.meta_at(0),
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
                meta: ctx.meta_at(0),
            });
        }
        "cd" => {
            stmts.push(Statement::ExprStmt {
                expr: Expression::CapabilityCall {
                    name: "env.set".to_string(),
                    args: vec![
                        Expression::StringLiteral {
                            value: "PWD".to_string(),
                            meta: ctx.meta_at(0),
                        },
                        args.into_iter()
                            .next()
                            .unwrap_or(Expression::StringLiteral {
                                value: "~".to_string(),
                                meta: ctx.meta_at(0),
                            }),
                    ],
                    meta: cap_meta("env", "set"),
                },
                meta: ctx.meta_at(0),
            });
        }
        "export" => {
            for arg in args {
                stmts.push(Statement::Export {
                    name: String::new(),
                    value: arg,
                    meta: ctx.meta_at(0),
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
                    meta: ctx.meta_at(0),
                },
                meta: ctx.meta_at(0),
            });
        }
    }

    Ok((stmts, Vec::new()))
}

fn lower_funcdef(func: &ZshFuncDef, ctx: &LowerCtx) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let body_prog = lower_program(func.body.as_ref(), ctx)?;
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
                meta: ctx.meta_at(0),
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
                meta: ctx.meta_at(0),
            })
            .collect();
        if let Some(name) = func.names.first() {
            stmts.push(Statement::ExprStmt {
                expr: Expression::Call {
                    function: name.clone(),
                    args: call_args,
                    meta: ctx.meta_at(0),
                },
                meta: ctx.meta_at(0),
            });
        }
    }

    Ok((stmts, funcs))
}

fn lower_if(if_cmd: &ZshIf, ctx: &LowerCtx) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let condition = program_to_expr(&if_cmd.cond, ctx);
    let then_prog = lower_program(if_cmd.then.as_ref(), ctx)?;
    let then_body = then_prog
        .functions
        .get("main")
        .map(|f| f.body.clone())
        .unwrap_or_default();

    let mut else_body: Option<Vec<Statement>> = None;
    if !if_cmd.elif.is_empty() || if_cmd.else_.is_some() {
        let mut combined = Vec::new();
        for (elif_cond, elif_then) in &if_cmd.elif {
            let elif_cond_expr = program_to_expr(elif_cond, ctx);
            let elif_then_prog = lower_program(elif_then, ctx)?;
            let elif_then_body = elif_then_prog
                .functions
                .get("main")
                .map(|f| f.body.clone())
                .unwrap_or_default();
            combined.push(Statement::If {
                condition: elif_cond_expr,
                then_body: elif_then_body,
                else_body: None,
                meta: ctx.meta_at(0),
            });
        }
        if let Some(else_) = &if_cmd.else_ {
            let else_prog = lower_program(else_.as_ref(), ctx)?;
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
            meta: ctx.meta_at(0),
        }],
        Vec::new(),
    ))
}

fn lower_while(w: &ZshWhile, ctx: &LowerCtx) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let condition = program_to_expr(&w.cond, ctx);
    let body_prog = lower_program(w.body.as_ref(), ctx)?;
    let body = body_prog
        .functions
        .get("main")
        .map(|f| f.body.clone())
        .unwrap_or_default();

    Ok((
        vec![Statement::While {
            condition: Box::new(condition),
            body,
            meta: ctx.meta_at(0),
        }],
        Vec::new(),
    ))
}

fn lower_until(w: &ZshWhile, ctx: &LowerCtx) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let condition = Expression::UnaryOp {
        operator: "!".to_string(),
        operand: Box::new(program_to_expr(&w.cond, ctx)),
        meta: ctx.meta_at(0),
    };
    let body_prog = lower_program(w.body.as_ref(), ctx)?;
    let body = body_prog
        .functions
        .get("main")
        .map(|f| f.body.clone())
        .unwrap_or_default();

    Ok((
        vec![Statement::While {
            condition: Box::new(condition),
            body,
            meta: ctx.meta_at(0),
        }],
        Vec::new(),
    ))
}

fn lower_for(f: &ZshFor, ctx: &LowerCtx) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let iterable = match &f.list {
        ForList::Words(words) => {
            let elements: Vec<Expression> = words.iter().map(|w| word_to_expr(w, ctx)).collect();
            Expression::ArrayLiteral {
                elements,
                meta: ctx.meta_at(0),
            }
        }
        ForList::Positional => Expression::Var {
            name: "@".to_string(),
            meta: ctx.meta_at(0),
        },
        ForList::CStyle { init, cond, step } => {
            let desc = format!("{}; {}; {}", init, cond, step);
            Expression::CapabilityCall {
                name: "bash.arithmetic_for".to_string(),
                args: vec![Expression::StringLiteral {
                    value: desc,
                    meta: ctx.meta_at(0),
                }],
                meta: cap_meta("bash", "arithmetic_for"),
            }
        }
    };

    let body_prog = lower_program(f.body.as_ref(), ctx)?;
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
            meta: ctx.meta_at(0),
        }],
        Vec::new(),
    ))
}

fn lower_case(case: &ZshCase, ctx: &LowerCtx) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let subject = word_to_expr(&case.word, ctx);
    let mut stmts = Vec::new();

    for arm in &case.arms {
        let condition = if arm.patterns.len() == 1 {
            Expression::BinaryOp {
                operator: "==".to_string(),
                left: Box::new(subject.clone()),
                right: Box::new(Expression::StringLiteral {
                    value: arm.patterns[0].clone(),
                    meta: ctx.meta_at(0),
                }),
                meta: ctx.meta_at(0),
            }
        } else {
            let mut or_chain = Expression::BinaryOp {
                operator: "==".to_string(),
                left: Box::new(subject.clone()),
                right: Box::new(Expression::StringLiteral {
                    value: arm.patterns[0].clone(),
                    meta: ctx.meta_at(0),
                }),
                meta: ctx.meta_at(0),
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
                            meta: ctx.meta_at(0),
                        }),
                        meta: ctx.meta_at(0),
                    }),
                    meta: ctx.meta_at(0),
                };
            }
            or_chain
        };

        let body_prog = lower_program(&arm.body, ctx)?;
        let body = body_prog
            .functions
            .get("main")
            .map(|f| f.body.clone())
            .unwrap_or_default();

        stmts.push(Statement::If {
            condition,
            then_body: body,
            else_body: None,
            meta: ctx.meta_at(0),
        });
    }

    Ok((stmts, Vec::new()))
}

fn lower_repeat(rep: &ZshRepeat, ctx: &LowerCtx) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let count = word_to_expr(&rep.count, ctx);
    let body_prog = lower_program(rep.body.as_ref(), ctx)?;
    let body = body_prog
        .functions
        .get("main")
        .map(|f| f.body.clone())
        .unwrap_or_default();

    let condition = Expression::BinaryOp {
        operator: ">".to_string(),
        left: Box::new(Expression::Var {
            name: "__repeat_i".to_string(),
            meta: ctx.meta_at(0),
        }),
        right: Box::new(Expression::StringLiteral {
            value: "0".to_string(),
            meta: ctx.meta_at(0),
        }),
        meta: ctx.meta_at(0),
    };

    let mut loop_body = body;
    loop_body.push(Statement::ExprStmt {
        expr: Expression::BinaryOp {
            operator: "-=".to_string(),
            left: Box::new(Expression::Var {
                name: "__repeat_i".to_string(),
                meta: ctx.meta_at(0),
            }),
            right: Box::new(Expression::StringLiteral {
                value: "1".to_string(),
                meta: ctx.meta_at(0),
            }),
            meta: ctx.meta_at(0),
        },
        meta: ctx.meta_at(0),
    });

    Ok((
        vec![
            Statement::VarDecl {
                name: "__repeat_i".to_string(),
                value: count,
                type_hint: CastType::Any,
                meta: ctx.meta_at(0),
            },
            Statement::While {
                condition: Box::new(condition),
                body: loop_body,
                meta: ctx.meta_at(0),
            },
        ],
        Vec::new(),
    ))
}

fn lower_try(try_cmd: &ZshTry, ctx: &LowerCtx) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let try_prog = lower_program(try_cmd.try_block.as_ref(), ctx)?;
    let try_body = try_prog
        .functions
        .get("main")
        .map(|f| f.body.clone())
        .unwrap_or_default();
    let always_prog = lower_program(try_cmd.always.as_ref(), ctx)?;
    let always_body = always_prog
        .functions
        .get("main")
        .map(|f| f.body.clone())
        .unwrap_or_default();
    let mut body = try_body;
    body.extend(always_body);
    Ok((body, Vec::new()))
}

fn lower_cond(cond: &ZshCond, ctx: &LowerCtx) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    let expr = cond_to_expr(cond, ctx);
    Ok((
        vec![Statement::ExprStmt {
            expr,
            meta: ctx.meta_at(0),
        }],
        Vec::new(),
    ))
}

fn lower_arith(expr: &str, ctx: &LowerCtx) -> anyhow::Result<(Vec<Statement>, Vec<(String, Function)>)> {
    Ok((
        vec![Statement::ExprStmt {
            expr: Expression::CapabilityCall {
                name: "bash.arithmetic".to_string(),
                args: vec![Expression::StringLiteral {
                    value: expr.to_string(),
                    meta: ctx.meta_at(0),
                }],
                meta: cap_meta("bash", "arithmetic"),
            },
            meta: ctx.meta_at(0),
        }],
        Vec::new(),
    ))
}

fn cond_to_expr(cond: &ZshCond, ctx: &LowerCtx) -> Expression {
    match cond {
        ZshCond::Not(inner) => Expression::UnaryOp {
            operator: "!".to_string(),
            operand: Box::new(cond_to_expr(inner, ctx)),
            meta: ctx.meta_at(0),
        },
        ZshCond::And(a, b) => Expression::BinaryOp {
            operator: "and".to_string(),
            left: Box::new(cond_to_expr(a, ctx)),
            right: Box::new(cond_to_expr(b, ctx)),
            meta: ctx.meta_at(0),
        },
        ZshCond::Or(a, b) => Expression::BinaryOp {
            operator: "or".to_string(),
            left: Box::new(cond_to_expr(a, ctx)),
            right: Box::new(cond_to_expr(b, ctx)),
            meta: ctx.meta_at(0),
        },
        ZshCond::Unary(op, val) => Expression::Call {
            function: format!("test_{}", op),
            args: vec![Expression::StringLiteral {
                value: val.clone(),
                meta: ctx.meta_at(0),
            }],
            meta: ctx.meta_at(0),
        },
        ZshCond::Binary(a, op, b) => Expression::BinaryOp {
            operator: op.clone(),
            left: Box::new(Expression::StringLiteral {
                value: a.clone(),
                meta: ctx.meta_at(0),
            }),
            right: Box::new(Expression::StringLiteral {
                value: b.clone(),
                meta: ctx.meta_at(0),
            }),
            meta: ctx.meta_at(0),
        },
        ZshCond::Regex(val, pat) => Expression::Call {
            function: "test_regex".to_string(),
            args: vec![
                Expression::StringLiteral {
                    value: val.clone(),
                    meta: ctx.meta_at(0),
                },
                Expression::StringLiteral {
                    value: pat.clone(),
                    meta: ctx.meta_at(0),
                },
            ],
            meta: ctx.meta_at(0),
        },
    }
}

fn program_to_expr(prog: &ZshProgram, ctx: &LowerCtx) -> Expression {
    let mut last_expr = Expression::BoolLiteral {
        value: true,
        meta: ctx.meta_at(0),
    };
    for list in &prog.lists {
        last_expr = sublist_to_expr(&list.sublist, ctx);
    }
    last_expr
}

fn sublist_to_expr(sublist: &ZshSublist, ctx: &LowerCtx) -> Expression {
    pipe_to_expr(&sublist.pipe, ctx)
}

fn pipe_to_expr(pipe: &ZshPipe, ctx: &LowerCtx) -> Expression {
    command_to_expr(&pipe.cmd, ctx)
}

fn command_to_expr(cmd: &ZshCommand, ctx: &LowerCtx) -> Expression {
    match cmd {
        ZshCommand::Simple(simple) => {
            let first = simple.words.first().map(|s| s.as_str()).unwrap_or("");
            let args: Vec<Expression> = simple.words[1..].iter().map(|w| word_to_expr(w, ctx)).collect();

            match first {
                "true" | ":" => Expression::BoolLiteral {
                    value: true,
                    meta: ctx.meta_at(0),
                },
                "false" => Expression::BoolLiteral {
                    value: false,
                    meta: ctx.meta_at(0),
                },
                "test" => expr_from_test_call(args, ctx),
                "[" => expr_from_test_call(args, ctx),
                _ => {
                    if !first.is_empty() {
                        Expression::Call {
                            function: first.to_string(),
                            args,
                            meta: ctx.meta_at(0),
                        }
                    } else {
                        Expression::BoolLiteral {
                            value: true,
                            meta: ctx.meta_at(0),
                        }
                    }
                }
            }
        }
        ZshCommand::Subsh(prog) | ZshCommand::Cursh(prog) => program_to_expr(prog, ctx),
        ZshCommand::Cond(cond) => cond_to_expr(cond, ctx),
        ZshCommand::Arith(expr) => Expression::CapabilityCall {
            name: "bash.arithmetic".to_string(),
            args: vec![Expression::StringLiteral {
                value: expr.clone(),
                meta: ctx.meta_at(0),
            }],
            meta: cap_meta("bash", "arithmetic"),
        },
        ZshCommand::Redirected(cmd_inner, _) => command_to_expr(cmd_inner, ctx),
        _ => Expression::BoolLiteral {
            value: true,
            meta: ctx.meta_at(0),
        },
    }
}

fn expr_from_test_call(args: Vec<Expression>, ctx: &LowerCtx) -> Expression {
    if args.len() == 3 {
        Expression::BinaryOp {
            operator: match &args[1] {
                Expression::StringLiteral { value, .. } => value.clone(),
                _ => "==".to_string(),
            },
            left: Box::new(args[0].clone()),
            right: Box::new(args[2].clone()),
            meta: ctx.meta_at(0),
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
            meta: ctx.meta_at(0),
        }
    } else if args.len() == 1 {
        args.into_iter().next().unwrap()
    } else {
        Expression::BoolLiteral {
            value: true,
            meta: ctx.meta_at(0),
        }
    }
}

fn assign_value_to_expr(value: &ZshAssignValue, ctx: &LowerCtx) -> Expression {
    match value {
        ZshAssignValue::Scalar(s) => word_to_expr(s, ctx),
        ZshAssignValue::Array(elems) => {
            let elements: Vec<Expression> = elems.iter().map(|e| word_to_expr(e, ctx)).collect();
            Expression::ArrayLiteral {
                elements,
                meta: ctx.meta_at(0),
            }
        }
    }
}

fn word_to_expr(word: &str, ctx: &LowerCtx) -> Expression {
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
                meta: ctx.meta_at(0),
            };
        }
    }

    // Expand $VAR references anywhere in the word
    if word.contains('$') {
        return expand_var_refs(word, ctx);
    }

    Expression::StringLiteral {
        value: word.to_string(),
        meta: ctx.meta_at(0),
    }
}

fn expand_var_refs(s: &str, ctx: &LowerCtx) -> Expression {
    let mut parts: Vec<Expression> = Vec::new();
    let mut remaining = s;

    loop {
        let dollar_pos = match remaining.find('$') {
            Some(p) => p,
            None => {
                if !remaining.is_empty() {
                    parts.push(Expression::StringLiteral {
                        value: remaining.to_string(),
                        meta: ctx.meta_at(0),
                    });
                }
                break;
            }
        };

        if dollar_pos > 0 {
            parts.push(Expression::StringLiteral {
                value: remaining[..dollar_pos].to_string(),
                meta: ctx.meta_at(0),
            });
        }

        let after_dollar = &remaining[dollar_pos + 1..];
        if after_dollar.starts_with('{') {
            if let Some(close) = after_dollar.find('}') {
                let var_name = after_dollar[1..close].to_string();
                parts.push(Expression::Var {
                    name: var_name,
                    meta: ctx.meta_at(0),
                });
                remaining = &after_dollar[close + 1..];
            } else {
                parts.push(Expression::StringLiteral {
                    value: remaining[dollar_pos..].to_string(),
                    meta: ctx.meta_at(0),
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
                    meta: ctx.meta_at(0),
                });
                remaining = &after_dollar[name_len..];
            } else {
                parts.push(Expression::StringLiteral {
                    value: remaining[dollar_pos..].to_string(),
                    meta: ctx.meta_at(0),
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
            meta: ctx.meta_at(0),
        };
    }
    result
}

fn expr_from_last_stmt(stmts: &[Statement], ctx: &LowerCtx) -> Expression {
    stmts
        .last()
        .map(|s| expr_from_statement(s, ctx))
        .unwrap_or(Expression::BoolLiteral {
            value: true,
            meta: ctx.meta_at(0),
        })
}

fn expr_from_statement(stmt: &Statement, ctx: &LowerCtx) -> Expression {
    match stmt {
        Statement::ExprStmt { expr, .. } => expr.clone(),
        Statement::VarDecl { name, value, .. } => Expression::BinaryOp {
            operator: "=".to_string(),
            left: Box::new(Expression::Var {
                name: name.clone(),
                meta: ctx.meta_at(0),
            }),
            right: Box::new(value.clone()),
            meta: ctx.meta_at(0),
        },
        _ => Expression::BoolLiteral {
            value: true,
            meta: ctx.meta_at(0),
        },
    }
}

fn wrap_if(
    cond: Expression,
    body: Vec<Statement>,
    else_body: Option<Vec<Statement>>,
    ctx: &LowerCtx) -> Vec<Statement> {
    vec![Statement::If {
        condition: cond,
        then_body: body,
        else_body,
        meta: ctx.meta_at(0),
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
