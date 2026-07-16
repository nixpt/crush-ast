//! Helper binary for running an AOT-compiled Crush shared library in a subprocess.
//!
//! Generated AOT code calls `std::process::exit(1)` on arithmetic errors, so it cannot be
//! loaded in-process by the differential harness. This binary loads the `.so`, calls
//! `crush_run()`, prints the `RuntimeValue` result, and exits with 0 on success or 1 on
//! failure. The harness can then run this binary as a subprocess and capture its stdout
//! and exit code.
//!
//! Usage: crush-aot-runner <path-to.so>

use std::env;

fn main() {
    let so_path = env::args()
        .nth(1)
        .expect("usage: crush-aot-runner <path-to.so>");

    let module = match crush_aot::Module::load(&so_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("crush-aot-runner: failed to load '{}': {e}", so_path);
            std::process::exit(1);
        }
    };

    let result = match module.call_main() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("crush-aot-runner: call_main failed: {e}");
            std::process::exit(1);
        }
    };

    // Emit a type-tagged string that the differential harness can parse without
    // depending on crush-vm/crush-aot types. Format: "int:42", "float:3.14",
    // "bool:true", "null:null", "str:hello" (str values are raw, no quotes).
    match result {
        crush_vm::RuntimeValue::Int(i) => println!("int:{}", i),
        crush_vm::RuntimeValue::Float(f) => {
            if f.is_finite() && f.fract() == 0.0 {
                println!("float:{:.1}", f);
            } else {
                println!("float:{}", f);
            }
        }
        crush_vm::RuntimeValue::Bool(b) => println!("bool:{}", b),
        crush_vm::RuntimeValue::Null => println!("null:null"),
        crush_vm::RuntimeValue::String(s) => println!("str:{}", s),
        _ => {
            eprintln!("crush-aot-runner: unsupported return value: {:?}", result);
            std::process::exit(1);
        }
    }
}
