//! `crush-vm` — standalone CVM1 bytecode runtime.
//!
//! Subcommands:
//!   run <file.cvm1>     execute a compiled program
//!   asm  <file.casm>    assemble CASM text to CVM1
//!   dis  <file.cvm1>    disassemble CVM1 to CASM text
//!
//! New (4-vm-NDJSON-pass):
//!   Add `--message-format=json` to route error paths to NDJSON records
//!   whose shape mirrors `crush_lang_sdk::theme::JsonDiagnostic` (code,
//!   level, file, line, col, message, hint).  Read failures emit `E-IO`;
//!   assembler errors emit `E-ASM` after downcasting to
//!   `crush_vm::AssemblyError` (parallels the `crush-compile` dispatch).
//!
//! Updated (5-vm-PEER-EXTRACT-pass):
//!   The previously inlined `wants_json` / `json_diag_line` /
//!   `json_string` trio + 5 wire-format lockdown tests were extracted
//!   to the canonical `crush_diagnostics` peer crate (see
//!   `crates/crush-diagnostics/tests/wire_format.rs`). This binary now
//!   imports `diag_line_from` + `wants_json` from the peer crate
//!   instead of carrying its own copy.

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use crush_diagnostics::{diag_line_from, wants_json};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let json_mode = wants_json(&args);
    match args.get(1).map(|s| s.as_str()) {
        Some("run") => cmd_run(&args[2..], json_mode),
        Some("asm") => cmd_asm(&args[2..], json_mode),
        Some("dis") => cmd_dis(&args[2..], json_mode),
        _ => {
            if json_mode {
                eprint!(
                    "{}",
                    diag_line_from(
                        "E-IO",
                        "error",
                        "no subcommand (expected: run | asm | dis)",
                        Some("usage: crush-vm [--message-format=json] <run|asm|dis> ..."),
                        None,
                    )
                );
            } else {
                eprintln!("crush-vm — standalone CVM1 bytecode runtime\n");
                eprintln!("Usage:");
                eprintln!("  crush-vm [--message-format=json] run  <file.cvm1>  execute a compiled program");
                eprintln!("  crush-vm [--message-format=json] asm  <file.casm>  assemble CASM text to CVM1");
                eprintln!("  crush-vm [--message-format=json] dis  <file.cvm1>  disassemble CVM1 to CASM text");
            }
            ExitCode::FAILURE
        }
    }
}

fn cmd_run(args: &[String], json_mode: bool) -> ExitCode {
    let path = match args.first() {
        Some(p) => PathBuf::from(p),
        None => {
            emit_simple_error(json_mode, "E-IO", "run: expected <file.cvm1>");
            return ExitCode::FAILURE;
        }
    };
    let blob = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            let msg = format!("cannot read {}: {e}", path.display());
            eprintln_or_json(json_mode, "E-IO", &msg, Some(&path.display().to_string()));
            return ExitCode::FAILURE;
        }
    };
    let program = match crush_vm::Program::from_blob(&blob) {
        Ok(p) => p,
        Err(e) => {
            let msg = format!("{e}");
            eprintln_or_json(json_mode, "E-IO", &msg, Some(&path.display().to_string()));
            return ExitCode::FAILURE;
        }
    };
    let quotas = crush_vm::Quotas::default();
    match crush_vm::run(&program, &quotas) {
        Ok(result) => {
            print!("{}", result.output);
            if !result.halted {
                emit_simple_warn(json_mode, "E-RT05", "(program fell off end without HALT)");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            let msg = format!("vm error: {e}");
            eprintln_or_json(json_mode, "E-IO", &msg, Some(&path.display().to_string()));
            ExitCode::FAILURE
        }
    }
}

fn cmd_asm(args: &[String], json_mode: bool) -> ExitCode {
    let source = if let Some(path) = args.first() {
        match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("cannot read {path}: {e}");
                eprintln_or_json(json_mode, "E-IO", &msg, Some(path));
                return ExitCode::FAILURE;
            }
        }
    } else {
        let mut s = String::new();
        if std::io::stdin().read_to_string(&mut s).is_err() {
            emit_simple_error(json_mode, "E-IO", "asm: cannot read stdin");
            return ExitCode::FAILURE;
        }
        s
    };
    match crush_vm::assemble(&source, None, None) {
        Ok(program) => {
            let blob = program.to_blob();
            use std::io::Write;
            std::io::stdout().write_all(&blob).ok();
            ExitCode::SUCCESS
        }
        Err(e) => {
            // `crush_vm::assemble()` returns `Result<_, AssemblyError>`
            // directly (NOT anyhow-wrapped like `crush-compile`'s
            // wrapper does). So at this site `e: AssemblyError` and a
            // downcast is unnecessary -- just `e.to_string()` carries
            // the line/column info. Emit `E-ASM` (assembler failure
            // code, parallels `crush-compile`'s dispatch; the unit
            // discriminator is the file path attached below).
            let file = args.first().map(|p| p.as_str());
            eprintln_or_json(json_mode, "E-ASM", &e.to_string(), file);
            ExitCode::FAILURE
        }
    }
}

fn cmd_dis(args: &[String], json_mode: bool) -> ExitCode {
    let path = match args.first() {
        Some(p) => PathBuf::from(p),
        None => {
            emit_simple_error(json_mode, "E-IO", "dis: expected <file.cvm1>");
            return ExitCode::FAILURE;
        }
    };
    let blob = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            let msg = format!("cannot read {}: {e}", path.display());
            eprintln_or_json(json_mode, "E-IO", &msg, Some(&path.display().to_string()));
            return ExitCode::FAILURE;
        }
    };
    let program = match crush_vm::Program::from_blob(&blob) {
        Ok(p) => p,
        Err(e) => {
            let msg = format!("{e}");
            eprintln_or_json(json_mode, "E-IO", &msg, Some(&path.display().to_string()));
            return ExitCode::FAILURE;
        }
    };
    println!("{}", crush_vm::disassemble(&program));
    ExitCode::SUCCESS
}

// ----- emit helpers ---------------------------------------------------------

/// Branches a single error site: text-mode eprintln preserves the
/// historical `crush-vm: ...` faithful prefix; JSON mode emits a
/// `JsonDiagnostic` mirror record with the file attached. Writes to
/// stderr (matches the prior crush-vm standalone convention; the
/// canonical `crush_diagnostics` crate does NOT expose a stream-
/// routing helper, so this binary's local `eprintln_or_json` is the
/// right place to encode the stream choice).
fn eprintln_or_json(
    json_mode: bool,
    code: &str,
    message: &str,
    file: Option<&str>,
) {
    if json_mode {
        eprint!(
            "{}",
            diag_line_from(code, "error", message, None, file)
        );
    } else {
        match file {
            Some(f) => eprintln!("{f}: {message}"),
            None => eprintln!("{message}"),
        }
    }
}

fn emit_simple_error(json_mode: bool, code: &str, msg: &str) {
    if json_mode {
        eprint!("{}", diag_line_from(code, "error", msg, None, None));
    } else {
        eprintln!("{msg}");
    }
}

fn emit_simple_warn(json_mode: bool, code: &str, msg: &str) {
    if json_mode {
        eprint!("{}", diag_line_from(code, "warning", msg, None, None));
    } else {
        eprintln!("{msg}");
    }
}

// ----- tests ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    //! Canonical wire-format lockdown (byte-exact field order,
    //! embedded-quote round-trip, canonical-order assertion,
    //! wants_json flag parsing) lives in
    //! `crates/crush-diagnostics/tests/wire_format.rs`. The tests
    //! here are import-smoke only: they confirm the import path
    //! resolves and that the imported `diag_line_from` /
    //! `wants_json` are reachable from this binary's call sites.

    #[test]
    fn import_smoke_crush_diagnostics_resolves() {
        use crush_diagnostics::{diag_line_from, wants_json};
        let line = diag_line_from("E-IO", "error", "smoke", None, None);
        assert!(
            line.starts_with(r#"{"code":"E-IO","level":"error","file":null,"line":null,"col":null,"message":"smoke","hint":null}"#),
            "imported diag_line_from must emit the canonical seven-field shape (got: {line:?})"
        );
        assert!(wants_json(&[
            "crush-vm".to_string(),
            "asm".to_string(),
            "--message-format=json".to_string(),
        ]));
        assert!(!wants_json(&["crush-vm".to_string(), "asm".to_string()]));
    }
}
