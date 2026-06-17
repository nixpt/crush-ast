//! Database host capabilities for the CRUSH runtime.
//!
//! Enabled by the `db` cargo feature. Provides SQLite query/execute via
//! `rusqlite`, bound to a single database path.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crush_vm::vm::Value;
use crush_vm::{HostCap, HostCapSpec, HostCaps};
use rusqlite::{Connection, types::ValueRef};

/// Add database capabilities to an existing [`HostCaps`] registry.
pub fn register(caps: &mut HostCaps, db_path: &str) -> Result<(), String> {
    let conn = Connection::open(db_path).map_err(|e| format!("db open {db_path}: {e}"))?;
    let conn = Arc::new(Mutex::new(conn));
    caps.register(Box::new(DbQueryCap::new(Arc::clone(&conn))));
    caps.register(Box::new(DbExecuteCap::new(conn)));
    Ok(())
}

fn params_from_values(values: &[Value]) -> Vec<rusqlite::types::Value> {
    values.iter().map(json_value_to_sql).collect()
}

fn crush_value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(b),
        Value::Int(i) => serde_json::Value::Number((*i).into()),
        Value::Float(f) => {
            serde_json::Value::Number(serde_json::Number::from_f64(*f).unwrap_or(0.into()))
        }
        Value::Str(s) => serde_json::Value::String(s.clone()),
        Value::Array(a) => serde_json::Value::Array(a.iter().map(crush_value_to_json).collect()),
        Value::Map(m) => {
            let obj: serde_json::Map<String, serde_json::Value> = m
                .iter()
                .map(|(k, v)| (k.clone(), crush_value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        crush_vm::vm::Value::Error(e) => serde_json::Value::String(format!("error({})", e)),
        crush_vm::vm::Value::Bytes(b) => serde_json::Value::String(format!("<{} bytes>", b.len())),
    }
}

fn json_value_to_sql(v: &Value) -> rusqlite::types::Value {
    match v {
        Value::Null => rusqlite::types::Value::Null,
        Value::Bool(b) => rusqlite::types::Value::Integer(if b { 1 } else { 0 }),
        Value::Int(i) => rusqlite::types::Value::Integer(*i),
        Value::Float(f) => rusqlite::types::Value::Real(*f),
        Value::Str(s) => rusqlite::types::Value::Text(s.clone()),
        Value::Array(a) => rusqlite::types::Value::Text(
            serde_json::to_string(&crush_value_to_json(&Value::Array(a.clone())))
                .unwrap_or_default(),
        ),
    }
}

fn sql_value_to_json(v: ValueRef<'_>) -> serde_json::Value {
    match v {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Integer(i) => serde_json::Value::Number(i.into()),
        ValueRef::Real(f) => {
            serde_json::Value::Number(serde_json::Number::from_f64(f).unwrap_or(0.into()))
        }
        ValueRef::Text(s) => {
            serde_json::Value::String(std::str::from_utf8(s).unwrap_or("").to_string())
        }
        ValueRef::Blob(b) => serde_json::Value::String(format!("bytes:{}", b.len())),
    }
}

pub struct DbQueryCap {
    conn: Arc<Mutex<Connection>>,
}

impl DbQueryCap {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }
}

impl HostCap for DbQueryCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "db.query".to_string(),
            argc: None,
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        if args.is_empty() {
            return Err("db.query requires at least a SQL string".into());
        }
        let sql = crate::caps::value_as_text(&args[0]);
        let params = params_from_values(&args[1..]);

        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("db.query prepare: {e}"))?;
        let cols: Vec<String> = stmt
            .column_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        let rows = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                let mut map = HashMap::new();
                for (i, col) in cols.iter().enumerate() {
                    map.insert(col.clone(), sql_value_to_json(row.get_ref(i)?));
                }
                Ok(map)
            })
            .map_err(|e| format!("db.query execute: {e}"))?;

        let mut out = Vec::new();
        for row in rows {
            let row = row.map_err(|e| format!("db.query row: {e}"))?;
            out.push(serde_json::Value::Object(
                row.into_iter().map(|(k, v)| (k, v)).collect(),
            ));
        }
        Ok(Some(Value::Array(
            out.into_iter()
                .map(|v| {
                    serde_json::to_string(&v)
                        .map(Value::Str)
                        .map_err(|e| e.to_string())
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("db.query serialize: {e}"))?,
        )))
    }
}

pub struct DbExecuteCap {
    conn: Arc<Mutex<Connection>>,
}

impl DbExecuteCap {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }
}

impl HostCap for DbExecuteCap {
    fn spec(&self) -> HostCapSpec {
        HostCapSpec {
            name: "db.execute".to_string(),
            argc: None,
            returns: true,
        }
    }

    fn call(&self, args: Vec<Value>) -> Result<Option<Value>, String> {
        if args.is_empty() {
            return Err("db.execute requires at least a SQL string".into());
        }
        let sql = crate::caps::value_as_text(&args[0]);
        let params = params_from_values(&args[1..]);

        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let affected = conn
            .execute(&sql, rusqlite::params_from_iter(params.iter()))
            .map_err(|e| format!("db.execute: {e}"))?;
        Ok(Some(Value::Int(affected as i64)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_and_query_roundtrip() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();

        let conn = Arc::new(Mutex::new(Connection::open(path).unwrap()));
        let exec = DbExecuteCap::new(Arc::clone(&conn));
        let query = DbQueryCap::new(conn);

        exec.call(vec![Value::Str(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)".to_string(),
        )])
        .unwrap();

        let affected = exec
            .call(vec![
                Value::Str("INSERT INTO users (name) VALUES (?)".to_string()),
                Value::Str("Alice".to_string()),
            ])
            .unwrap();
        assert_eq!(affected, Some(Value::Int(1)));

        let rows = query
            .call(vec![
                Value::Str("SELECT id, name FROM users WHERE name = ?".to_string()),
                Value::Str("Alice".to_string()),
            ])
            .unwrap();
        // Results are serialized as JSON strings in Value::Str for simplicity.
        assert!(rows.is_some());
    }
}
