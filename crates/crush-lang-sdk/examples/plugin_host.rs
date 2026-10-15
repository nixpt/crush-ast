//! Load a manually built Rust `cdylib` and grant its named exports to Crush.
//!
//! Build the wrapper first with `cargo build -p crush-plugin-example`, then
//! pass its shared-library path as this example's first argument.

use std::error::Error;

use crush_lang_sdk::{compile::compile_crush_source, HostCaps, Runtime, Value};

fn main() -> Result<(), Box<dyn Error>> {
    let plugin_path = std::env::args()
        .nth(1)
        .ok_or("usage: plugin_host <host-built-plugin-path>")?;

    let mut host_caps = HostCaps::new();
    // The host selects this library. Its wrapper exports only the API it
    // intends to grant; Crush source cannot choose a library path or Cargo args.
    let _loaded_plugin =
        crush_vm::plugin::load_plugin_exports(&plugin_path, &["rust.greet"], &mut host_caps)?;
    assert!(host_caps.get("rust.greet").is_some());
    assert!(host_caps.get("rust.version").is_none());

    let source = r#"
        fn main() {
            let greeting = rust.greet("Crush");
            print(greeting);
            return greeting;
        }
    "#;
    let program = compile_crush_source(source)?;
    let result = Runtime::new().with_host_caps(host_caps).run(&program)?;

    assert_eq!(result.output, "Hello, Crush from Rust Extension!\n");
    assert_eq!(
        result.return_value,
        Some(Value::Str("Hello, Crush from Rust Extension!".to_string()))
    );
    println!(
        "Crush called the Rust wrapper and returned {:?}",
        result.return_value
    );
    Ok(())
}
