// Emits the Way-3 crush→PTX Q6_K GEMV kernel to stdout (for ptxas validation + vendoring).
fn main() {
    let prog = crush_ptx::q6k_gemv_program();
    match crush_ptx::compile_program(&prog) {
        Ok(ptx) => print!("{ptx}"),
        Err(e) => {
            eprintln!("emit error: {e}");
            std::process::exit(1);
        }
    }
}
