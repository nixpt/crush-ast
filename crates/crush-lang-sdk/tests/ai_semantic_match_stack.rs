//! GH #375: `ai_semantic_match(x, "concept")` written in source must declare
//! `stack_args: 1` so the target operand is consumed — otherwise every call
//! leaked one stack slot (2000 iterations -> `stack quota exceeded (1024)`).

use crush_lang_sdk::HostCapsBuilder;
use crush_vm::fastvm::{FastYield, HostRequest};
use crush_vm::portable_vm::PortableVm;
use crush_vm::vm::Quotas;

const SRC: &str = r#"
fn main() {
    let i = 0;
    while i < 2000 {
        let m = ai_semantic_match("hello there", "greeting", 0.5);
        i = i + 1;
    }
    return i;
}
"#;

fn prog() -> (casm::Program, crush_vm::Program) {
    let casm = crush_lang_sdk::compile::compile_crush_to_casm(SRC).expect("compile");
    let vm = crush_lang_sdk::compile::casm_to_vm(&casm).expect("casm_to_vm");
    (casm, vm)
}

#[test]
fn source_call_declares_stack_args() {
    let (casm, _) = prog();
    let instr = casm
        .functions
        .values()
        .flat_map(|f| f.body.iter())
        .find(|i| i.op == "ai_semantic_match")
        .expect("ai_semantic_match instr");
    assert_eq!(instr.args["stack_args"], 1);
}

#[test]
fn scheduler_stack_stays_flat() {
    let (_, vm) = prog();
    let caps = HostCapsBuilder::new().ai_native(true).build();
    let r = crush_vm::run_with_caps(&vm, &Quotas::default(), Some(&caps)).expect("scheduler");
    assert!(r.stack.len() <= 1, "stack leaked: {} entries", r.stack.len());
}

#[test]
fn portable_vm_stack_stays_flat() {
    let (_, vm) = prog();
    let mut pvm = PortableVm::new(vm);
    pvm.set_host_caps(HostCapsBuilder::new().ai_native(true).build());
    let r = pvm.run().expect("portable");
    assert!(r.stack.len() <= 1, "stack leaked: {} entries", r.stack.len());
}

#[test]
fn fastvm_yields_the_stack_arg_payload() {
    let (casm, _) = prog();
    match crush_vm::run_fastvm(&casm) {
        Ok(FastYield::Request(HostRequest::AiSemanticMatch { args })) => {
            assert_eq!(args["stack_args"], 1);
        }
        other => panic!("expected AiSemanticMatch request, got {other:?}"),
    }
}
