//! #504: what a type annotation does at run time. Nothing: annotations are
//! checked by the compiler (a `let` annotation, a call argument) and never by
//! an engine, so a struct built with a value of the wrong kind runs the same
//! whether the field says `Int` or `Array`. The container annotations
//! (`Array`, `List`, `Set`, `Map`) behave as the scalars do on the scheduler
//! VM, PortableVm and FastVM.

use std::sync::Arc;

use crush_vm::fastvm::{FastVM, FastYield, Hal};
use crush_vm::{InputSource, PortableVm, Quotas};

#[derive(Debug)]
struct NoHal;
impl Hal for NoHal {}

fn casm_of(src: &str) -> casm::Program {
    crush_frontend::compile_crush_source(src).unwrap_or_else(|e| panic!("compile: {e:#}\n{src}"))
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
    let lowered = crush_vm::fastvm::lower_program(p).map_err(|e| format!("{e:?}"))?;
    let cap = Arc::new(crush_vm::io_print::PrintCap::new());
    let caps: Vec<Arc<dyn crush_vm::fastvm::Capability>> = vec![cap.clone()];
    match FastVM::new(lowered, caps, Arc::new(NoHal)).run(1_000_000) {
        FastYield::Finished(_) => Ok(cap.output()),
        other => Err(format!("{other:?}")),
    }
}

fn everywhere(src: &str) -> Vec<(&'static str, Result<String, String>)> {
    let p = casm_of(src);
    vec![
        ("scheduler VM", on_scheduler(&p)),
        ("PortableVm", on_portable(&p)),
        ("FastVM", on_fastvm(&p)),
    ]
}

fn assert_all(src: &str, want: &str) {
    for (engine, got) in everywhere(src) {
        assert_eq!(got.as_deref(), Ok(want), "{engine}\n{src}");
    }
}

#[test]
fn a_container_field_takes_an_array_on_every_engine() {
    for name in ["Array", "List", "Set", "Map"] {
        assert_all(
            &format!(
                "struct H {{ cells: {name} }}\nfn main() {{ let h = H {{ cells: [1, 2, 3] }}\n print(len(h.cells)) }}\n"
            ),
            "3\n",
        );
    }
}

#[test]
fn a_field_annotation_is_not_checked_at_run_time_scalar_or_container() {
    // The wrong kind of value in a field: the same on every engine, for a
    // scalar annotation and for each container annotation.
    assert_all(
        "struct P { x: Int }\nfn main() { let p = P { x: \"a\" }\n print(p.x) }\n",
        "a\n",
    );
    for name in ["Array", "List", "Set", "Map"] {
        assert_all(
            &format!(
                "struct H {{ cells: {name} }}\nfn main() {{ let h = H {{ cells: 5 }}\n print(h.cells) }}\n"
            ),
            "5\n",
        );
    }
}

#[test]
fn a_field_set_is_not_checked_at_run_time_either() {
    assert_all(
        "struct H { cells: Array }\nfn main() { let h = H { cells: [1] }\n h.cells = 7\n print(h.cells) }\n",
        "7\n",
    );
}

#[test]
fn a_wrong_call_argument_is_refused_by_the_compiler_before_any_engine() {
    let err = crush_frontend::compile_crush_source(
        "fn f(x: Array) { return 1 }\nfn main() { return f(5) }\n",
    )
    .expect_err("a non-array for an Array parameter");
    assert!(
        format!("{err:#}").contains("Argument 0 to 'f' has wrong type"),
        "{err:#}"
    );
}
