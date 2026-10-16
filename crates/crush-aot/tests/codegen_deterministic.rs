//! The generated source is the AOT build cache's key, so the same program must
//! give byte-identical Rust and C every time. `casm::Program::functions` is a
//! `HashMap`, and each map gets its own iteration order, so compiling each
//! example twice (two maps) catches any output that follows hash order.

use std::path::Path;

#[test]
fn generated_source_does_not_depend_on_hash_order() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/crush");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("examples/crush")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "crush"))
        .collect();
    files.sort();
    let (mut compared, mut differ) = (0, Vec::new());
    for path in &files {
        let source = std::fs::read_to_string(path).unwrap();
        let Ok(first) = crush_frontend::compile_crush_source(&source) else { continue };
        let backends: [(&str, fn(&casm::Program) -> Option<String>); 2] = [
            ("rust", |p| crush_aot::codegen::gen_rust_source(p).ok()),
            ("c", |p| crush_aot::codegen_c::gen_c_source(p).ok()),
        ];
        for (backend, generate) in backends {
            let Some(expected) = generate(&first) else { continue };
            compared += 1;
            // Several fresh programs: one more map could repeat the order by chance.
            for _ in 0..4 {
                let again = crush_frontend::compile_crush_source(&source).unwrap();
                if generate(&again).as_deref() != Some(expected.as_str()) {
                    differ.push(format!("{} [{backend}]", path.display()));
                    break;
                }
            }
        }
    }
    assert!(differ.is_empty(), "generated source differs between runs:\n{}", differ.join("\n"));
    assert!(compared >= 36, "only {compared} example sources compared");
}
