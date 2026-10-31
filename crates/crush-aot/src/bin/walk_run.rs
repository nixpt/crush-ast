//! `crush-walk-run` — Walk → CAST → CASM → CVM1 execution.
use std::path::PathBuf; use std::time::Instant;
use clap::Parser;
use crush_walker_core::AdapterRegistry;

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
     .register(Box::new(crush_lang_go::GoAdapter))
     .register(Box::new(crush_lang_zig::ZigAdapter))
     .register(Box::new(crush_lang_wasm::WasmAdapter))
     .register(Box::new(crush_lang_bash::BashAdapter))
     .register(Box::new(crush_lang_zsh::ZshAdapter))
     .register(Box::new(crush_lang_nepali::NepcodeAdapter))
     .register(Box::new(crush_lang_dart::DartAdapter));
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
    struct WalkIoWrite;
    impl HostCap for WalkIoWrite {
        fn spec(&self) -> HostCapSpec { HostCapSpec { name: "io.write".into(), argc: None, returns: false } }
        fn call(&self, args: Vec<crush_vm::vm::Value>) -> Result<Option<crush_vm::vm::Value>, String> {
            for a in &args { print!("{a}"); } Ok(None)
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
    host_caps.register(Box::new(WalkIoWrite));
    // Shell programs end with `sys.exit($?)` whenever the last command failed,
    // and `exit N` lowers to it directly (CRUSH-266). Without this the whole
    // run failed with "unknown capability: sys.exit".
    host_caps.register(Box::new(crush_lang_sdk::host_caps::SysExitCap));
    // Pathname expansion (`echo *.txt`, `for f in *`, `echo */*.log`) is the
    // shell's one look at the filesystem: `fs.exists` + `fs.list` +
    // `fs.is_dir` (a walk into `*/*.txt` has to know which matches it may
    // descend into), confined to the directory the script was started in.
    // Hosts that embed walker output can leave them out and a glob is then
    // refused loudly ("unknown capability: fs.list").
    let root = std::env::current_dir()?.display().to_string();
    host_caps.register(Box::new(crush_lang_sdk::host_caps::FsExistsCap::new(&root)));
    host_caps.register(Box::new(crush_lang_sdk::host_caps::FsIsDirCap::new(&root)));
    host_caps.register(Box::new(crush_lang_sdk::host_caps::FsListCap::new(&root)));
    for name in &[
        "append", "push", "make_range", "arr_set", "arr_get", "str.concat",
        "__crush_deref__", "__crush_addr_of__", "__crush_unary__",
        "__crush_setindex__",
    ] {
        host_caps.register(Box::new(WalkNop { name: name.to_string() }));
    }

    let quotas = crush_vm::Quotas { max_steps: 10_000_000, ..Default::default() };
    let mut exec_times = Vec::with_capacity(cli.runs);
    let mut output = String::new();
    let mut exit_code = None;
    let mut failed: Option<String> = None;

    for run_i in 0..cli.runs {
        let t2 = Instant::now();
        // Streaming, so output printed before a `sys.exit` or a runtime error
        // is kept (CRUSH-184).
        let mut run_out = String::new();
        let result = crush_vm::vm::run_with_caps_streaming(&vm_prog, &quotas, Some(&host_caps), &mut |s: &str| {
            run_out.push_str(s)
        });
        exec_times.push(t2.elapsed());
        if run_i == 0 {
            // Program output verbatim — trimming hid trailing spaces and
            // newline-free `io.write` output (C printf, Rust print!).
            output = run_out;
        }
        match result {
            Ok(_) => {}
            // `sys.exit(N)` is a requested stop, not a failure: keep the
            // output and hand the status to the caller.
            Err(crush_vm::VmError::Exit(code)) => {
                exit_code = Some(code);
                break;
            }
            // A runtime error is a failure: reported after the output, and a
            // non-zero exit (CRUSH-184).
            Err(e) => {
                failed = Some(format!("{e}"));
                break;
            }
        }
    }

    if cli.runs == 1 { print!("{output}"); }
    if let Some(code) = exit_code {
        std::process::exit(code);
    }
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
    // A runtime error: reported after the streamed output, and a non-zero exit
    // (CRUSH-184).
    if let Some(e) = failed {
        return Err(anyhow::anyhow!("{e}"));
    }
    Ok(())
}
