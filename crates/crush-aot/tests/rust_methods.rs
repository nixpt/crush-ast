//! Public AOT loader result contracts, not only stdout comparisons.
use crush_aot::{AotCompiler, Module};
use crush_vm::RuntimeValue as V;
#[test]
fn mapped_method_return_types_survive_the_public_loader() {
    let cache = tempfile::tempdir().unwrap();
    let compiler = AotCompiler::new()
        .with_optimize(false)
        .with_cache_dir(cache.path().into());
    let cases = [
        (
            "integer",
            r#"fn main() { return rust.num("pow", "i8", 6, 3); }"#,
            "int",
        ),
        ("float", r#"fn main() { return math.sqrt(4); }"#, "float"),
        (
            "bool",
            r#"fn main() { return str.contains("abc", "a"); }"#,
            "bool",
        ),
        (
            "text_number",
            r#"fn main() { return str.to_lower("42"); }"#,
            "number_string",
        ),
        (
            "text_bool",
            r#"fn main() { return str.to_lower("TRUE"); }"#,
            "bool_string",
        ),
        (
            "text_space",
            r#"fn main() { return str.to_lower(" X "); }"#,
            "space_string",
        ),
        (
            "infinity",
            r#"fn main() { return math.pow(0, -1); }"#,
            "infinity",
        ),
        ("nan", r#"fn main() { return math.pow(-1, 0.5); }"#, "nan"),
    ];
    for (name, source, kind) in cases {
        let p = crush_frontend::compile_crush_source(source).unwrap();
        for c in [false, true] {
            if c && !cfg!(target_os = "linux") {
                continue;
            }
            let path = if c {
                compiler.compile_c(&p, name, "gcc")
            } else {
                compiler.compile_casm(&p, name)
            }
            .unwrap();
            let value = Module::load(&path).unwrap().call_main().unwrap();
            let correct = match (kind, value) {
                ("int", V::Int(-40)) | ("float", V::Float(2.0)) | ("bool", V::Bool(true)) => true,
                ("number_string", V::String(s)) => s == "42",
                ("bool_string", V::String(s)) => s == "true",
                ("space_string", V::String(s)) => s == " x ",
                ("infinity", V::Float(f)) => f == f64::INFINITY,
                ("nan", V::Float(f)) => f.is_nan(),
                _ => false,
            };
            assert!(correct, "wrong {kind} root value on C={c}");
        }
    }
}
#[test]
#[cfg(target_os = "linux")]
fn support_symbols_are_private_and_optimization_changes_cache_identity() {
    let cache = tempfile::tempdir().unwrap();
    let p = crush_frontend::compile_crush_source(r#"fn main() { return str.to_lower("ΟΣ"); }"#)
        .unwrap();
    let compiler = |opt| {
        AotCompiler::new()
            .with_optimize(opt)
            .with_cache_dir(cache.path().into())
    };
    let a = compiler(false).compile_c(&p, "cache_probe", "gcc").unwrap();
    let b = compiler(true).compile_c(&p, "cache_probe", "gcc").unwrap();
    assert_ne!(a, b);
    assert_eq!(
        a,
        compiler(false).compile_c(&p, "cache_probe", "gcc").unwrap()
    );
    for path in [a, b] {
        assert!(
            matches!(Module::load(&path).unwrap().call_main().unwrap(),V::String(s) if s=="ος")
        );
        let symbols = std::process::Command::new("nm")
            .arg("-D")
            .arg(path)
            .output()
            .unwrap();
        assert!(symbols.status.success());
        assert!(!String::from_utf8_lossy(&symbols.stdout).contains("crush_support_"));
    }
}
#[test]
fn nul_capable_host_results_are_refused_before_rust_artifact_generation() {
    for source in [
        "fn main() { return str.to_lower(\"A\0B\"); }",
        "fn main() { return str.to_lower(io.read()); }",
    ] {
        let p = crush_frontend::compile_crush_source(source).unwrap();
        let e = crush_aot::codegen::gen_rust_source(&p)
            .unwrap_err()
            .to_string();
        assert!(e.contains("length-aware host result ABI"), "{e}");
    }
}
