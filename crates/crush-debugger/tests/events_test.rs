//! CRUSH-160: a host drives a `DebugSession` programmatically and reads
//! `DebugEvent`s, at each visibility level its grants allow.

use crush_debugger::{
    CollectingSink, DebugEvent, DebugSession, PortableVmDriver, StopReason, parse_command,
};
use crush_vm::{DebugVisibility, HostCaps, PortableVm, ValueView};

fn vm(level: DebugVisibility) -> PortableVm {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/calls.crush"
    ))
    .unwrap();
    let program = crush_vm::assemble(&src, Some(&["io.print"]), Some("calls.crush")).unwrap();
    let mut vm = PortableVm::new(program);
    let mut caps = HostCaps::new();
    caps.grant_debug(level);
    vm.set_host_caps(caps);
    vm
}

/// Run REPL lines through the session; return the events.
fn events_for(level: DebugVisibility, lines: &[&str]) -> Vec<DebugEvent> {
    let mut vm = vm(level);
    let mut session = DebugSession::new(PortableVmDriver::new(&mut vm), Vec::new());
    assert_eq!(session.visibility(), level);
    let sink = CollectingSink::new();
    session.set_event_sink(sink.clone());
    for line in lines {
        session
            .handle_command(parse_command(line).unwrap())
            .unwrap();
    }
    sink.events()
}

#[test]
fn full_visibility_reports_stops_with_frames_output_and_finish() {
    let events = events_for(
        DebugVisibility::Full,
        &["watch 0", "continue", "next", "step", "continue"],
    );
    // 1. the watch: main's slot 0 first assigned 5.
    match &events[0] {
        DebugEvent::Stopped {
            reason:
                StopReason::Watchpoint {
                    slot: 0,
                    depth: 1,
                    old: None,
                    new,
                    ..
                },
            frames,
        } => {
            assert_eq!(new.to_string(), "5");
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0].function.as_deref(), Some("main"));
            assert_eq!(frames[0].locals.len(), 1);
        }
        other => panic!("expected watch stop, got {other:?}"),
    }
    // 2. `next` to the CALL: still one frame.
    assert!(
        matches!(&events[1], DebugEvent::Stopped { reason: StopReason::Step, frames } if frames.len() == 1),
        "{:?}",
        events[1]
    );
    // 3. `step` into inc: two frames, innermost first.
    match &events[2] {
        DebugEvent::Stopped { frames, .. } => {
            let names: Vec<_> = frames.iter().map(|f| f.function.as_deref()).collect();
            assert_eq!(names, [Some("inc"), Some("main")]);
            assert_eq!(frames[0].depth, 2);
        }
        other => panic!("expected stop inside inc, got {other:?}"),
    }
    // 4-5. continue: the program's output, then the end.
    assert_eq!(
        &events[3..],
        &[DebugEvent::Output("11\n".into()), DebugEvent::Finished]
    );
}

/// Under the redacted grant no event carries a value's content.
#[test]
fn redacted_visibility_never_emits_plain_values() {
    let events = events_for(
        DebugVisibility::InspectRedacted,
        &["watch 0", "continue", "step", "step", "step", "step", "step"],
    );
    assert!(!events.is_empty());
    let mut views = 0;
    for e in &events {
        if let DebugEvent::Stopped { reason, frames } = e {
            if let StopReason::Watchpoint { new, .. } = reason {
                assert!(matches!(new, ValueView::Redacted { type_name: "int", .. }), "{new:?}");
                views += 1;
            }
            for (_, v) in frames.iter().flat_map(|f| &f.locals) {
                assert!(matches!(v, ValueView::Redacted { .. }), "{v:?}");
                views += 1;
            }
        }
    }
    assert!(views > 3, "too few values to prove anything: {events:?}");
}

#[test]
fn control_only_visibility_hides_every_value() {
    let events = events_for(DebugVisibility::ControlOnly, &["watch 0", "continue", "next"]);
    for e in &events {
        if let DebugEvent::Stopped { reason, frames } = e {
            if let StopReason::Watchpoint { new, .. } = reason {
                assert_eq!(new, &ValueView::Hidden);
            }
            assert!(frames.iter().flat_map(|f| &f.locals).all(|(_, v)| *v == ValueView::Hidden));
        }
    }
}

#[test]
fn no_grant_emits_only_refusals() {
    let events = events_for(DebugVisibility::None, &["continue", "watch 0", "print 0"]);
    assert_eq!(
        events,
        [
            DebugEvent::Refused {
                operation: "continue",
                needs: "debug.step"
            },
            DebugEvent::Refused {
                operation: "watch",
                needs: "debug.step"
            },
            DebugEvent::Refused {
                operation: "print",
                needs: "debug.inspect.redacted"
            },
        ]
    );
}

#[test]
fn a_channel_carries_events_to_another_thread() {
    let mut vm = vm(DebugVisibility::Full);
    let mut session = DebugSession::new(PortableVmDriver::new(&mut vm), Vec::new());
    let (tx, rx) = std::sync::mpsc::channel();
    session.set_event_sink(tx);
    let reader = std::thread::spawn(move || rx.iter().collect::<Vec<_>>());
    session.handle_command(parse_command("continue").unwrap()).unwrap();
    drop(session);
    assert_eq!(
        reader.join().unwrap(),
        [DebugEvent::Output("11\n".into()), DebugEvent::Finished]
    );
}
