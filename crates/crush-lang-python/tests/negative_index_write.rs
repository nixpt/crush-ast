//! #442: `a[-1] = v` and `a[-1] += v` were silently dropped while reading
//! `a[-1]` worked, so this printed `[1, 2, 3] 3` instead of CPython's
//! `[1, 2, 13] 13`. A write out of range is now an IndexError-like failure,
//! as in Python, instead of a no-op.

use crush_vm::{InputSource, PortableVm, Quotas};

/// `source` compiled through both front doors (`compile_cast` and the
/// walker's `Compiler`), as VM programs.
fn programs(source: &str) -> Vec<(&'static str, crush_vm::Program)> {
    let cast = crush_lang_python::python_to_cast(source).expect("python to cast");
    let checked = crush_frontend::compile_cast(&cast).expect("compile_cast");
    let walker = crush_frontend::compiler::Compiler::new()
        .compile(cast)
        .expect("Compiler::compile");
    [("compile_cast", checked), ("Compiler", walker)]
        .into_iter()
        .map(|(door, casm)| {
            (
                door,
                crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm to vm"),
            )
        })
        .collect()
}

fn on_scheduler(program: &crush_vm::Program) -> Result<String, String> {
    crush_vm::run(program, &Quotas::default())
        .map(|r| r.output)
        .map_err(|e| e.to_string())
}

fn on_portable(program: &crush_vm::Program) -> Result<String, String> {
    let mut vm = PortableVm::new(program.clone());
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

/// Every (front door, VM) pair's result for `source`.
fn everywhere(source: &str) -> Vec<(String, Result<String, String>)> {
    let mut out = Vec::new();
    for (door, program) in programs(source) {
        out.push((format!("{door} / scheduler"), on_scheduler(&program)));
        out.push((format!("{door} / PortableVm"), on_portable(&program)));
    }
    out
}

#[test]
fn negative_index_writes_land() {
    let source = "a = [1, 2, 3]\na[-1] = 99\na[-1] += 10\na[-3] *= 5\nprint(a, a[-1])\n";
    for (run, got) in everywhere(source) {
        assert_eq!(got.as_deref(), Ok("[5, 2, 109] 109\n"), "{run}");
    }
}

#[test]
fn a_write_out_of_range_fails() {
    for write in ["a[3] = 0", "a[-4] = 0", "a[-4] += 1"] {
        let source = format!("a = [1, 2, 3]\n{write}\nprint(a)\n");
        for (run, got) in everywhere(&source) {
            let e = got.expect_err(&format!("{run}: `{write}` must fail"));
            assert!(e.contains("out of range"), "{run}: `{write}`: {e}");
        }
    }
}
