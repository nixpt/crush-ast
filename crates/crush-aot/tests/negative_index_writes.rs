//! #442: `a[-1] = v` (and `a[-1] += v`) was silently dropped while reading
//! `a[-1]` worked. Indexed assignment reaches the `arr_set` capability, which
//! truncated the index to `usize` and skipped any write that fell out of
//! range. Every engine now counts a negative index from the end, the way the
//! read path and the `ARR_SET` opcode already did. On both VMs, FastVM and
//! the JIT, a write out of range or with a non-Int index is an error instead
//! of a no-op (AOT: #447).

use crush_vm::fastvm::{FastVM, FastYield, Hal};
use crush_vm::{InputSource, PortableVm, Quotas, RuntimeValue};

#[derive(Debug)]
struct NoHal;
impl Hal for NoHal {}

/// Writes through negative indices, then reads them back. `a` ends as
/// `[11, 2, 99]`, so `main` returns 11 * 1000 + 99.
const WRITES: &str = "fn main() {
  let a = [1, 2, 3];
  a[-1] = 99;
  a[-3] = a[-3] + 10;
  return a[0] * 1000 + a[-1];
}";
const WANT: i64 = 11_099;

fn casm(source: &str) -> casm::Program {
    crush_frontend::compile_crush_source(source).expect("frontend")
}

fn vm_program(source: &str) -> crush_vm::Program {
    crush_lang_sdk::compile::compile_crush_source(source).expect("compile")
}

/// What `main` prints, on the scheduler VM.
fn on_scheduler(source: &str) -> Result<String, String> {
    crush_vm::run(&vm_program(source), &Quotas::default())
        .map(|r| r.output)
        .map_err(|e| e.to_string())
}

/// What `main` prints, on PortableVm.
fn on_portable(source: &str) -> Result<String, String> {
    let mut vm = PortableVm::new(vm_program(source));
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

fn on_fastvm(source: &str) -> FastYield {
    let lowered = crush_vm::fastvm::lower_program(&casm(source)).expect("lowers");
    FastVM::new(lowered, Vec::new(), std::sync::Arc::new(NoHal)).run(1_000_000)
}

/// `WRITES` with its result printed, for the VMs.
fn printing(source: &str) -> String {
    source.replace(
        "return a[0] * 1000 + a[-1];",
        "print(a); print(a[0] * 1000 + a[-1]);",
    )
}

#[test]
fn vms_write_through_negative_indices() {
    let source = printing(WRITES);
    let want = format!("[11, 2, 99]\n{WANT}\n");
    assert_eq!(
        on_scheduler(&source).as_deref(),
        Ok(want.as_str()),
        "scheduler VM"
    );
    assert_eq!(
        on_portable(&source).as_deref(),
        Ok(want.as_str()),
        "PortableVm"
    );
}

#[test]
fn fastvm_and_jit_write_through_negative_indices() {
    match on_fastvm(WRITES) {
        FastYield::Finished(Some(RuntimeValue::Int(n))) => assert_eq!(n, WANT, "FastVM"),
        other => panic!("FastVM: {other:?}"),
    }
    let lowered = crush_vm::fastvm::lower_program(&casm(WRITES)).expect("lowers");
    let jit = crush_jit::JitEngine::new().expect("JitEngine::new");
    match jit.run(&lowered).expect("jit runs") {
        FastYield::Finished(Some(RuntimeValue::Int(n))) => assert_eq!(n, WANT, "JIT"),
        other => panic!("JIT: {other:?}"),
    }
    // `JitEngine::run` falls back to FastVM when it can't compile; make sure
    // this result really came from compiled code.
    assert_eq!(jit.compilation_count(), 1, "the JIT compiled `main`");
}

#[test]
fn aot_writes_through_negative_indices() {
    let program = casm(WRITES);
    let compiler = crush_aot::AotCompiler::new();
    let so = compiler
        .compile_casm(&program, "neg_index_rust")
        .expect("rust backend");
    let got = crush_aot::Module::load(&so)
        .expect("load")
        .call_main()
        .expect("call_main");
    assert_eq!(got, RuntimeValue::Int(WANT), "AOT rust");
}

/// Out of range on either side, or a non-Int index, is an error on every
/// engine that can report one. It used to be a silent no-op.
#[test]
fn a_write_out_of_range_is_an_error() {
    for (write, want) in [
        ("a[3] = 0;", "array index out of range: 3 (len 3)"),
        ("a[-4] = 0;", "array index out of range: -4 (len 3)"),
        ("a[\"x\"] = 0;", "array index must be int"),
    ] {
        let source = format!("fn main() {{\n  let a = [1, 2, 3];\n  {write}\n  print(a);\n}}");
        for (engine, got) in [
            ("scheduler VM", on_scheduler(&source)),
            ("PortableVm", on_portable(&source)),
        ] {
            let e = got.expect_err(&format!("{engine}: `{write}` must fail"));
            assert!(
                e.contains(want),
                "{engine}: `{write}`: want {want:?} in {e:?}"
            );
        }
        let returning = source.replace("print(a);", "return 0;");
        let fast = on_fastvm(&returning);
        assert!(
            matches!(fast, FastYield::Error(_)),
            "FastVM: `{write}` must fail, got {fast:?}"
        );
        let lowered = crush_vm::fastvm::lower_program(&casm(&returning)).expect("lowers");
        let jit = crush_jit::JitEngine::new().expect("JitEngine::new");
        let got = jit.run(&lowered);
        assert_eq!(jit.compilation_count(), 1, "JIT: `{write}` compiled");
        assert!(
            got.is_err() || matches!(got, Ok(FastYield::Error(_))),
            "JIT: `{write}` must fail, got {got:?}"
        );
    }
}
