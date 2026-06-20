use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use crush_cast::{Expression, Function, Program, Statement};
use crush_frontend::compiler::Compiler;
use crush_frontend::optimizer::Optimizer;
use crush_frontend::parser::{ParseError, Parser};
use crush_frontend::semantics::SemanticAnalyzer;
use crush_vm::{PortableVm, Quotas};

use crate::cli::MessageFormat;
use crate::compile;
use crate::theme::JsonDiagnostic;

pub struct ReplConfig {
    pub quotas: Quotas,
    pub stdlib: bool,
    /// Diagnostic output mode for per-line errors inside the REPL loop.
    /// `Text` (default) prints themed output to stderr via `theme::render_*`.
    /// `Json` emits one NDJSON record per error via `JsonDiagnostic::*`
    /// so editors / LSP bridges can ingest the stream uniformly with the
    /// other Crush binaries.
    pub message_format: MessageFormat,
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
            message_format: MessageFormat::Text,
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
                ..Default::default()
            },
        );
        Program {
            cast_version: "1.0.0".to_string(),
            entry: "main".to_string(),
            lang: Some("crush".to_string()),
            functions,
            ai_meta: None,
            ..Default::default()
        }
    }

    /// Returns (Program, bool) where bool is true if the last expression was
    /// converted to a Return (meaning the result value is meaningful).
    fn to_exec_program(&self) -> (Program, bool) {
        let mut program = self.to_program();
        let mut converted = false;
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

/// Parse a REPL input line. Returns the typed `Vec<ParseError>` on
/// failure so the caller (the `run` loop) can route both `Text` and
/// `Json` modes against the same data: themed output via
/// `theme::render_parse_errors`, NDJSON records via `JsonDiagnostic::parse_error`
/// + `render_diagnostics_ndjson`.
fn parse_repl_source(source: &str) -> Result<Program, Vec<ParseError>> {
    Parser::parse(source)
}

fn parse_single_expr(source: &str) -> anyhow::Result<Expression> {
    // `parse_repl_source`'s typed `Vec<ParseError>` return doesn't
    // implement `std::error::Error`, so we can't `?` through anyhow's
    // blanket `From<E: Error>`. Re-flatten inline (text-mode-equivalent
    // themed rendering stays consistent with `evaluate_input`'s path).
    // Meta-command errors only happen on `.type` / `.ast` / `.casm`
    // subcommands so this re-flatten is fine — readers see the
    // themed multi-line diagnostic and JSON mode falls back to the
    // blanket `E-IO` via `handle_meta_command`'s outer arm.
    let program = parse_repl_source(source).map_err(|errs| {
        let rendered = crate::theme::render_parse_errors(&errs, None, source);
        anyhow::anyhow!("{rendered}")
    })?;
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
                ..Default::default()
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
    let snippet = parse_repl_source(source).map_err(|errors| {
        // Re-flatten the typed parse-error vector into a `#[error]`-shaped
        // display so `evaluate_input`'s `anyhow::Result<()>` keeps its
        // shape. The REPL loop intercepts this case BEFORE calling
        // `evaluate_input` in JSON mode (see `run`) and renders NDJSON
        // records per `ParseError`. This path is only hit in `Text` mode
        // — theme-rendering is the historical behavior.
        let rendered = crate::theme::render_parse_errors(&errors, None, source);
        anyhow::anyhow!("{rendered}")
    })?;
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
                    eprintln!("{}", crate::theme::format_repl_result(&top));
                }
                if !result.halted {
                    // Bracketed — kept verbatim so users who learned the
                    // original `[fell off end]` notice aren't surprised.
                    eprintln!("[fell off end]");
                }
            }
            Err(e) => {
                match config.message_format {
                    MessageFormat::Text => {
                        eprint!("{}", crate::theme::render_runtime_error(&e));
                    }
                    MessageFormat::Json => {
                        // VM errors are not wrapped in `RuntimeError` here
                        // (the REPL uses `PortableVm` directly, not the
                        // SDK's `Runtime`). The downcast won't match — fall
                        // through to the generic I/O code, but use
                        // `JsonDiagnostic::runtime_error`'s `"E-RT05"`
                        // arm by wrapping into a synthetic `RuntimeError::Vm`
                        // — that way editors can branch on the same code
                        // family they already see from `crush-run`.
                        let diag = JsonDiagnostic::runtime_error(
                            &crate::RuntimeError::Vm(e),
                        );
                        eprint!("{}\n", diag.to_line());
                    }
                }
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
    crate::theme::init_styling();
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut state = ReplState::new();
    let mut pending = String::new();

    println!("crush-repl — incremental Crush language REPL");
    println!("Type .help for commands, .quit to exit.");

    loop {
        if pending.is_empty() {
            print!("{}", crate::theme::format_repl_prompt("crush> "));
        } else {
            print!("{}", crate::theme::format_repl_prompt("...> "));
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
                Err(err) => {
                    match config.message_format {
                        MessageFormat::Text => eprintln!("{}", err),
                        MessageFormat::Json => {
                            // Unknown-command / parse-while-meta errors
                            // don't carry typed `ParseError` / `RuntimeError`
                            // payloads — fall back to a generic I/O code so
                            // the JSON stream stays well-formed.
                            let diag = JsonDiagnostic::generic_error(
                                &err.to_string(),
                                JsonDiagnostic::CODE_IO,
                            );
                            eprint!("{}\n", diag.to_line());
                        }
                    }
                }
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

        // Pre-evaluate parse dispatch — `parse_repl_source` returns a
        // typed `Vec<ParseError>` so we can emit N NDJSON records (one
        // per error) in JSON mode, mirroring `crushc`'s parse-error
        // aggregation. Text mode continues to call `evaluate_input`
        // (which re-flattens the vec for themed rendering).
        if let Err(errors) = parse_repl_source(&source) {
            match config.message_format {
                MessageFormat::Text => {
                    eprint!(
                        "{}",
                        crate::theme::render_parse_errors(&errors, None, &source)
                    );
                }
                MessageFormat::Json => {
                    let diags: Vec<JsonDiagnostic> = errors
                        .iter()
                        .map(|e| JsonDiagnostic::parse_error(e, None))
                        .collect();
                    eprint!("{}", crate::theme::render_diagnostics_ndjson(&diags));
                }
            }
            continue;
        }

        if let Err(err) = evaluate_input(&source, &mut state, &config) {
            // Only non-parse errors reach this arm — semantic / compile /
            // assembly / IO / VM-eval failures. The VM error case is routed
            // inside `evaluate_input` (above) so this arm sees a
            // `theme::render_*`'d string for the legacy themed path or a
            // synthetic-`RuntimeError` rewrap for JSON.
            match config.message_format {
                MessageFormat::Text => eprintln!("{}", err),
                MessageFormat::Json => {
                    // If the underlying anyhow chain still carries a typed
                    // `RuntimeError` (rare — only when callers rewrap the
                    // SDK VM path), surface it; otherwise fall back to the
                    // generic I/O code.
                    let diag = if let Some(runtime_err) =
                        err.downcast_ref::<crate::RuntimeError>()
                    {
                        JsonDiagnostic::runtime_error(runtime_err)
                    } else {
                        JsonDiagnostic::generic_error(
                            &err.to_string(),
                            JsonDiagnostic::CODE_IO,
                        )
                    };
                    eprint!("{}\n", diag.to_line());
                }
            }
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
    fn test_parse_repl_source_returns_typed_vec_on_error() {
        // The signature change (anyhow → Vec<ParseError>) preserves type
        // information so the REPL `run` loop can dispatch per-error in
        // JSON mode. Sanity-check the shape: an unterminated string
        // yields at least one `UnterminatedString` variant.
        let bad = "\"unterminated\n";
        let errs = parse_repl_source(bad).expect_err("expected parse error");
        assert!(
            errs.iter().any(|e| matches!(e, ParseError::UnterminatedString { .. })),
            "expected at least one UnterminatedString error, got {errs:?}"
        );
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
