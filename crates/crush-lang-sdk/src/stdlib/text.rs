//! `text.sort` / `text.uniq` — the pure half of exosphere's `text` family.
//!
//! Ported from exosphere `core/base/stdlib/src/text.rs`. The rest of that
//! family reads files (`text.head`, `text.tail`, `text.wc`, `text.cut`,
//! `text.grep`) and lives with the sandboxed `--fs` host capabilities in
//! `crate::text_tools`; `text.echo` duplicated `io.print` and was dropped.

use super::value_to_string;
use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};

pub(super) fn register(caps: &mut HostCaps) {
    caps.register(Box::new(TextSortCap));
    caps.register(Box::new(TextUniqCap));
}

fn lines(cap: &str, v: &Value) -> Result<Vec<String>, String> {
    match v {
        Value::Array(a) => Ok(a.borrow().iter().map(value_to_string).collect()),
        other => Err(format!("{cap}: expected array, got {}", other.type_name())),
    }
}

fn str_array(items: Vec<String>) -> Value {
    Value::new_array(items.into_iter().map(Value::Str).collect())
}

std_cap!(TextSortCap, "text.sort", Some(1), |args: &[Value]| {
    let mut items = lines("text.sort", &args[0])?;
    items.sort();
    Ok(Some(str_array(items)))
});

// Like `uniq(1)`: drops *adjacent* duplicates only (sort first for a set).
std_cap!(TextUniqCap, "text.uniq", Some(1), |args: &[Value]| {
    let mut items = lines("text.uniq", &args[0])?;
    items.dedup();
    Ok(Some(str_array(items)))
});

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, arg: Value) -> Result<Option<Value>, String> {
        let mut caps = HostCaps::new();
        register(&mut caps);
        caps.get(name).expect(name).call(vec![arg])
    }

    fn arr(items: &[&str]) -> Value {
        str_array(items.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn sort_then_uniq_is_a_set() {
        let sorted = call("text.sort", arr(&["b", "a", "b", "c", "a"]))
            .unwrap()
            .unwrap();
        assert_eq!(sorted, arr(&["a", "a", "b", "b", "c"]));
        assert_eq!(call("text.uniq", sorted), Ok(Some(arr(&["a", "b", "c"]))));
    }

    #[test]
    fn uniq_only_drops_adjacent_duplicates() {
        assert_eq!(
            call("text.uniq", arr(&["a", "a", "b", "a"])),
            Ok(Some(arr(&["a", "b", "a"])))
        );
        assert!(call("text.sort", Value::Str("x".into())).is_err());
    }
}
