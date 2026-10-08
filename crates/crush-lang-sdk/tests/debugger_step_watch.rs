//! CRUSH-159: step over/out and watchpoints on `PortableVm`, driven against
//! a compiled Crush program (not hand-written bytecode).

use crush_vm::vm::Value;
use crush_vm::{DebugStop, PortableVm, StepMode, VmYield, WatchScope};

const SRC: &str = r#"
fn sum_to(n) {
    let total = 0
    let i = 1
    while i <= n {
        total = total + i
        i = i + 1
    }
    return total
}

fn main() {
    let r = sum_to(4)
    print(r)
}
"#;

fn vm() -> PortableVm {
    PortableVm::new(crush_lang_sdk::compile::compile_crush_source(SRC).unwrap())
}

/// Run until the VM pauses (Some) or halts (None).
fn resume(vm: &mut PortableVm) -> Option<VmYield> {
    loop {
        if vm.is_halted() {
            return None;
        }
        if let Some(y) = vm.step().unwrap() {
            return Some(y);
        }
    }
}

/// Break at `sum_to`'s first instruction.
fn vm_in_sum_to() -> PortableVm {
    let mut vm = vm();
    let entry = vm.program().manifest.functions["sum_to"].entry;
    vm.set_breakpoints(&[entry]);
    assert!(matches!(resume(&mut vm), Some(VmYield::DebugBreak { .. })));
    assert!(matches!(vm.last_stop(), Some(DebugStop::Breakpoint { ip }) if *ip == entry));
    assert_eq!(vm.call_depth(), 2);
    vm
}

#[test]
fn watch_on_a_loop_accumulator_sees_every_value() {
    let mut vm = vm_in_sum_to();
    for slot in 0..4 {
        vm.add_watchpoint(slot, WatchScope::Frame(2));
    }
    let mut changes: Vec<(u16, Value)> = Vec::new();
    while resume(&mut vm).is_some() {
        match vm.last_stop() {
            Some(DebugStop::Watch {
                slot,
                depth: 2,
                new,
                ..
            }) => changes.push((*slot, new.clone())),
            other => panic!("unexpected stop {other:?}"),
        }
    }
    assert_eq!(vm.take_output(), "10\n");
    // `total` is whichever slot was first assigned 0.
    let total_slot = changes
        .iter()
        .find(|(_, v)| *v == Value::Int(0))
        .map(|(s, _)| *s)
        .expect("total = 0 was never seen");
    let totals: Vec<&Value> = changes
        .iter()
        .filter(|(s, _)| *s == total_slot)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(
        totals,
        [0, 1, 3, 6, 10].map(Value::Int).iter().collect::<Vec<_>>(),
        "all changes: {changes:?}"
    );
}

#[test]
fn step_out_of_sum_to_lands_in_main_with_its_result() {
    let mut vm = vm_in_sum_to();
    vm.request_step(StepMode::Out);
    assert!(resume(&mut vm).is_some());
    assert!(matches!(
        vm.last_stop(),
        Some(DebugStop::Step {
            mode: StepMode::Out,
            ..
        })
    ));
    assert_eq!(vm.call_depth(), 1);
    // Nothing printed yet; the rest of main still runs.
    assert_eq!(vm.take_output(), "");
    assert!(resume(&mut vm).is_none());
    assert_eq!(vm.take_output(), "10\n");
}

#[test]
fn stepping_over_main_never_enters_sum_to() {
    let mut vm = vm();
    let mut depths = Vec::new();
    loop {
        vm.request_step(StepMode::Over);
        if resume(&mut vm).is_none() {
            break;
        }
        depths.push(vm.call_depth());
    }
    assert!(!depths.is_empty());
    assert!(depths.iter().all(|&d| d == 1), "entered a call: {depths:?}");
    assert_eq!(vm.take_output(), "10\n");
}

#[test]
fn stepping_into_from_main_does_enter_sum_to() {
    let mut vm = vm();
    let mut max_depth = 0;
    loop {
        vm.request_step(StepMode::Into);
        if resume(&mut vm).is_none() {
            break;
        }
        max_depth = max_depth.max(vm.call_depth());
    }
    assert_eq!(max_depth, 2);
}
