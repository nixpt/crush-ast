//! CRUSH-272: `polyglot: "lower"` compiles `@python` / `@javascript` blocks
//! into the program through their walkers — no grant, no executor — and a
//! construct the walker can't lower is a compile error, never a fallback.

use crush_web::{execute_with_options, set_host_lang_executor, RunOptions};

fn lower() -> RunOptions {
    RunOptions { polyglot: Some("lower".into()), ..Default::default() }
}

#[cfg(feature = "polyglot-python")]
const PYTHON: &str = r#"
fn main() {
  let base = 5
  let xs = [1, 2]
  @python -> (result, n) {
      def cube(v):
          return v * v * v
      xs.append(3)
      n = len(xs)
      result = cube(base)
  }
  print(result)
  print(n)
  print(len(xs))
}
"#;

#[cfg(feature = "polyglot-python")]
#[test]
fn python_block_lowers_with_no_grant_and_no_executor() {
    set_host_lang_executor(Some(std::rc::Rc::new(|_, _, _| panic!("a lowered block must not reach the executor"))));
    let r = execute_with_options(PYTHON, &lower());
    set_host_lang_executor(None);
    assert!(r.ok, "{:?}", r.error);
    assert_eq!(r.output, "125\n3\n2\n");
}

#[cfg(feature = "polyglot-python")]
#[test]
fn python_block_the_walker_cannot_lower_is_a_compile_error() {
    let src = "fn main() {\n  @python {\n      class A:\n          pass\n  }\n}\n";
    let r = execute_with_options(src, &lower());
    assert!(!r.ok);
    let e = r.error.unwrap_or_default();
    assert!(e.starts_with("compile error: line 2:") && e.contains("class definitions"), "{e}");
    assert!(e.contains("not as CPython"), "{e}");
    assert_eq!(r.output, "");
}

#[cfg(feature = "polyglot-python")]
#[test]
fn exec_stays_the_default() {
    // Same program, no `polyglot` option: an @python block is EXEC_LANG behind
    // its grant, refused before anything runs.
    let r = execute_with_options(PYTHON, &RunOptions::default());
    assert!(!r.ok);
    assert!(r.error.unwrap_or_default().contains("polyglot.python"));
}

#[cfg(feature = "polyglot-javascript")]
#[test]
fn javascript_block_lowers_with_no_grant() {
    let src = r#"
fn main() {
  let n = 4
  @javascript -> total {
    let total = 0;
    for (let i = 1; i <= n; i++) { total += i; }
    console.log("sum", total);
  }
  print(total)
}
"#;
    let r = execute_with_options(src, &lower());
    assert!(r.ok, "{:?}", r.error);
    // What `crush-run --polyglot` prints: the block's output trimmed (#312).
    assert_eq!(r.output, "sum 1010\n");
}

#[cfg(not(feature = "polyglot-python"))]
#[test]
fn lowering_without_the_walker_says_which_feature() {
    let src = "fn main() {\n  @python {\n      print(1)\n  }\n}\n";
    let r = execute_with_options(src, &lower());
    assert!(!r.ok);
    let e = r.error.unwrap_or_default();
    assert!(e.contains("polyglot-python"), "{e}");
}

#[test]
fn an_unknown_polyglot_mode_is_refused() {
    let r = execute_with_options("fn main() {\n  print(1)\n}\n", &RunOptions { polyglot: Some("pyodide".into()), ..Default::default() });
    assert!(!r.ok);
    assert!(r.error.unwrap_or_default().contains("\"exec\" or \"lower\""));
}
