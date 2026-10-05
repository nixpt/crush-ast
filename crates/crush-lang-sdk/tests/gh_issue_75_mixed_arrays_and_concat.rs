//! #75 / CRUSH-135: mixed array literals are `array<any>`, and `a + b` on two
//! arrays returns a new array (a's elements then b's) without changing either.

use crush_lang_sdk::{HostCapsBuilder, Runtime};

fn run(source: &str) -> Result<String, String> {
    let program =
        crush_lang_sdk::compile::compile_crush_source(source).map_err(|e| e.to_string())?;
    Runtime::new()
        .with_host_caps(HostCapsBuilder::new().build())
        .run(&program)
        .map(|r| r.output)
        .map_err(|e| e.to_string())
}

/// Was `Array elements must have compatible types`.
#[test]
fn mixed_literal() {
    let src = "fn main() {\n  let a = [\"hello\", 0]\n  print(a[0])\n  print(a[1])\n  print(len(a))\n}\n";
    assert_eq!(run(src).unwrap(), "hello\n0\n2\n");
}

/// Was `Invalid binary op + for array<int>`.
#[test]
fn plus_concatenates_into_a_new_array() {
    let src = r#"
fn main() {
  let a = [1, 2]
  let b = a + [3]
  print(len(a))
  print(len(b))
  print(b[2])
  b[0] = 99
  print(a[0])
  let c = ["x"] + [1, true]
  print(len(c))
  print(c[0])
}
"#;
    assert_eq!(run(src).unwrap(), "2\n3\n3\n1\n3\nx\n");
}

#[test]
fn array_plus_number_is_still_an_error() {
    assert!(run("fn main() {\n  let a = [1]\n  print(a + 1)\n}\n").is_err());
}
