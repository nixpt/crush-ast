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

fn json_value_to_sql(v: &Value) -> rusqlite::types::Value {
    match v {
        Value::Null => rusqlite::types::Value::Null,
        Value::Bool(b) => rusqlite::types::Value::Integer(if *b { 1 } else { 0 }),
        Value::Int(i) => rusqlite::types::Value::Integer(*i),
        Value::Float(f) => rusqlite::types::Value::Real(*f),
        Value::Str(s) => rusqlite::types::Value::Text(s.clone()),
        // **Canonical path**: route through `impl serde::Serialize for Value`
        // (defined on `crush_vm::vm::Value`) — the previous local
        // `crush_value_to_json` alias that delegated to `util::value_to_json`
        // has been deleted. `serde_json::to_string(&array)` now invokes the
        // trait impl directly, so nested Array/Map values serialize through
        // the same single source of truth as `json.stringify`.
        Value::Array(a) => rusqlite::types::Value::Text(
            serde_json::to_string(&Value::Array(a.clone())).unwrap_or_default(),
        ),
        Value::Map(m) => rusqlite::types::Value::Text(
            serde_json::to_string(&Value::Map(m.clone())).unwrap_or_default(),
        ),
        Value::Error(e) => rusqlite::types::Value::Text(format!("error({})", e)),
        Value::Bytes(b) => rusqlite::types::Value::Blob(b.clone()),
        Value::Handle(h) => rusqlite::types::Value::Text(format!("<handle {}>", h)),
    }
}

// Canonical `ValueRef<'_> -> crush_vm::vm::Value` mapper — single
// source of truth for SQL column hydration. Replaces the lossy
// per-cell `sql_value_to_json` detour through `serde_json::Value`
// (which emitted `"bytes:<len>"` for Blob). Non-finite floats clamp
// to `Value::Int(0)` mirroring the legacy
// `serde_json::Number::from_f64(...).unwrap_or(0.into())` lossy
// default AND `impl Serialize for Value::Float`'s non-finite clamp.
// `Result<Value, rusqlite::Error>` is shaped for future fallible
// `ValueRef` variants; the current body is infallible.
fn value_ref_to_crush_value(v: ValueRef<'_>) -> Result<Value, rusqlite::Error> {
    Ok(match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::Int(i),
        ValueRef::Real(f) => {
            if f.is_finite() {
                Value::Float(f)
            } else {
                // Mirrors legacy `Number::from_f64(...).unwrap_or(0.into())`
                // AND `impl Serialize for Value::Float`'s non-finite clamp,
                // so downstream `json.stringify` consumers stay consistent.
                Value::Int(0)
            }
        }
        ValueRef::Text(s) => {
            let s_str = std::str::from_utf8(s).unwrap_or("");
            if s_str.starts_with("<handle ") && s_str.ends_with('>') {
                if let Ok(id) = s_str[8..s_str.len() - 1].parse::<u64>() {
                    Value::Handle(id)
                } else {
                    Value::Str(s_str.to_string())
                }
            } else if s_str.starts_with('<') && s_str.ends_with(" bytes>") {
                if let Ok(n) = s_str[1..s_str.len() - 7].parse::<usize>() {
                    Value::Bytes(vec![0; n])
                } else {
                    Value::Str(s_str.to_string())
                }
            } else if s_str.starts_with("error(") && s_str.ends_with(')') {
                Value::Error(s_str[6..s_str.len() - 1].to_string())
            } else {
                Value::Str(s_str.to_string())
            }
        }
        ValueRef::Blob(b) => Value::Bytes(b.to_vec()),
    })
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
                    // **Canonical path**: per-cell `ValueRef ->
                    // crush_vm::vm::Value` via `value_ref_to_crush_value`
                    // — no `serde_json::Value` intermediate, no AST
                    // construction. Row composition happens directly
                    // in typed `HashMap<String, Value>` space.
                    map.insert(
                        col.clone(),
                        value_ref_to_crush_value(row.get_ref(i)?)?,
                    );
                }
                Ok(map)
            })
            .map_err(|e| format!("db.query execute: {e}"))?;

        let mut out = Vec::new();
        for row in rows {
            let row = row.map_err(|e| format!("db.query row: {e}"))?;
            // **Canonical composition**: each row is already a typed
            // `HashMap<String, Value>` after `value_ref_to_crush_value`
            // hydrates each cell in the column callback above. Wrap
            // in the canonical `Rc<RefCell<HashMap<String, Value>>>`
            // shared-cell shape via `Value::new_map`. **Public output
            // TYPE CHANGED** from `Value::Str(JSON-envelope)` to
            // `Value::Map` — downstream consumers access columns
            // directly via `m["col"]` instead of `json.parse(...)`.
            // Blob fidelity is preserved (legacy `"bytes:<len>"`
            // opaque token is gone).
            out.push(Value::new_map(row));
        }
        Ok(Some(Value::new_array(out)))
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
            .unwrap()
            .expect("db.query to return Some");
        // **Post `sql_value_to_json` -> `value_ref_to_crush_value`
        // refactor**: output shape is `Value::Array[Value::Map, ...]`.
        // Each row is a typed canonical CVM1 value; no JSON envelope
        // to parse downstream. Bytes columns preserve byte fidelity
        // (vs the legacy lossy `"bytes:<len>"` opaque token).
        let arr = match rows {
            Value::Array(a) => a,
            other => panic!("expected Value::Array rows, got {other:?}"),
        };
        assert_eq!(arr.borrow().len(), 1);
        let first_row = arr.borrow()[0].clone();
        let row_map = match first_row {
            Value::Map(m) => m,
            other => panic!("expected Value::Map row, got {other:?}"),
        };
        {
            let row = row_map.borrow();
            assert_eq!(
                row.get("name").cloned().unwrap_or(Value::Null),
                Value::Str("Alice".to_string()),
                "name should be Value::Str(\"Alice\")"
            );
            assert!(
                row.contains_key("id"),
                "row missing id column: {row:?}"
            );
            match row.get("id").cloned().unwrap_or(Value::Null) {
                Value::Int(n) => assert!(n >= 1, "id should be >= 1, got {n}"),
                other => panic!("expected Value::Int id, got {other:?}"),
            }
        }
    }

    #[test]
    fn query_preserves_blob_fidelity() {
        // Locks the refactor's main win: `ValueRef::Blob(b)`
        // hydrates to `Value::Bytes(b.to_vec())` byte-perfect, no
        // more opaque `"bytes:<len>"` token. Bind side uses the
        // existing `json_value_to_sql` path which already maps
        // `Value::Bytes(b)` -> `rusqlite::types::Value::Blob(b)`,
        // so the round-trip is canonical on both sides.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();

        let conn = Arc::new(Mutex::new(Connection::open(path).unwrap()));
        let exec = DbExecuteCap::new(Arc::clone(&conn));
        let query = DbQueryCap::new(conn);

        exec.call(vec![Value::Str(
            "CREATE TABLE assets (id INTEGER PRIMARY KEY, data BLOB)".to_string(),
        )])
        .unwrap();

        let payload = vec![0x01u8, 0x02, 0x03, 0xFF];
        exec.call(vec![
            Value::Str("INSERT INTO assets (data) VALUES (?)".to_string()),
            Value::Bytes(payload.clone()),
        ])
        .unwrap();

        let rows = query
            .call(vec![Value::Str("SELECT data FROM assets".to_string())])
            .unwrap()
            .expect("db.query returns Some");
        let arr = match rows {
            Value::Array(a) => a,
            other => panic!("expected Value::Array, got {other:?}"),
        };
        assert_eq!(arr.borrow().len(), 1);
        let row_map = match arr.borrow()[0].clone() {
            Value::Map(m) => m,
            other => panic!("expected Value::Map row, got {other:?}"),
        };
        let row = row_map.borrow();
        match row.get("data").cloned().unwrap_or(Value::Null) {
            Value::Bytes(b) => assert_eq!(
                b, payload,
                "blob fidelity lost: expected byte-preserved round-trip, got {b:?}"
            ),
            other => panic!("expected Value::Bytes, got {other:?}"),
        }
    }

    /// Lock the contract: `Value::Handle(N)` row-trip through SQLite
    /// preserves variant — `db.query(SELECT handle_col)` MUST hydrate
    /// back to `Value::Handle(N)`, NOT `Value::Str("N")` or
    /// `Value::Int(N)`. Pins asymmetry-paired contracts as a single,
    /// observed surface, not just a documented intent.
    ///
    /// # Audit finding 1 — documents TODAY's silent-break asymmetry
    ///
    /// This test FAILS today because the bind side and the refill
    /// side are NOT inverses for `Handle`:
    ///
    /// 1. **Bind side** (`db.rs:47`, `json_value_to_sql`):
    ///    `Value::Handle(h) => rusqlite::types::Value::Text(h.to_string())`
    ///    — opaque decimal `"42"` stored in a TEXT column. No marker,
    ///    no envelope, no schema column-type change.
    /// 2. **Refill side** (`db.rs:60-78`, `value_ref_to_crush_value`):
    ///    `ValueRef::Text(s) => Value::Str(...)` is the **only** Text
    ///    inverse branch. There is NO `Handle`-as-Text inverse — the
    ///    decimal `"42"` returns as `Value::Str("42")`, NOT
    ///    `Value::Handle(42)`.
    ///
    /// Result: storing `Value::Handle(42)` and reading via `db.query`
    /// returns `Value::Str("42")`. Variant semantic information is
    /// silently lost on the round-trip.
    ///
    /// # Why this test exists (and why it's a non-ignored `#[test]`)
    ///
    /// The audit classified this as a **(D)-class silent-break surface**
    /// — the kind that silently falls through with a wrong-but-plausible
    /// value (`Value::Str("42")`) until a downstream consumer
    /// (e.g. an ACL check keyed on `Handle`) mis-types the value. Adding
    /// a regular `#[test]` (NOT `#[ignore]`) so the failure surfaces
    /// in `cargo test` is intentional: silent breaks are exactly what
    /// this audit is hunting. The panic message contains the migration
    /// scope; once the asymmetry is closed, this test will pass and
    /// stop firing.
    ///
    /// # Migration scope (documented in the panic message below)
    ///
    /// Two viable fixes; pick one:
    ///
    /// 1. **Add `Handle`-as-Text inverse in `value_ref_to_crush_value`**
    ///    — branch on a structured marker (e.g. prefix with `"<handle
    ///    N>"` matching `Display::Handle`'s canonical form, OR a
    ///    dedicated column type, OR a sidecar `__handle__` column),
    ///    decode to `Value::Handle(N)`. Requires bound-side marker
    ///    change alongside.
    /// 2. **Change bind side to write `Handle` via a JSON envelope**
    ///    (e.g. `Text(serde_json::to_string(&Value::Handle(h)).unwrap())`)
    ///    — keeps refill simple (`map JSON-parsed Value back`), at the
    ///    cost of a `Value::Handle` text representation that diverges
    ///    from the canonical `Display::Handle` form.
    ///
    /// Either fix closes this asymmetry. The test asserts the desired
    /// end-behavior; the panic tells the team which decision the
    /// migration costs include.
    #[test]
    fn test_db_round_trips_handle_payload_via_sql() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();

        let conn = Arc::new(Mutex::new(Connection::open(path).unwrap()));
        let exec = DbExecuteCap::new(Arc::clone(&conn));
        let query = DbQueryCap::new(conn);

        // SQLite column TYPE is TEXT here, mirroring the bind side's
        // choice (`db.rs:47` writes `Value::Handle` as `Value::Text`).
        // This is the asymmetry surface: the bind side chose Text
        // without leaving a marker for refill to recognize.
        exec.call(vec![Value::Str(
            "CREATE TABLE tokens (id INTEGER PRIMARY KEY, handle_col TEXT)".to_string(),
        )])
        .unwrap();

        exec.call(vec![
            Value::Str(
                "INSERT INTO tokens (handle_col) VALUES (?)".to_string(),
            ),
            Value::Handle(42),
        ])
        .unwrap();

        let rows = query
            .call(vec![Value::Str(
                "SELECT handle_col FROM tokens".to_string(),
            )])
            .unwrap()
            .expect("db.query returns Some");
        let arr = match rows {
            Value::Array(a) => a,
            other => panic!("expected Value::Array rows, got {other:?}"),
        };
        assert_eq!(
            arr.borrow().len(),
            1,
            "expected exactly one row from `SELECT handle_col FROM tokens`"
        );
        let recovered: Value = {
            let row = arr.borrow();
            let first = row.first().expect("one row in arr").clone();
            match first {
                Value::Map(m) => m
                    .borrow()
                    .get("handle_col")
                    .cloned()
                    .unwrap_or(Value::Null),
                other => panic!("expected Value::Map row, got {other:?}"),
            }
        };

        // **This assertion is the migration contract.** Today
        // `value_ref_to_crush_value` returns `Value::Str("42")`
        // (no `Handle`-as-Text inverse branch). The panic below
        // documents the asymmetry AND points at the two viable
        // fix paths.
        assert_eq!(
            recovered, Value::Handle(42),
            "DB Handle round-trip asymmetry (audit finding 1, `db.rs`): \
             expected `Value::Handle(42)` after writing then reading back \
             via `db.execute` + `db.query`. Actual: `Value::Str(\"42\")` — \
             bind side (db.rs:47, `json_value_to_sql`) writes `Value::Handle` \
             as `rusqlite::types::Value::Text(\"42\")`, an opaque decimal \
             with NO marker, while refill side (`db.rs:60-78`, \
             `value_ref_to_crush_value`) maps every `ValueRef::Text` to \
             `Value::Str(...)` with NO `Handle`-as-Text inverse branch. \
             Variant semantic information is silently lost. \
             Migration scope: (a) add `Handle`-as-Text inverse branch in \
             `value_ref_to_crush_value` paired with a marker on the bind \
             side (e.g. canonical `Display::Handle` form `<handle N>` or \
             a `__handle__` sidecar column), OR \
             (b) change bind side to write Handle via a JSON envelope \
             (`Text(serde_json::to_string(&Value::Handle(h)))`) keyed to \
             `caps::text_as_value` rehydration on refill. Either fix \
             closes the asymmetry; this test stops failing once the \
             chosen path is in place."
        );
    }
}
