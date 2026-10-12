//! #137: `Name { field: expr, ... }` constructs a declared struct.

use crush_lang_sdk::{HostCapsBuilder, Runtime};

fn run(source: &str) -> String {
    let program = crush_lang_sdk::compile::compile_crush_source(source).expect("compiles");
    Runtime::new()
        .with_host_caps(HostCapsBuilder::new().build())
        .run(&program)
        .expect("runs")
        .output
}

fn compile_err(source: &str) -> String {
    format!("{:#}", crush_lang_sdk::compile::compile_crush_source(source).expect_err("should not compile"))
}

/// The issue's repro: was `[type] type error: Undefined variable: Point`.
#[test]
fn struct_literal_and_field_access() {
    let out = run(r#"
struct Point { x: Float, y: Float }
fn main() {
  let p = Point { x: 10.0, y: 20.0 }
  print(p.x)
  print(p.y)
}
"#);
    assert_eq!(out, "10.0\n20.0\n");
}

#[test]
fn shorthand_nesting_arrays_and_assignment() {
    let out = run(r#"
struct Point { x: Int, y: Int }
struct Line { from: Point, to: Point }
fn main() {
  let x = 1
  let y = 2
  let a = Point { x, y }
  let l = Line {
    from: a,
    to: new Point { x: 3, y: 4 },
  }
  print(l.to.x + l.from.y)
  let ps = [Point { x: 5, y: 6 }, a]
  print(ps[0].y)
  a.x = 9
  print(a.x)
  if a.x > 1 { print("block, not a literal") }
}
"#);
    assert_eq!(out, "5\n6\n9\nblock, not a literal\n");
}

#[test]
fn field_errors() {
    let unknown = compile_err("struct P { x: Int }\nfn main() { let p = P { x: 1, z: 2 } }");
    assert!(unknown.contains("no field `z`"), "{unknown}");
    let missing = compile_err("struct P { x: Int, y: Int }\nfn main() { let p = P { x: 1 } }");
    assert!(missing.contains("missing field(s) `y`"), "{missing}");
    let twice = compile_err("struct P { x: Int }\nfn main() { let p = P { x: 1, x: 2 } }");
    assert!(twice.contains("given twice"), "{twice}");
}
