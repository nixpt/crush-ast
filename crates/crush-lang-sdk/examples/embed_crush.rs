//! Compile and run a Crush program from a Rust host.

use crush_lang_sdk::{Runtime, Value, compile::compile_crush_source};

fn main() -> anyhow::Result<()> {
    let program = compile_crush_source(
        r#"
        fn main() {
            print("hello from Crush");
            return 42;
        }
        "#,
    )?;

    let result = Runtime::new().run(&program)?;
    assert_eq!(result.output, "hello from Crush\n");
    let returned = result.return_value.unwrap_or(Value::Null);
    assert_eq!(returned, Value::Int(42));
    println!("Crush printed {:?} and returned {returned} to Rust", result.output);
    Ok(())
}
