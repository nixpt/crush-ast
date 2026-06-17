use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("run") => cmd_run(&args[2..]),
        Some("asm") => cmd_asm(&args[2..]),
        Some("dis") => cmd_dis(&args[2..]),
        _ => {
            eprintln!("crush-vm — standalone CVM1 bytecode runtime\n");
            eprintln!("Usage:");
            eprintln!("  crush-vm run  <file.cvm1>         execute a compiled program");
            eprintln!("  crush-vm asm  <file.casm>         assemble CASM text to CVM1");
            eprintln!("  crush-vm dis  <file.cvm1>         disassemble CVM1 to CASM text");
            ExitCode::FAILURE
        }
    }
}

fn cmd_run(args: &[String]) -> ExitCode {
    let path = match args.first() {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("run: expected <file.cvm1>");
            return ExitCode::FAILURE;
        }
    };
    let blob = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("run: cannot read {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let program = match crush_vm::Program::from_blob(&blob) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("run: {e}");
            return ExitCode::FAILURE;
        }
    };
    let quotas = crush_vm::Quotas::default();
    match crush_vm::run(&program, &quotas) {
        Ok(result) => {
            print!("{}", result.output);
            if !result.halted {
                eprintln!("(program fell off end without HALT)");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("run: vm error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_asm(args: &[String]) -> ExitCode {
    let source = if let Some(path) = args.first() {
        match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("asm: cannot read {path}: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        let mut s = String::new();
        if std::io::stdin().read_to_string(&mut s).is_err() {
            eprintln!("asm: cannot read stdin");
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
            eprintln!("asm: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_dis(args: &[String]) -> ExitCode {
    let path = match args.first() {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("dis: expected <file.cvm1>");
            return ExitCode::FAILURE;
        }
    };
    let blob = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("dis: cannot read {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let program = match crush_vm::Program::from_blob(&blob) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("dis: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("{}", crush_vm::disassemble(&program));
    ExitCode::SUCCESS
}
