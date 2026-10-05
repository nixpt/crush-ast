//! #72 / CRUSH-132: an op or capability the AOT backends can't translate is a
//! compile error naming it — not a no-op or a null stub that compiles and
//! then silently does nothing (a dropped `@python` block printed nothing).

fn casm(source: &str) -> casm::Program {
    crush_lang_sdk::compile::compile_crush_to_casm(source).expect("compiles to CASM")
}

const PYTHON_BLOCK: &str =
    "fn main() {\n  let base = 5\n  @python { result = base * 2 }\n  print(\"r=\" + result)\n}\n";

#[test]
fn python_block_is_rejected_by_both_backends() {
    let program = casm(PYTHON_BLOCK);
    for err in [
        crush_aot::codegen::gen_rust_source(&program).unwrap_err(),
        crush_aot::codegen_c::gen_c_source(&program).unwrap_err(),
    ] {
        assert_eq!(err.ops.len(), 1, "{err}");
        assert!(
            err.ops[0].starts_with("`exec_lang` (fn main, instruction "),
            "{err}"
        );
    }
}

#[test]
fn unknown_capability_is_rejected_by_both_backends() {
    let program = casm("fn main() { fs.mkdir(\"x\") }");
    for err in [
        crush_aot::codegen::gen_rust_source(&program).unwrap_err(),
        crush_aot::codegen_c::gen_c_source(&program).unwrap_err(),
    ] {
        assert!(err.to_string().contains("cap_call 'fs.mkdir'"), "{err}");
    }
}

#[test]
fn the_aot_compiler_surfaces_the_error() {
    let err = crush_aot::AotCompiler::new()
        .compile_casm(&casm(PYTHON_BLOCK), "unsupported_ops_test")
        .unwrap_err();
    assert!(
        err.to_string().contains("cannot compile: `exec_lang`"),
        "{err}"
    );
}

#[test]
fn supported_programs_still_compile() {
    let program = casm("fn add(a, b) { return a + b }\nfn main() { print(add(1, 2)) }");
    assert!(crush_aot::codegen::gen_rust_source(&program).is_ok());
    assert!(crush_aot::codegen_c::gen_c_source(&program).is_ok());
}
