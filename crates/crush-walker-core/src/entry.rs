//! The entry function every walker's CAST is run from is named `main`
//! (the compiler looks `main` up by name; `Program::entry` is not consulted).
//! A walker that wraps the module's top-level code in `main` must therefore
//! first move a user function that is itself called `main` out of the way,
//! or the module body replaces it (CRUSH-560).

use crush_cast::{Function, Statement};
use std::collections::HashMap;

/// Name a user-defined `main` is moved to. It must stay a valid CASM function
/// name, so it is a reserved-looking identifier rather than a symbol.
pub const USER_MAIN: &str = "__user_main";

/// If `functions` holds a user-defined `main`, rename it to [`USER_MAIN`]
/// and rewrite every call of it (`Call.function`) and reference to it
/// (`Var.name`) in all functions and in `body` (the module's top-level
/// statements, about to become `main`), so `main` is free for the body.
/// No-op when there is no user `main`.
///
/// Binding sites are not tracked, so the rule is conservative: a function
/// that itself binds the name `main` (parameter, `VarDecl`, `Assign`, `For`
/// variable, catch variable, pattern, nested function) is left untouched,
/// since there `main` is that local. Refuses (loudly) when the module body
/// rebinds `main`, or when [`USER_MAIN`] is already taken.
pub fn reserve_main(
    functions: &mut HashMap<String, Function>,
    body: &mut Vec<Statement>,
) -> anyhow::Result<()> {
    if !functions.contains_key("main") {
        return Ok(());
    }
    if functions.contains_key(USER_MAIN) {
        anyhow::bail!("`{USER_MAIN}` is reserved for a user function named `main`");
    }
    let mut body_json = serde_json::to_value(&*body)?;
    if binds_main(&body_json) {
        anyhow::bail!(
            "top-level code rebinds `main` while a function named `main` is defined: \
             not supported; rename one of them"
        );
    }
    let user_main = functions.remove("main").expect("checked above");
    functions.insert(USER_MAIN.to_string(), user_main);
    for f in functions.values_mut() {
        let mut v = serde_json::to_value(&*f)?;
        if binds_main(&v) {
            continue;
        }
        rename_main(&mut v);
        *f = serde_json::from_value(v)?;
    }
    rename_main(&mut body_json);
    *body = serde_json::from_value(body_json)?;
    Ok(())
}

/// Does anything in `v` bind the name `main`?
fn binds_main(v: &serde_json::Value) -> bool {
    use serde_json::Value;
    let is_main = |x: Option<&Value>| x.and_then(Value::as_str) == Some("main");
    match v {
        Value::Object(map) => {
            let own = match map.get("type").and_then(Value::as_str) {
                Some("VarDecl" | "FunctionDef" | "Identifier") => is_main(map.get("name")),
                Some("Assign") => is_main(map.get("target")),
                Some("For") => is_main(map.get("variable")),
                Some("TryCatch") => is_main(map.get("error_var")),
                // Expression-position assignment (`x = v` inside an expression).
                Some("Call") => is_main_call_arg(map, "__crush_assign__"),
                _ => false,
            };
            let params = map.get("params").is_some_and(params_bind_main);
            own || params || map.values().any(binds_main)
        }
        Value::Array(xs) => xs.iter().any(binds_main),
        _ => false,
    }
}

fn is_main_call_arg(map: &serde_json::Map<String, serde_json::Value>, callee: &str) -> bool {
    map.get("function").and_then(serde_json::Value::as_str) == Some(callee)
        && map.get("args").and_then(|a| a.get(0)).is_some_and(|a| {
            a.get("type").and_then(serde_json::Value::as_str) == Some("Var")
                && a.get("name").and_then(serde_json::Value::as_str) == Some("main")
        })
}

fn params_bind_main(p: &serde_json::Value) -> bool {
    p.as_array().is_some_and(|ps| {
        ps.iter()
            .any(|x| x.get(0).and_then(serde_json::Value::as_str) == Some("main"))
    })
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
