//! pyaot FILE.py {rust|gcc|clang} — Python → (rustpython walker) → CAST → CASM → AOT .so.
//! Prints the .so path; `crush-aot-runner <so>` runs it.
use crush_walker_core::AdapterRegistry;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (file, backend) = (&args[1], args.get(2).map(String::as_str).unwrap_or("rust"));
    let source = std::fs::read_to_string(file)?;
    let mut reg = AdapterRegistry::new();
    reg.register(Box::new(crush_lang_python::PythonAdapter));
    let (_, cast) = reg.walk(&source, file)?;
    let casm = crush_frontend::compiler::Compiler::new()
        .compile(cast)
        .map_err(|e| anyhow::anyhow!("CAST→CASM: {e}"))?;
    if backend == "c-src" {
        println!(
            "{}",
            crush_aot::codegen_c::gen_c_source(&casm).map_err(|e| anyhow::anyhow!("{e:?}"))?
        );
        return Ok(());
    }
    if backend == "casm" {
        for (fname, f) in &casm.functions {
            println!("fn {fname} params={:?} hints={:?}", f.params, f.type_hints);
            for i in &f.body {
                println!("  {} {}", i.op, i.args);
            }
        }
        return Ok(());
    }
    let name = format!(
        "pyaot_{}_{backend}",
        std::path::Path::new(file)
            .file_stem()
            .unwrap()
            .to_string_lossy()
    );
    let aot = crush_aot::AotCompiler::new();
    let so = match backend {
        "rust" => aot.compile_casm(&casm, &name)?,
        cc => aot.compile_c(&casm, &name, cc)?,
    };
    println!("{}", so.display());
    Ok(())
}
