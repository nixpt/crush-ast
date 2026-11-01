//! CRUSH-221: float literals with a decimal exponent (`1.5e10`, `1e-7`,
//! `1E-7`, `2.5e+3`) are one `Float` token and print the same way on every
//! engine that runs a compiled program, and every `crushc --emit` artifact
//! carries the same value. Malformed exponents are a loud lexer error.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use crush_vm::{InputSource, PortableVm, Quotas};

/// (literal, what `print` writes for it). The expected text is the runtime's
/// existing float print rule on every engine: Rust `Display`, with `.0`
/// appended to integral values. That is not CPython's repr (`1e-07`); changing
/// it is a separate, all-floats decision. What this test pins is that the
/// printed text parses back to exactly the literal's value (see `check_text`).
const CASES: &[(&str, &str)] = &[
    ("1e10", "10000000000.0"),
    ("1E10", "10000000000.0"),
    ("1.5e10", "15000000000.0"),
    ("1.5E10", "15000000000.0"),
    ("1e-7", "0.0000001"),
    ("1E-7", "0.0000001"),
    ("2.5e+3", "2500.0"),
    ("2.5E+3", "2500.0"),
    ("1e20", "100000000000000000000.0"),
];

/// The printed line is exactly `expected`, and it reads back as the literal's
/// value, so no engine prints a different number.
fn check_text(engine: &str, literal: &str, expected: &str, printed: &str) {
    assert_eq!(printed, format!("{expected}\n"), "{engine}, `{literal}`");
    let back: f64 = printed.trim().parse().expect("printed text is a float");
    let want: f64 = literal.parse().expect("literal is a float");
    assert_eq!(
        back, want,
        "{engine}: `{literal}` printed as `{printed}` does not round-trip"
    );
}

fn program_text(literal: &str) -> String {
    format!("print({literal})\n")
}

/// Scheduler: the reference engine (`crush_vm::run`).
#[test]
fn scheduler_prints_exponent_floats() {
    for (literal, expected) in CASES {
        let program = crush_lang_sdk::compile::compile_crush_source(&program_text(literal))
            .unwrap_or_else(|e| panic!("`{literal}`: compile: {e}"));
        let result = crush_vm::run(&program, &Quotas::default())
            .unwrap_or_else(|e| panic!("`{literal}`: scheduler: {e}"));
        check_text("scheduler", literal, expected, &result.output);
    }
}

/// PortableVm: stepped the way crush-web's `Session` steps it.
#[test]
fn portable_vm_prints_exponent_floats() {
    for (literal, expected) in CASES {
        let program = crush_lang_sdk::compile::compile_crush_source(&program_text(literal))
            .unwrap_or_else(|e| panic!("`{literal}`: compile: {e}"));
        let mut vm = PortableVm::new(program);
        vm.set_input(InputSource::supplied(""));
        loop {
            if vm.is_halted() {
                break;
            }
            match vm.step() {
                Ok(None) => {}
                Ok(Some(y)) => panic!("`{literal}`: unexpected pause {y:?}"),
                Err(e) => panic!("`{literal}`: PortableVm: {e}"),
            }
        }
        check_text("PortableVm", literal, expected, &vm.take_output());
    }
}

/// `io.print` for FastVM: the same text `crush_vm::io_print` produces, sent
/// to a buffer the test can read.
#[cfg(feature = "native-plugins")]
mod fastvm {
    use super::*;
    use crush_vm::fastvm::{lower_program, Capability, FastVM, FastYield, Hal};
    use crush_vm::memory::Arena;
    use crush_vm::RuntimeValue;

    #[derive(Debug)]
    struct NoHal;
    impl Hal for NoHal {}

    #[derive(Debug)]
    struct Print(Arc<Mutex<String>>);
    impl Capability for Print {
        fn name(&self) -> &str {
            "io.print"
        }
        fn call(
            &self,
            arena: &mut Arena,
            args: Vec<RuntimeValue>,
            _hal: Arc<dyn Hal>,
        ) -> anyhow::Result<RuntimeValue> {
            let parts: Vec<String> = args
                .iter()
                .map(|v| crush_vm::io_print::runtime_value_to_text(v, arena))
                .collect();
            let mut out = self.0.lock().unwrap();
            out.push_str(&crush_vm::io_print::format_io_print_line(&parts));
            Ok(RuntimeValue::Null)
        }
    }

    #[derive(Debug)]
    struct Unsupported(String);
    impl Capability for Unsupported {
        fn name(&self) -> &str {
            &self.0
        }
        fn call(
            &self,
            _arena: &mut Arena,
            _args: Vec<RuntimeValue>,
            _hal: Arc<dyn Hal>,
        ) -> anyhow::Result<RuntimeValue> {
            anyhow::bail!("test program used unsupported capability `{}`", self.0)
        }
    }

    #[test]
    fn fastvm_prints_exponent_floats() {
        for (literal, expected) in CASES {
            let casm = crush_frontend::compile_crush_source(&program_text(literal))
                .unwrap_or_else(|e| panic!("`{literal}`: compile: {e}"));
            let lowered =
                lower_program(&casm).unwrap_or_else(|e| panic!("`{literal}`: lower: {e:?}"));
            let buf = Arc::new(Mutex::new(String::new()));
            let mut caps: Vec<Arc<dyn Capability>> = Vec::new();
            let mut names: Vec<(&String, &u32)> = lowered.symbols.capabilities.iter().collect();
            names.sort_by_key(|(_, idx)| **idx);
            for (name, idx) in names {
                assert_eq!(*idx as usize, caps.len(), "capability indices are dense");
                caps.push(if name == "io.print" {
                    Arc::new(Print(buf.clone()))
                } else {
                    Arc::new(Unsupported(name.clone()))
                });
            }
            let mut vm = FastVM::new(lowered, caps, Arc::new(NoHal));
            let y = vm.run(100_000);
            assert!(
                matches!(y, FastYield::Finished(_)),
                "`{literal}`: FastVM yielded {y:?}"
            );
            check_text("FastVM", literal, expected, &buf.lock().unwrap());
        }
    }
}

fn crush_run(args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_crush-run"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("spawn crush-run");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn crushc(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_crushc"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("spawn crushc")
}

/// Every `crushc --emit` artifact, run by `crush-run`, prints the value; the
/// `ast` view re-compiles to the same value.
#[test]
fn emit_formats_carry_exponent_floats() {
    let dir = tempfile::tempdir().unwrap();
    for (i, (literal, expected)) in CASES.iter().enumerate() {
        let src = dir.path().join(format!("case{i}.crush"));
        std::fs::write(&src, program_text(literal)).unwrap();
        let src_s = src.to_str().unwrap();
        for (emit, ext) in [
            ("vm", "cvm1"),
            ("casm", "casm"),
            ("cvms", "cvms"),
            ("cast", "cast.json"),
        ] {
            let out = dir.path().join(format!("case{i}.{ext}"));
            let out_s = out.to_str().unwrap();
            let c = crushc(&[src_s, "--emit", emit, "-o", out_s]);
            assert!(
                c.status.success(),
                "`{literal}` --emit {emit}: {}",
                String::from_utf8_lossy(&c.stderr)
            );
            let (ok, stdout, stderr) = crush_run(&["run", out_s]);
            assert!(ok, "`{literal}` --emit {emit} run failed: {stderr}");
            assert!(
                stdout.starts_with(&format!("{expected}\n")),
                "`{literal}` --emit {emit}: got {stdout:?}"
            );
        }
        // `ast` is a source view: it must re-lex to the same float.
        let ast = crushc(&[src_s, "--emit", "ast", "-o", "-"]);
        assert!(
            ast.status.success(),
            "`{literal}` --emit ast: {}",
            String::from_utf8_lossy(&ast.stderr)
        );
        let rendered = String::from_utf8(ast.stdout).unwrap();
        let round = dir.path().join(format!("case{i}.round.crush"));
        std::fs::write(&round, &rendered).unwrap();
        let (ok, stdout, stderr) = crush_run(&["run", round.to_str().unwrap()]);
        assert!(
            ok,
            "`{literal}` re-rendered as `{}` does not run: {stderr}",
            rendered.trim_end()
        );
        assert!(
            stdout.starts_with(&format!("{expected}\n")),
            "`{literal}` re-rendered as `{}`: got {stdout:?}",
            rendered.trim_end()
        );
    }
}

/// A malformed exponent fails `crushc` with the InvalidNumber diagnostic
/// that names the literal, and nothing is compiled from it.
#[test]
fn malformed_exponent_is_loud_in_crushc() {
    let dir = tempfile::tempdir().unwrap();
    for literal in ["1e", "1.5e+", "1.e5"] {
        let src = dir.path().join("bad.crush");
        let mut f = std::fs::File::create(&src).unwrap();
        writeln!(f, "print({literal})").unwrap();
        drop(f);
        let out = crushc(&[src.to_str().unwrap(), "-C"]);
        assert!(!out.status.success(), "`{literal}` should not check");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(&format!("invalid number literal `{literal}`")),
            "`{literal}`: diagnostic was {stderr}"
        );
    }
}
