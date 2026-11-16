//! #425: bitwise operators (`& | ^ << >> ~`) compile and agree on every
//! engine, under the captain's ruling H1 (2026-11-13):
//! - Int operands only; anything else is a type error.
//! - A shift count outside 0..=63 is ArithmeticOverflow. A left shift that
//!   loses bits is too (checked, like `+` and `*`, per the i64 ruling).
//! - Right shift is arithmetic; full i64 everywhere.
//!
//! Before, the compiler refused them ("Unsupported op: &"), and the engines
//! disagreed underneath:
//! - Floats were truncated by the VMs, refused by FastVM, read as raw bits by
//!   the JIT, and turned into null by AOT.
//! - A count >= 64 or < 0 raised an error on the VMs, panicked or wrapped on
//!   FastVM and AOT Rust, and was UB in AOT C.
//! - The JIT is now refused for these ops and falls back to FastVM.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use crush_vm::fastvm::{FastVM, FastYield, Hal};
use crush_vm::{InputSource, PortableVm, Quotas};

#[derive(Debug)]
struct NoHal;
impl Hal for NoHal {}

fn casm_of(python: &str) -> casm::Program {
    let cast = crush_lang_python::python_to_cast(python).expect("python to cast");
    crush_frontend::compile_cast(&cast).expect("compile_cast")
}

fn on_scheduler(p: &casm::Program) -> Result<String, String> {
    let vm = crush_lang_sdk::compile::casm_to_vm(p).expect("casm to vm");
    crush_vm::run(&vm, &Quotas::default())
        .map(|r| r.output)
        .map_err(|e| e.to_string())
}

fn on_portable(p: &casm::Program) -> Result<String, String> {
    let vm = crush_lang_sdk::compile::casm_to_vm(p).expect("casm to vm");
    let mut vm = PortableVm::new(vm);
    vm.set_input(InputSource::supplied(""));
    while !vm.is_halted() {
        match vm.step() {
            Ok(None) => {}
            Ok(Some(y)) => return Err(format!("unexpected pause {y:?}")),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(vm.take_output())
}

fn on_fastvm(p: &casm::Program) -> Result<String, String> {
    let lowered = crush_vm::fastvm::lower_program(p).expect("lowers");
    let cap = Arc::new(crush_vm::io_print::PrintCap::new());
    let caps: Vec<Arc<dyn crush_vm::fastvm::Capability>> = vec![cap.clone()];
    match FastVM::new(lowered, caps, Arc::new(NoHal)).run(1_000_000) {
        FastYield::Finished(_) => Ok(cap.output()),
        other => Err(format!("{other:?}")),
    }
}

/// The JIT refuses bitwise ops, so `JitEngine` runs the program on FastVM.
/// Returns the output and how many functions the JIT compiled.
fn on_jit(p: &casm::Program) -> (Result<String, String>, u64) {
    let lowered = crush_vm::fastvm::lower_program(p).expect("lowers");
    let cap = Arc::new(crush_vm::io_print::PrintCap::new());
    let jit = crush_jit::JitEngine::new()
        .expect("JitEngine")
        .with_capabilities(vec![cap.clone()]);
    let out = match jit.run(&lowered) {
        Ok(FastYield::Finished(_)) => Ok(cap.output()),
        Ok(other) => Err(format!("{other:?}")),
        Err(e) => Err(e.to_string()),
    };
    (out, jit.compilation_count())
}

fn run_so(so: &Path) -> Result<String, String> {
    let out = Command::new(env!("CARGO_BIN_EXE_crush-aot-runner"))
        .arg(so)
        .output()
        .expect("spawn crush-aot-runner");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).to_string());
    }
    // Drop the type-tagged result line the runner prints last.
    let body = stdout.trim_end_matches('\n');
    Ok(match body.rfind('\n') {
        Some(i) => format!("{}\n", &body[..i]),
        None => String::new(),
    })
}

fn cc_available() -> bool {
    Command::new("gcc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// `(engine, result)` for every engine.
fn everywhere(name: &str, python: &str) -> Vec<(&'static str, Result<String, String>)> {
    let p = casm_of(python);
    let (jit, compiled) = on_jit(&p);
    assert_eq!(compiled, 0, "{name}: the JIT must refuse bitwise ops");
    let compiler = crush_aot::AotCompiler::new();
    let mut out = vec![
        ("scheduler VM", on_scheduler(&p)),
        ("PortableVm", on_portable(&p)),
        ("FastVM", on_fastvm(&p)),
        ("JIT (FastVM fallback)", jit),
        (
            "AOT rust",
            run_so(
                &compiler
                    .compile_casm(&p, &format!("{name}_rust"))
                    .expect("rust backend"),
            ),
        ),
    ];
    if cc_available() {
        let so = compiler
            .compile_c(&p, &format!("{name}_c"), "gcc")
            .expect("c backend");
        out.push(("AOT c", run_so(&so)));
    }
    out
}

#[test]
fn bitwise_operators_agree_on_every_engine() {
    let source = "\
a = 12
b = 10
print(a & b)
print(a | b)
print(a ^ b)
print(~a)
print(a << 3)
print(a >> 2)
print(-17 >> 2)
print(-1 << 62)
print(9223372036854775807 >> 63)
x = 5
x <<= 4
x |= 3
x ^= 1
x &= 255
x >>= 1
print(x)
";
    let want = "8\n14\n6\n-13\n96\n3\n-5\n-4611686018427387904\n0\n41\n";
    for (engine, got) in everywhere("bitwise_ok", source) {
        assert_eq!(got.as_deref(), Ok(want), "{engine}");
    }
}

/// Each case must fail on every engine; the VMs' message must name it.
#[test]
fn bad_bitwise_operands_fail_on_every_engine() {
    for (name, body, vm_message) in [
        ("shl_64", "print(n(1) << n(64))", "overflow"),
        ("shl_neg", "print(n(1) << n(-1))", "overflow"),
        ("shr_64", "print(n(-1) >> n(64))", "overflow"),
        ("shl_loses_bits", "print(n(1) << n(63))", "overflow"),
        ("float_and", "print(n(1.5) & n(1))", "int"),
        ("str_or", "print(n('a') | n(1))", "int"),
        ("float_not", "print(~n(2.0))", "int"),
    ] {
        // `n` hides the operand types from the compile-time check, so the
        // runtime check is what's tested.
        let source = format!("def n(v):\n    return v\n{body}\n");
        for (engine, got) in everywhere(name, &source) {
            let e = got.expect_err(&format!("{engine}: `{body}` must fail"));
            if engine.ends_with("VM") && !engine.starts_with("Fast") {
                assert!(
                    e.to_lowercase().contains(vm_message),
                    "{engine}: `{body}`: want {vm_message:?} in {e:?}"
                );
            }
        }
    }
}

/// With the operand types known, a non-int operand is a compile error.
#[test]
fn a_non_int_literal_operand_is_a_compile_error() {
    for source in ["print(1.5 & 1)\n", "print('a' << 1)\n", "print(~2.0)\n"] {
        let cast = crush_lang_python::python_to_cast(source).expect("python to cast");
        let err =
            crush_frontend::compile_cast(&cast).expect_err(&format!("`{source}` must not compile"));
        let err = format!("{err:#}");
        assert!(
            err.contains("bitwise operators take int operands"),
            "`{source}`: {err}"
        );
    }
}
