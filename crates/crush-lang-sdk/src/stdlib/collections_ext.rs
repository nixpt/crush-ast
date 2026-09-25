//! The map / record half of `collections.*`.
//!
//! Ported from exosphere `core/base/stdlib/src/collections.rs`: `keys`,
//! `values`, `entries`, `merge`, `pluck`, `sort_by`, `find`, `any`, `all`
//! (the array half — `len`, `reverse`, ... — was ported earlier and lives in
//! `stdlib.rs`).
//!
//! One deliberate difference: nanovm returned `keys` / `values` / `entries`
//! in `HashMap` iteration order, i.e. a different order on every run. They
//! are sorted by key here so programs are deterministic.

use super::get_str;
use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};
use std::cmp::Ordering;
use std::collections::HashMap;

pub(super) fn register(caps: &mut HostCaps) {
    caps.register(Box::new(CollKeysCap));
    caps.register(Box::new(CollValuesCap));
    caps.register(Box::new(CollEntriesCap));
    caps.register(Box::new(CollMergeCap));
    caps.register(Box::new(CollPluckCap));
    caps.register(Box::new(CollSortByCap));
    caps.register(Box::new(CollFindCap));
    caps.register(Box::new(CollAnyCap));
    caps.register(Box::new(CollAllCap));
}

fn map_arg(cap: &str, v: &Value) -> Result<HashMap<String, Value>, String> {
    match v {
        Value::Map(m) => Ok(m.borrow().clone()),
        other => Err(format!("{cap}: expected map, got {}", other.type_name())),
    }
}

fn array_arg(cap: &str, v: &Value) -> Result<Vec<Value>, String> {
    match v {
        Value::Array(a) => Ok(a.borrow().clone()),
        other => Err(format!("{cap}: expected array, got {}", other.type_name())),
    }
}

/// Entries sorted by key.
fn sorted_entries(m: HashMap<String, Value>) -> Vec<(String, Value)> {
    let mut entries: Vec<_> = m.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

/// `item[field]` if `item` is a map that has it.
fn field(item: &Value, name: &str) -> Option<Value> {
    match item {
        Value::Map(m) => m.borrow().get(name).cloned(),
        _ => None,
    }
}

std_cap!(
    CollKeysCap,
    "collections.keys",
    Some(1),
    |args: &[Value]| {
        let m = map_arg("collections.keys", &args[0])?;
        let keys = sorted_entries(m)
            .into_iter()
            .map(|(k, _)| Value::Str(k))
            .collect();
        Ok(Some(Value::new_array(keys)))
    }
);

std_cap!(
    CollValuesCap,
    "collections.values",
    Some(1),
    |args: &[Value]| {
        let m = map_arg("collections.values", &args[0])?;
        let values = sorted_entries(m).into_iter().map(|(_, v)| v).collect();
        Ok(Some(Value::new_array(values)))
    }
);

std_cap!(
    CollEntriesCap,
    "collections.entries",
    Some(1),
    |args: &[Value]| {
        let m = map_arg("collections.entries", &args[0])?;
        let entries = sorted_entries(m)
            .into_iter()
            .map(|(k, v)| Value::new_array(vec![Value::Str(k), v]))
            .collect();
        Ok(Some(Value::new_array(entries)))
    }
);

// A new map; on a shared key the second map wins. Neither input is modified.
std_cap!(
    CollMergeCap,
    "collections.merge",
    Some(2),
    |args: &[Value]| {
        let mut merged = map_arg("collections.merge", &args[0])?;
        merged.extend(map_arg("collections.merge", &args[1])?);
        Ok(Some(Value::new_map(merged)))
    }
);

// `[item[field] for item in array]`, null where an item lacks the field.
std_cap!(
    CollPluckCap,
    "collections.pluck",
    Some(2),
    |args: &[Value]| {
        let items = array_arg("collections.pluck", &args[0])?;
        let name = get_str(args, 1)?;
        let plucked = items
            .iter()
            .map(|item| field(item, &name).unwrap_or(Value::Null))
            .collect();
        Ok(Some(Value::new_array(plucked)))
    }
);

fn compare(a: &Option<Value>, b: &Option<Value>) -> Ordering {
    match (a, b) {
        (Some(Value::Int(x)), Some(Value::Int(y))) => x.cmp(y),
        (Some(Value::Str(x)), Some(Value::Str(y))) => x.cmp(y),
        (Some(x), Some(y)) => match (as_f64(x), as_f64(y)) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            _ => Ordering::Equal,
        },
        _ => Ordering::Equal,
    }
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

// Stable sort of records by one field (ints, floats, strings); items whose
// field is missing or not comparable keep their relative position.
std_cap!(
    CollSortByCap,
    "collections.sort_by",
    Some(2),
    |args: &[Value]| {
        let items = array_arg("collections.sort_by", &args[0])?;
        let name = get_str(args, 1)?;
        let mut keyed: Vec<(Option<Value>, Value)> = items
            .into_iter()
            .map(|item| (field(&item, &name), item))
            .collect();
        keyed.sort_by(|a, b| compare(&a.0, &b.0));
        Ok(Some(Value::new_array(
            keyed.into_iter().map(|(_, item)| item).collect(),
        )))
    }
);

/// Shared argument handling for `find` / `any` / `all`: (records, key, value).
fn record_query(cap: &str, args: &[Value]) -> Result<(Vec<Value>, String, Value), String> {
    Ok((
        array_arg(cap, &args[0])?,
        get_str(args, 1)?,
        args[2].clone(),
    ))
}

// First record whose `key` equals `value`, else null.
std_cap!(
    CollFindCap,
    "collections.find",
    Some(3),
    |args: &[Value]| {
        let (items, key, value) = record_query("collections.find", args)?;
        let found = items
            .into_iter()
            .find(|item| field(item, &key).as_ref() == Some(&value));
        Ok(Some(found.unwrap_or(Value::Null)))
    }
);

std_cap!(
    CollAnyCap,
    "collections.any",
    Some(3),
    |args: &[Value]| {
        let (items, key, value) = record_query("collections.any", args)?;
        let any = items
            .iter()
            .any(|item| field(item, &key).as_ref() == Some(&value));
        Ok(Some(Value::Bool(any)))
    }
);

// True for an empty array (vacuous truth), as in nanovm.
std_cap!(
    CollAllCap,
    "collections.all",
    Some(3),
    |args: &[Value]| {
        let (items, key, value) = record_query("collections.all", args)?;
        let all = items
            .iter()
            .all(|item| field(item, &key).as_ref() == Some(&value));
        Ok(Some(Value::Bool(all)))
    }
);

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: Vec<Value>) -> Result<Option<Value>, String> {
        let mut caps = HostCaps::new();
        register(&mut caps);
        caps.get(name).expect(name).call(args)
    }

    fn map(pairs: &[(&str, Value)]) -> Value {
        Value::new_map(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    fn s(v: &str) -> Value {
        Value::Str(v.to_string())
    }

    fn people() -> Value {
        Value::new_array(vec![
            map(&[("name", s("cy")), ("age", Value::Int(40))]),
            map(&[("name", s("al")), ("age", Value::Int(30))]),
            map(&[("name", s("bo")), ("age", Value::Int(30))]),
        ])
    }

    #[test]
    fn keys_values_entries_are_key_sorted() {
        let m = map(&[("b", Value::Int(2)), ("a", Value::Int(1))]);
        assert_eq!(
            call("collections.keys", vec![m.clone()]),
            Ok(Some(Value::new_array(vec![s("a"), s("b")])))
        );
        assert_eq!(
            call("collections.values", vec![m.clone()]),
            Ok(Some(Value::new_array(vec![Value::Int(1), Value::Int(2)])))
        );
        assert_eq!(
            call("collections.entries", vec![m]),
            Ok(Some(Value::new_array(vec![
                Value::new_array(vec![s("a"), Value::Int(1)]),
                Value::new_array(vec![s("b"), Value::Int(2)]),
            ])))
        );
    }

    #[test]
    fn merge_prefers_the_second_map_and_copies() {
        let a = map(&[("x", Value::Int(1)), ("y", Value::Int(1))]);
        let b = map(&[("y", Value::Int(2))]);
        let merged = call("collections.merge", vec![a.clone(), b])
            .unwrap()
            .unwrap();
        assert_eq!(merged, map(&[("x", Value::Int(1)), ("y", Value::Int(2))]));
        assert_eq!(a, map(&[("x", Value::Int(1)), ("y", Value::Int(1))]));
    }

    #[test]
    fn pluck_and_stable_sort_by() {
        assert_eq!(
            call("collections.pluck", vec![people(), s("name")]),
            Ok(Some(Value::new_array(vec![s("cy"), s("al"), s("bo")])))
        );
        let sorted = call("collections.sort_by", vec![people(), s("age")])
            .unwrap()
            .unwrap();
        assert_eq!(
            call("collections.pluck", vec![sorted, s("name")]),
            Ok(Some(Value::new_array(vec![s("al"), s("bo"), s("cy")])))
        );
    }

    #[test]
    fn find_any_all() {
        assert_eq!(
            call("collections.find", vec![people(), s("age"), Value::Int(30)]),
            Ok(Some(map(&[("name", s("al")), ("age", Value::Int(30))])))
        );
        assert_eq!(
            call("collections.find", vec![people(), s("age"), Value::Int(99)]),
            Ok(Some(Value::Null))
        );
        assert_eq!(
            call("collections.any", vec![people(), s("name"), s("bo")]),
            Ok(Some(Value::Bool(true)))
        );
        assert_eq!(
            call("collections.all", vec![people(), s("age"), Value::Int(30)]),
            Ok(Some(Value::Bool(false)))
        );
        assert_eq!(
            call(
                "collections.all",
                vec![Value::new_array(vec![]), s("age"), Value::Int(30)]
            ),
            Ok(Some(Value::Bool(true)))
        );
    }
}
