use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use crush_cast::{Expression, Function, Program, Statement};
use crush_frontend::compiler::Compiler;
use crush_frontend::optimizer::Optimizer;
use crush_frontend::parser::Parser;
use crush_frontend::semantics::SemanticAnalyzer;
use crush_vm::{PortableVm, Quotas};

use crate::compile;

pub struct ReplConfig {
    pub quotas: Quotas,
    pub stdlib: bool,
}

impl Default for ReplConfig {
    fn default() -> Self {
        Self {
            quotas: Quotas {
                max_steps: 100000,
                max_stack: 1024,
                max_output: 65536,
                max_call_depth: 64,
                ..Default::default()
            },
            stdlib: false,
        }
    }
}

struct ReplState {
    functions: HashMap<String, Function>,
    main_statements: Vec<Statement>,
}

impl ReplState {
    fn new() -> Self {
        Self {
            functions: HashMap::new(),
            main_statements: Vec::new(),
        }
    }

    fn merge_snippet(&mut self, snippet: Program) -> bool {
        let mut had_main_statements = false;
        for (name, func) in snippet.functions {
            if name == "main" {
                if !func.body.is_empty() {
                    had_main_statements = true;
                    merge_main_statements(self, func.body);
                }
            } else {
                self.functions.insert(name, func);
            }
        }
        had_main_statements
    }

    fn to_program(&self) -> Program {
        let mut functions = self.functions.clone();
        functions.insert(
            "main".to_string(),
            Function {
                params: Vec::new(),
                body: self.main_statements.clone(),
                meta: HashMap::new(),
            },
        );
        Program {
            cast_version: "1.0.0".to_string(),
            entry: "main".to_string(),
            lang: Some("crush".to_string()),
            functions,
            ai_meta: None,
        }
    }

    /// Returns (Program, bool) where bool is true if the last expression was
    /// converted to a Return (meaning the result value is meaningful).
    fn to_exec_program(&self) -> (Program, bool) {
        let mut program = self.to_program();
        let mut converted = false;
<<<<<<< HEAD
        if let Some(main) = program.functions.get_mut("main") {
            if let Some(Statement::ExprStmt { expr, meta }) = main.body.last().cloned() {
                let returns_value = match &expr {
                    Expression::CapabilityCall { name, .. } => crush_vm::capabilities()
                        .get(name.as_str())
                        .map(|spec| spec.returns)
                        .unwrap_or(true),
                    _ => true,
                };
                if returns_value {
                    main.body.pop();
                    main.body.push(Statement::Return {
                        value: Some(expr),
                        meta,
                    });
                    converted = true;
                }
=======
        if let Some(main) = program.functions.get_mut("main")
            && let Some(Statement::ExprStmt { expr, meta }) = main.body.last().cloned()
        {
            let returns_value = match &expr {
                Expression::CapabilityCall { name, .. } => crush_vm::capabilities()
                    .get(name.as_str())
                    .map(|spec| spec.returns)
                    .unwrap_or(true),
                _ => true,
            };
            if returns_value {
                main.body.pop();
                main.body.push(Statement::Return {
                    value: Some(expr),
                    meta,
                });
                converted = true;
>>>>>>> main
            }
        }
        (program, converted)
    }
}

fn merge_main_statements(state: &mut ReplState, mut statements: Vec<Statement>) {
    for stmt in statements.drain(..) {
        if let Statement::VarDecl { name, .. } = &stmt {
            state.main_statements.retain(
                |existing| !matches!(existing, Statement::VarDecl { name: old, .. } if old == name),
            );
        }
        state.main_statements.push(stmt);
    }
}

fn is_input_complete(source: &str) -> bool {
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for ch in source.chars() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            _ => {}
        }
    }

    if in_string || paren_depth > 0 || brace_depth > 0 || bracket_depth > 0 {
        return false;
    }

    let trimmed = source.trim_end();
    let trailing_ops = [
        "+", "-", "*", "/", "=", "&&", "||", "|>", "==", "!=", "<", ">", "<=", ">=",
    ];

    !trailing_ops.iter().any(|op| trimmed.ends_with(op))
}

fn parse_repl_source(source: &str) -> anyhow::Result<Program> {
    Parser::parse(source).map_err(|errors| {
        let msg = errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" | ");
        anyhow::anyhow!("parse error: {}", msg)
    })
}

fn parse_single_expr(source: &str) -> anyhow::Result<Expression> {
    let program = parse_repl_source(source)?;
    let main = program
        .functions
        .get("main")
        .ok_or_else(|| anyhow::anyhow!("expected an expression"))?;
    if main.body.len() != 1 {
        return Err(anyhow::anyhow!("expected a single expression"));
    }
    match &main.body[0] {
        Statement::ExprStmt { expr, .. } => Ok(expr.clone()),
        _ => Err(anyhow::anyhow!("expected an expression")),
    }
}

fn compile_pipeline(program: Program) -> anyhow::Result<casm::Program> {
    let mut analyzer = SemanticAnalyzer::new();
    analyzer.check(&program)?;
    let mut program = program;
    Optimizer::optimize(&mut program);
    let mut compiler = Compiler::new();
    compiler.compile(program)
}

fn print_help() {
    println!(".help               Show this help");
    println!(".quit               Exit the REPL");
    println!(".type <expr>        Show inferred expression type");
    println!(".ast <expr>         Print parsed expression AST");
    println!(".casm <expr>        Print CASM instructions for expression");
    println!(".caps               Show available capabilities");
    println!(".clear              Clear the screen");
}

fn is_meta_command(input: &str) -> bool {
    input.starts_with('.')
}

fn handle_meta_command(input: &str, state: &mut ReplState) -> anyhow::Result<bool> {
    let trimmed = input.trim();
    match trimmed {
        ".help" => {
            print_help();
            return Ok(false);
        }
        ".quit" | ".exit" => return Ok(true),
        ".caps" => {
            println!("Built-in: io.print, str.concat, str.len");
            #[cfg(feature = "stdlib")]
            println!("Stdlib: str.*, math.*, conv.*, collections.*, json.*, path.*, regex.*");
            return Ok(false);
        }
        ".clear" => {
            print!("\x1B[2J\x1B[1;1H");
            io::stdout().flush()?;
            return Ok(false);
        }
        _ => {}
    }

    if let Some(expr_src) = trimmed.strip_prefix(".type ") {
        let expr = parse_single_expr(expr_src.trim())?;
        let program = state.to_program();
        let mut analyzer = SemanticAnalyzer::new();
        let ty = analyzer.infer_expression_type(&program, &expr)?;
        println!("{}", ty);
        return Ok(false);
    }

    if let Some(expr_src) = trimmed.strip_prefix(".ast ") {
        let expr = parse_single_expr(expr_src.trim())?;
        println!("{:#?}", expr);
        return Ok(false);
    }

    if let Some(expr_src) = trimmed.strip_prefix(".casm ") {
        let expr = parse_single_expr(expr_src.trim())?;
        let mut preview = state.to_program();
        preview
            .functions
            .entry("main".to_string())
            .or_insert(Function {
                params: Vec::new(),
                body: Vec::new(),
                meta: HashMap::new(),
            })
            .body
            .push(Statement::ExprStmt {
                expr,
                meta: HashMap::new(),
            });

        let casm = compile_pipeline(preview)?;
        if let Some(main) = casm.functions.get("main") {
            for (i, instr) in main.body.iter().enumerate() {
                println!("{:04} {:<16} {}", i, instr.op, instr.args);
            }
        }
        return Ok(false);
    }

    Err(anyhow::anyhow!("unknown command: {}", input))
}

fn evaluate_input(source: &str, state: &mut ReplState, config: &ReplConfig) -> anyhow::Result<()> {
    let snippet = parse_repl_source(source)?;
    let mut defined: Vec<String> = snippet
        .functions
        .keys()
        .filter(|k| *k != "main")
        .cloned()
        .collect();
    let had_main_statements = state.merge_snippet(snippet);
    let (compiled_program, show_result) = state.to_exec_program();
    let casm = compile_pipeline(compiled_program)?;
    let vm_program = compile::casm_to_vm(&casm)?;

    if had_main_statements {
        let mut vm = PortableVm::new(vm_program);
        vm.set_quotas(config.quotas.clone());
        #[cfg(feature = "stdlib")]
        if config.stdlib {
            let mut builder = crate::HostCapsBuilder::new();
            builder = builder.stdlib(true);
            vm.set_host_caps(builder.build());
        }
        match vm.run() {
            Ok(result) => {
                if !result.output.is_empty() {
                    print!("{}", result.output);
                }
                if show_result && !result.stack.is_empty() {
                    let top = crush_vm::value_to_text(result.stack.last().unwrap());
                    eprintln!("=> {}", top);
                }
                if !result.halted {
                    eprintln!("[fell off end]");
                }
            }
            Err(e) => {
                eprintln!("runtime error: {}", e);
            }
        }
    }

    if !defined.is_empty() {
        defined.sort_unstable();
        println!("defined: {}", defined.join(", "));
    }

    Ok(())
}

pub fn run(config: ReplConfig) -> anyhow::Result<()> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut state = ReplState::new();
    let mut pending = String::new();

    println!("crush-repl — incremental Crush language REPL");
    println!("Type .help for commands, .quit to exit.");

    loop {
        if pending.is_empty() {
            print!("crush> ");
        } else {
            print!("...> ");
        }
        io::stdout().flush()?;

        let mut line = String::new();
        let read = input.read_line(&mut line)?;
        if read == 0 {
            break;
        }
        let line = line.trim_end_matches(['\r', '\n']).to_string();
        let trimmed = line.trim();

        if pending.is_empty() && is_meta_command(trimmed) {
            match handle_meta_command(trimmed, &mut state) {
                Ok(true) => break,
                Ok(false) => {}
                Err(err) => eprintln!("{}", err),
            }
            continue;
        }

        if !pending.is_empty() && trimmed.is_empty() {
            pending.clear();
            continue;
        }

        if !pending.is_empty() {
            pending.push('\n');
        }
        pending.push_str(&line);

        if !is_input_complete(&pending) {
            continue;
        }

        if pending.trim().is_empty() {
            pending.clear();
            continue;
        }

        let source = std::mem::take(&mut pending);
        if let Err(err) = evaluate_input(&source, &mut state, &config) {
            eprintln!("{}", err);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_input_complete_basic() {
        assert!(is_input_complete("1 + 2"));
        assert!(!is_input_complete("1 +"));
        assert!(!is_input_complete("fn main() {"));
    }

    #[test]
    fn test_is_input_complete_balanced() {
        assert!(!is_input_complete("if true {"));
        assert!(is_input_complete("if true { io.print(1) }"));
        assert!(!is_input_complete("let x = [1, 2"));
    }

    #[test]
    fn test_parse_repl_source_simple() {
        let source = "io.print(42)";
        let prog = parse_repl_source(source).expect("parse");
        assert!(prog.functions.contains_key("main"));
    }

    #[test]
    fn test_merge_snippet_non_main_function() {
        let mut state = ReplState::new();
        let source = "fn helper() { io.print(1) }";
        let snippet = parse_repl_source(source).expect("parse");
        state.merge_snippet(snippet);
        assert!(state.functions.contains_key("helper"));
        assert!(!state.functions.contains_key("main"));
    }

    #[test]
    fn test_merge_snippet_main_body() {
        let mut state = ReplState::new();
        let source = "io.print(1)";
        let snippet = parse_repl_source(source).expect("parse");
        state.merge_snippet(snippet);
        assert_eq!(state.main_statements.len(), 1);
    }

    #[test]
    fn test_to_program_builds_complete_program() {
        let mut state = ReplState::new();
        let s1 = parse_repl_source("fn helper() { io.print(1) }").expect("parse");
        state.merge_snippet(s1);
        let s2 = parse_repl_source("io.print(2)").expect("parse");
        state.merge_snippet(s2);
        let program = state.to_program();
        assert!(program.functions.contains_key("main"));
        assert!(program.functions.contains_key("helper"));
    }

    #[test]
    fn test_to_exec_program_converts_last_expr() {
        let mut state = ReplState::new();
        let snippet = parse_repl_source("40 + 2").expect("parse");
        state.merge_snippet(snippet);
        let (program, converted) = state.to_exec_program();
        assert!(converted);
        let main = program.functions.get("main").expect("main");
        match main.body.last().expect("last stmt") {
            Statement::Return {
                value: Some(Expression::BinaryOp { .. }),
                ..
            } => {}
            _ => panic!("expected Return with BinaryOp"),
        }
    }

    #[test]
    fn test_to_exec_program_skips_non_returning_cap() {
        let mut state = ReplState::new();
        let snippet = parse_repl_source("io.print(42)").expect("parse");
        state.merge_snippet(snippet);
        let (_, converted) = state.to_exec_program();
        assert!(!converted, "io.print should not be converted to Return");
    }

    #[test]
    fn test_merge_redefinition_replaces_previous_vardecl() {
        let mut state = ReplState::new();
        let s1 = parse_repl_source("let x = 1").expect("parse");
        state.merge_snippet(s1);
        assert_eq!(state.main_statements.len(), 1);
        let s2 = parse_repl_source("let x = 2").expect("parse");
        state.merge_snippet(s2);
        assert_eq!(state.main_statements.len(), 1);
        match &state.main_statements[0] {
            Statement::VarDecl {
                value: Expression::IntLiteral { value: 2, .. },
                ..
            } => {}
            _ => panic!("expected x = 2"),
        }
    }

    #[test]
    fn test_evaluate_expression_incremental() {
        let mut state = ReplState::new();
        let config = ReplConfig::default();
        evaluate_input("let x = 40", &mut state, &config).expect("eval");
        assert_eq!(state.main_statements.len(), 1);
        evaluate_input("let y = 2", &mut state, &config).expect("eval");
        assert_eq!(state.main_statements.len(), 2);
        evaluate_input("io.print(x + y)", &mut state, &config).expect("eval");
        assert_eq!(state.main_statements.len(), 3);
    }

    #[test]
    fn test_parse_single_expr_basic() {
        let expr = parse_single_expr("42").expect("parse");
        assert!(matches!(expr, Expression::IntLiteral { value: 42, .. }));
    }

    #[test]
    fn test_repl_help_and_quit() {
        let mut state = ReplState::new();
        assert!(handle_meta_command(".quit", &mut state).unwrap());
        assert!(!handle_meta_command(".help", &mut state).unwrap());
    }
}
