//! The entry function every walker's CAST is run from is named `main`
//! (the compiler looks `main` up by name; `Program::entry` is not consulted).
//! A walker that wraps the module's top-level code in `main` must therefore
//! first move a user function that is itself called `main` out of the way,
//! or the module body replaces it (CRUSH-560).

use crush_cast::Function;
use std::collections::HashMap;

/// Name a user-defined `main` is moved to. Not a valid identifier in the
/// source languages that use this, so no user code can collide with it.
pub const USER_MAIN: &str = "main$user";

/// If `functions` holds a user-defined `main`, rename it to [`USER_MAIN`]
/// and rewrite every call of it (`Call.function`) and reference to it
/// (`Var.name`) in all functions, so `main` is free for the module body.
/// No-op when there is no user `main`.
pub fn reserve_main(functions: &mut HashMap<String, Function>) -> anyhow::Result<()> {
    let Some(user_main) = functions.remove("main") else {
        return Ok(());
    };
    functions.insert(USER_MAIN.to_string(), user_main);
    for f in functions.values_mut() {
        let mut v = serde_json::to_value(&*f)?;
        rename_main(&mut v);
        *f = serde_json::from_value(v)?;
    }
    Ok(())
}

fn rename_main(v: &mut serde_json::Value) {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            let key = match map.get("type").and_then(Value::as_str) {
                Some("Call") => Some("function"),
                Some("Var") => Some("name"),
                _ => None,
            };
            if let Some(key) = key {
                if map.get(key).and_then(Value::as_str) == Some("main") {
                    map.insert(key.to_string(), Value::String(USER_MAIN.to_string()));
                }
            }
            map.values_mut().for_each(rename_main);
        }
        Value::Array(xs) => xs.iter_mut().for_each(rename_main),
        _ => {}
    }
}
