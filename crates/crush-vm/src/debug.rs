//! Debugger stop conditions for [`PortableVm`](crate::PortableVm): step
//! into/over/out by call depth, and watchpoints on local slots (CRUSH-159).
//!
//! Everything here is bytecode-level, like `set_breakpoints`: a watchpoint
//! names a local *slot* (CVM1 frames have no globals and the compiled
//! program keeps no slot names), and stepping counts instructions and call
//! depth, not source lines. Mapping names and lines onto these is the
//! debugger front end's job once the frontend emits a source map.
//!
//! The VM reports every stop as `VmYield::DebugBreak` and records the
//! structured reason, readable through `PortableVm::last_stop`.

use crate::vm::Value;

/// How far a requested step runs before the VM pauses again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMode {
    /// Execute one instruction; a `CALL` stops at the callee's first one.
    Into,
    /// Execute one instruction, running any call it makes to completion:
    /// stops at the next instruction whose call depth is at most the
    /// starting depth.
    Over,
    /// Run until the current frame returns (or unwinds): stops at the next
    /// instruction whose call depth is below the starting depth. From the
    /// entry frame this runs to the end.
    Out,
}

/// Which frame's slot a watchpoint observes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchScope {
    /// The frame at this call depth (1 = the entry frame). Only that
    /// activation is watched; the watch is idle while no frame is that deep.
    Frame(usize),
    /// Whichever frame is on top (the running function's locals). A call or
    /// return re-reads the slot without reporting a change.
    Top,
}

/// Handle returned by `PortableVm::add_watchpoint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WatchId(pub u32);

/// Why the VM last returned `VmYield::DebugBreak`.
#[derive(Debug, Clone)]
pub enum DebugStop {
    /// A breakpoint at `ip`; the instruction there has not run yet.
    Breakpoint { ip: usize },
    /// A requested step finished; the instruction at `ip` has not run yet.
    Step { mode: StepMode, ip: usize },
    /// The instruction just executed changed a watched slot. `old` is
    /// `None` when the slot had never been assigned in that frame.
    Watch {
        id: WatchId,
        slot: u16,
        depth: usize,
        old: Option<Value>,
        new: Value,
    },
}

/// A pending step request.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StepRequest {
    pub mode: StepMode,
    /// Call depth when the step was requested.
    pub depth: usize,
    /// At least one instruction has run since the request (a step never
    /// stops where it started).
    pub executed: bool,
}

impl StepRequest {
    /// Whether the VM, about to run an instruction at `depth`, should stop.
    pub fn done_at(&self, depth: usize) -> bool {
        self.executed
            && match self.mode {
                StepMode::Into => true,
                StepMode::Over => depth <= self.depth,
                StepMode::Out => depth < self.depth,
            }
    }
}

/// A registered watchpoint plus the last value it saw.
#[derive(Debug, Clone)]
pub(crate) struct Watch {
    pub id: WatchId,
    pub slot: u16,
    pub scope: WatchScope,
    /// `(depth, snapshot)` from the last check; `None` while the watched
    /// frame doesn't exist. `snapshot` is `None` for an unassigned slot.
    pub seen: Option<(usize, Option<Snapshot>)>,
}

/// A deep copy of a value plus a canonical rendering to compare by.
/// Arrays and maps are shared (`Rc<RefCell<..>>`), so keeping a plain clone
/// would show in-place edits on both sides of the comparison.
#[derive(Debug, Clone)]
pub(crate) struct Snapshot {
    pub value: Value,
    key: String,
}

impl Snapshot {
    pub fn of(v: &Value) -> Self {
        let mut key = String::new();
        canonical(v, &mut key);
        Snapshot {
            value: deep_copy(v),
            key,
        }
    }
}

impl PartialEq for Snapshot {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

/// Copy a value without sharing any of its `Rc` cells.
pub fn deep_copy(v: &Value) -> Value {
    use std::cell::RefCell;
    use std::rc::Rc;
    let vec = |xs: &Vec<Value>| xs.iter().map(deep_copy).collect::<Vec<_>>();
    match v {
        Value::Array(a) => Value::Array(Rc::new(RefCell::new(vec(&a.borrow())))),
        Value::Vector(a) => Value::Vector(Rc::new(RefCell::new(vec(&a.borrow())))),
        Value::Set(a) => Value::Set(Rc::new(RefCell::new(vec(&a.borrow())))),
        Value::List(l) => Value::List(Rc::new(RefCell::new(
            l.borrow().iter().map(deep_copy).collect(),
        ))),
        Value::Map(m) => Value::Map(Rc::new(RefCell::new(
            m.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), deep_copy(v)))
                .collect(),
        ))),
        Value::Tuple(t) => Value::Tuple(vec(t)),
        other => other.clone(),
    }
}

/// Type-exact rendering (`2` and `2.0` differ, unlike `Value`'s `==`) with
/// map keys sorted, so equal contents always render the same.
fn canonical(v: &Value, out: &mut String) {
    use std::fmt::Write;
    let seq = |tag: &str, xs: &mut dyn Iterator<Item = &Value>, out: &mut String| {
        out.push_str(tag);
        out.push('[');
        for x in xs {
            canonical(x, out);
            out.push(',');
        }
        out.push(']');
    };
    match v {
        Value::Array(a) => seq("A", &mut a.borrow().iter(), out),
        Value::Vector(a) => seq("V", &mut a.borrow().iter(), out),
        Value::Set(a) => seq("S", &mut a.borrow().iter(), out),
        Value::List(l) => seq("L", &mut l.borrow().iter(), out),
        Value::Tuple(t) => seq("T", &mut t.iter(), out),
        Value::Map(m) => {
            let m = m.borrow();
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push_str("M{");
            for k in keys {
                let _ = write!(out, "{k:?}:");
                canonical(&m[k], out);
                out.push(',');
            }
            out.push('}');
        }
        other => {
            let _ = write!(out, "{other:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembler::assemble;
    use crate::bytecode::{CALL, LOAD, PUSH, RET, STORE};
    use crate::{PortableVm, VmYield};

    /// main: x = 5; y = inc(1); print(x). inc(n) = n + 10, using its own
    /// slot 0 as scratch.
    const PROG: &str = r#"
        .func main
        PUSH 5
        STORE 0
        PUSH 1
        CALL inc
        STORE 1
        LOAD 0
        CAP_CALL "io.print" 1
        HALT
        .func inc
        STORE 0
        LOAD 0
        PUSH 10
        ADD
        RET
    "#;

    fn vm(src: &str) -> PortableVm {
        PortableVm::new(assemble(src, Some(&["io.print"]), Some("t")).unwrap())
    }

    fn opcode(vm: &PortableVm) -> u8 {
        vm.program().code[vm.current_ip()]
    }

    /// Step plain instructions until the next one is `op`.
    fn run_to(vm: &mut PortableVm, op: u8) {
        while opcode(vm) != op {
            assert!(vm.step().unwrap().is_none());
        }
    }

    /// Request a step and run until the VM pauses or halts.
    fn do_step(vm: &mut PortableVm, mode: StepMode) -> Option<VmYield> {
        vm.request_step(mode);
        loop {
            if vm.is_halted() {
                return None;
            }
            if let Some(y) = vm.step().unwrap() {
                return Some(y);
            }
        }
    }

    #[test]
    fn step_into_a_call_stops_at_the_callees_first_instruction() {
        let mut vm = vm(PROG);
        run_to(&mut vm, CALL);
        assert!(matches!(
            do_step(&mut vm, StepMode::Into),
            Some(VmYield::DebugBreak { .. })
        ));
        assert_eq!((vm.call_depth(), opcode(&vm)), (2, STORE));
        assert!(matches!(
            vm.last_stop(),
            Some(DebugStop::Step {
                mode: StepMode::Into,
                ..
            })
        ));
    }

    #[test]
    fn step_over_a_call_runs_it_and_stops_after_it() {
        let mut vm = vm(PROG);
        run_to(&mut vm, CALL);
        let steps = vm.steps();
        do_step(&mut vm, StepMode::Over).unwrap();
        assert_eq!((vm.call_depth(), opcode(&vm)), (1, STORE));
        // CALL + inc's five instructions ran.
        assert_eq!(vm.steps(), steps + 6);
        assert!(
            matches!(vm.last_stop(), Some(DebugStop::Step { mode: StepMode::Over, ip }) if *ip == vm.current_ip())
        );
        // The pause didn't consume the instruction: resuming finishes normally.
        assert_eq!(vm.run().unwrap().output, "5\n");
    }

    #[test]
    fn step_over_a_plain_instruction_is_one_instruction() {
        let mut vm = vm(PROG);
        assert_eq!(opcode(&vm), PUSH);
        do_step(&mut vm, StepMode::Over).unwrap();
        assert_eq!((vm.steps(), opcode(&vm)), (1, STORE));
    }

    #[test]
    fn step_over_a_return_stops_in_the_caller() {
        let mut vm = vm(PROG);
        run_to(&mut vm, RET);
        do_step(&mut vm, StepMode::Over).unwrap();
        assert_eq!((vm.call_depth(), opcode(&vm)), (1, STORE));
    }

    #[test]
    fn step_out_runs_to_the_callers_next_instruction() {
        let mut vm = vm(PROG);
        run_to(&mut vm, CALL);
        do_step(&mut vm, StepMode::Into).unwrap();
        assert_eq!(vm.call_depth(), 2);
        do_step(&mut vm, StepMode::Out).unwrap();
        assert_eq!((vm.call_depth(), opcode(&vm)), (1, STORE));
        assert_eq!(vm.local(1, 0), Some(&Value::Int(5)));
    }

    #[test]
    fn step_out_of_the_entry_frame_runs_to_the_end() {
        let mut vm = vm(PROG);
        assert!(do_step(&mut vm, StepMode::Out).is_none());
        assert!(vm.is_halted());
        assert_eq!(vm.take_output(), "5\n");
    }

    #[test]
    fn a_breakpoint_inside_a_stepped_over_call_wins_and_ends_the_step() {
        let mut vm = vm(PROG);
        let inc = vm.program().manifest.functions["inc"].entry;
        vm.set_breakpoints(&[inc]);
        run_to(&mut vm, CALL);
        do_step(&mut vm, StepMode::Over).unwrap();
        assert_eq!(vm.current_ip(), inc);
        assert!(matches!(vm.last_stop(), Some(DebugStop::Breakpoint { ip }) if *ip == inc));
        // The step is gone: continuing runs to the end.
        assert_eq!(vm.run().unwrap().output, "5\n");
    }

    #[test]
    fn watch_on_the_entry_frame_reports_its_assignments_only() {
        let mut vm = vm(PROG);
        let id = vm.add_watchpoint(0, WatchScope::Frame(1));
        // First stop: `x = 5`, the slot's first assignment.
        vm.run().unwrap();
        match vm.last_stop() {
            Some(DebugStop::Watch {
                id: w,
                slot: 0,
                depth: 1,
                old: None,
                new,
            }) => {
                assert_eq!((*w, new), (id, &Value::Int(5)));
            }
            other => panic!("expected watch stop, got {other:?}"),
        }
        // The changing STORE already ran.
        assert_eq!(opcode(&vm), PUSH);
        // inc's own slot 0 (depth 2) is a different frame: no more stops.
        let r = vm.run().unwrap();
        assert!(r.halted, "stopped again: {:?}", vm.last_stop());
        assert_eq!(r.output, "5\n");
    }

    #[test]
    fn watch_on_the_top_frame_follows_calls_without_reporting_them() {
        let mut vm = vm(PROG);
        vm.add_watchpoint(0, WatchScope::Top);
        vm.run().unwrap();
        assert!(matches!(
            vm.last_stop(),
            Some(DebugStop::Watch { depth: 1, .. })
        ));
        // Next change: inc's `STORE 0` of its argument (1), in depth 2. The
        // CALL itself (slot 0 reads differently in the new frame) is silent.
        vm.run().unwrap();
        match vm.last_stop() {
            Some(DebugStop::Watch {
                depth: 2,
                old: None,
                new,
                ..
            }) => {
                assert_eq!(new, &Value::Int(1));
            }
            other => panic!("expected watch stop in inc, got {other:?}"),
        }
        assert_eq!(opcode(&vm), LOAD);
        assert!(vm.run().unwrap().halted);
    }

    #[test]
    fn watch_sees_an_in_place_array_edit() {
        let src = r#"
            .func main
            NEW_ARRAY 0
            STORE 0
            LOAD 0
            PUSH 7
            ARR_PUSH
            POP
            HALT
        "#;
        let mut vm = vm(src);
        vm.add_watchpoint(0, WatchScope::Top);
        vm.run().unwrap(); // the STORE
        vm.run().unwrap(); // the ARR_PUSH on the shared array
        match vm.last_stop() {
            Some(DebugStop::Watch {
                old: Some(old),
                new,
                ..
            }) => {
                assert_eq!(old, &Value::new_array(vec![]));
                assert_eq!(new, &Value::new_array(vec![Value::Int(7)]));
            }
            other => panic!("expected in-place watch stop, got {other:?}"),
        }
        assert_eq!(opcode(&vm), crate::bytecode::POP);
        assert!(vm.run().unwrap().halted);
    }

    #[test]
    fn a_watch_hit_ends_a_pending_step() {
        let mut vm = vm(PROG);
        vm.add_watchpoint(0, WatchScope::Frame(1));
        do_step(&mut vm, StepMode::Out).unwrap();
        assert!(matches!(vm.last_stop(), Some(DebugStop::Watch { .. })));
        assert!(vm.run().unwrap().halted);
    }

    #[test]
    fn removed_watchpoints_stop_reporting() {
        let mut vm = vm(PROG);
        let id = vm.add_watchpoint(0, WatchScope::Top);
        assert_eq!(
            vm.watchpoints().collect::<Vec<_>>(),
            vec![(id, 0, WatchScope::Top)]
        );
        assert!(vm.remove_watchpoint(id));
        assert!(!vm.remove_watchpoint(id));
        assert!(vm.run().unwrap().halted);
    }

    #[test]
    fn snapshot_sees_in_place_array_edits() {
        let arr = Value::new_array(vec![Value::Int(1)]);
        let before = Snapshot::of(&arr);
        if let Value::Array(a) = &arr {
            a.borrow_mut().push(Value::Int(2));
        }
        assert!(before != Snapshot::of(&arr));
        // The snapshot kept the old contents.
        assert_eq!(before.value, Value::new_array(vec![Value::Int(1)]));
    }

    #[test]
    fn snapshot_distinguishes_int_from_float() {
        assert!(Snapshot::of(&Value::Int(2)) != Snapshot::of(&Value::Float(2.0)));
        assert!(Snapshot::of(&Value::Int(2)) == Snapshot::of(&Value::Int(2)));
    }
}
