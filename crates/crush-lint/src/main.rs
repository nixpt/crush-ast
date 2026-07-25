use std::path::PathBuf;
use std::process::ExitCode;
use crush_diagnostics::{diag_line_from, wants_json, DiagRecord};
use crush_lint::AiLinter;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let json_mode = wants_json(&args);

    // We expect `crush-lint <file.cr>` or `crush-lint <file.casm>`
    let path = match args.iter().find(|arg| !arg.starts_with("-") && **arg != args[0]) {
        Some(p) => PathBuf::from(p),
        None => {
            if json_mode {
                eprint!("{}", diag_line_from(
                    "E-USAGE", "error", "Missing file argument",
                    Some("Usage: crush-lint [--message-format=json] <file.cr>"), None
                ));
            } else {
                eprintln!("crush-lint — AI-Native Code Linter & Formatter");
                eprintln!("Usage: crush-lint [--message-format=json] <file.cr>");
            }
            return ExitCode::FAILURE;
        }
    };
    
    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("Cannot read {}: {}", path.display(), e);
            if json_mode {
                eprint!("{}", diag_line_from("E-IO", "error", &msg, None, None));
            } else {
                eprintln!("{}", msg);
            }
            return ExitCode::FAILURE;
        }
    };
    
    let linter = AiLinter::new(true);
    let mut has_errors = false;
    
    // Naive mock parser: in a real implementation we would use `casm::parse` or `tree-sitter-crush`.
    // We will just scan for obvious anti-patterns to demonstrate the AI augmentation.
    for (i, line) in source.lines().enumerate() {
        let line_num = (i + 1) as u32;
        
        let error_msg = if line.contains("def ") {
            Some("Unexpected token: 'def'. Did you mean 'fn'?")
        } else if !line.contains(";") && line.contains("let ") {
            // Very naive check for missing semicolon
            Some("Missing semicolon")
        } else {
            None
        };
        
        if let Some(msg) = error_msg {
            has_errors = true;
            let diag = DiagRecord {
                code: "E-SYNTAX",
                level: "error",
                file: Some(path.to_str().unwrap()),
                line: Some(line_num),
                col: None,
                message: msg,
                hint: None,
            };
            
            // Pass the error through the AI Engine!
            let augmented = linter.augment_diagnostic(diag, line);
            
            if json_mode {
                eprint!("{}", crush_diagnostics::diag_line(&augmented));
            } else {
                eprintln!("error[{}]: {} at {}:{}", augmented.code, augmented.message, path.display(), line_num);
                // Hint is printed natively by the linter engine MVP to stderr
            }
        }
    }
    
    if has_errors {
        ExitCode::FAILURE
    } else {
        if !json_mode {
            println!("✅ {} is looking good!", path.display());
        }
        ExitCode::SUCCESS
    }
}
