//! Green tasks in the browser runtime (#161). crush-web runs the portable VM,
//! which used to record a `spawn` and run it only when awaited (`yield` was a
//! no-op), so these printed differently from `crush run`.

use crush_web::{RunOptions, execute_with_options};

fn run(src: &str) -> String {
    let result = execute_with_options(src, &RunOptions::default());
    assert!(result.ok, "{:?}", result.error);
    result.output
}

#[test]
fn yield_runs_the_spawned_worker() {
    // The program from #161.
    let src = "fn worker() {\n  print(\"w\")\n}\nfn main() {\n  spawn worker()\n  yield\n  print(\"m\")\n}\n";
    assert_eq!(run(src), "w\nm\n");
}

#[test]
fn interleaved_prints_keep_execution_order() {
    let src = "fn worker() {\n  print(\"w1\")\n  yield\n  print(\"w2\")\n}\nfn main() {\n  spawn worker()\n  yield\n  print(\"m1\")\n  yield\n  print(\"m2\")\n}\n";
    assert_eq!(run(src), "w1\nm1\nw2\nm2\n");
}

#[test]
fn await_after_the_task_finished() {
    let src = "fn work() {\n  return 42\n}\nfn main() {\n  let h = spawn work()\n  yield\n  let r = await h\n  print(r)\n  print(\"after\")\n}\n";
    assert_eq!(run(src), "42\nafter\n");
}
