//! `crush-walk-run` — Walk → CAST → CASM → CVM1 execution.
use std::path::PathBuf; use std::time::Instant;
use clap::Parser;
use walker_core::AdapterRegistry;

#[derive(Parser)]
#[command(name = "crush-walk-run")]
struct Cli {
    file: PathBuf,
    #[arg(short = 'n', long, default_value = "1")] runs: usize,
    #[arg(long)] dump_cast: bool,
    #[arg(long)] dump_casm: bool,
    #[arg(short = 't', long)] timing: bool,
}

/// Global adapter registry for all 11 language walkers.
fn registry() -> AdapterRegistry {
    let mut r = AdapterRegistry::new();
    r.register(Box::new(crush_lang_python::PythonAdapter))
     .register(Box::new(crush_lang_js::JsAdapter))
     .register(Box::new(crush_lang_rust::RustAdapter))
     .register(Box::new(crush_lang_c::CAdapter))
     .register(Box::new(go_walker::GoAdapter))
     .register(Box::new(zig_walker::ZigAdapter))
     .register(Box::new(wasm_walker::WasmAdapter))
     .register(Box::new(crush_lang_bash::BashAdapter))
     .register(Box::new(crush_lang_zsh::ZshAdapter))
     .register(Box::new(crush_lang_nepali::NepcodeAdapter))
     .register(Box::new(dart_walker::DartAdapter));
    r
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let source = std::fs::read_to_string(&cli.file)?;
    let filename = cli.file.to_string_lossy();

    let t0 = Instant::now();
    let (_, cast_program) = registry().walk(&source, &filename)?;
    let walk_time = t0.elapsed();
    if cli.dump_cast { println!("{}", serde_json::to_string_pretty(&cast_program)?); return Ok(()); }

    let t1 = Instant::now();
    let mut compiler = crush_frontend::compiler::Compiler::new();
    let casm_program = compiler.compile(cast_program)
        .map_err(|e| anyhow::anyhow!("CAST→CASM: {e}"))?;
    let compile_time = t1.elapsed();
    if cli.dump_casm { println!("{}", serde_json::to_string_pretty(&casm_program)?); return Ok(()); }

    // Assemble CASM → CVM1 binary program
    let mut vm_prog = crush_lang_sdk::compile::casm_to_vm(&casm_program)?;

    // Register walked capabilities as host capabilities
    use crush_vm::{HostCap, HostCapSpec, HostCaps};
    let mut host_caps = HostCaps::new();
    struct WalkIoPrint;
    impl HostCap for WalkIoPrint {
        fn spec(&self) -> HostCapSpec { HostCapSpec { name: "io.print".into(), argc: None, returns: false } }
        fn call(&self, args: Vec<crush_vm::vm::Value>) -> Result<Option<crush_vm::vm::Value>, String> {
            for a in &args { print!("{a}"); } println!(); Ok(None)
        }
    }
    struct WalkNop { name: String }
    impl HostCap for WalkNop {
        fn spec(&self) -> HostCapSpec { HostCapSpec { name: self.name.clone(), argc: None, returns: true } }
        fn call(&self, _: Vec<crush_vm::vm::Value>) -> Result<Option<crush_vm::vm::Value>, String> {
            Ok(Some(crush_vm::vm::Value::Null))
        }
    }
    host_caps.register(Box::new(WalkIoPrint));
    for name in &[
        "append", "push", "make_range", "arr_set", "arr_get", "str.concat",
        "__crush_deref__", "__crush_addr_of__", "__crush_unary__",
        "__crush_slice__", "__crush_contains__", "__crush_is__", "__crush_ifexpr__",
        "__crush_setindex__",
    ] {
        host_caps.register(Box::new(WalkNop { name: name.to_string() }));
    }

    let quotas = crush_vm::Quotas { max_steps: 10_000_000, ..Default::default() };
    let mut exec_times = Vec::with_capacity(cli.runs);
    let mut output = String::new();

    for run_i in 0..cli.runs {
        let t2 = Instant::now();
        let result = crush_vm::run_with_caps(&vm_prog, &quotas, Some(&host_caps));
        exec_times.push(t2.elapsed());
        if run_i == 0 {
            output = match result {
                Ok(r) => r.output.trim().to_string(),
                Err(e) => format!("Error: {e}"),
            };
        }
    }

    if cli.runs == 1 { println!("{output}"); }
    if cli.timing {
        let w = walk_time.as_micros() as f64;
        let c = compile_time.as_micros() as f64;
        let e = if exec_times.is_empty() { 0.0 } else { exec_times.iter().map(|d| d.as_micros() as f64).sum::<f64>() / exec_times.len() as f64 };
        eprintln!("=== crush-walk-run ({} runs) ===", cli.runs);
        eprintln!("  Walk    {:>10.1} s", w);
        eprintln!("  Compile {:>10.1} s", c);
        eprintln!("  Execute {:>10.1} s", e);
        eprintln!("  Total   {:>10.1} s", w + c + e);
    }
    Ok(())
}
