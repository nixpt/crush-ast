//! CVM1 interpreter — sandboxed execution with hard quotas.
//!
//! The only way out of the sandbox is `CAP_CALL`. The program must declare
//! each cap in `manifest.permissions`; the host can further restrict via
//! `Quotas::allowed_caps`. Division and modulo truncate toward zero (matching
//! Python's `int(a/b)` for same-sign and `a//b` for same-sign).
//!
//! Array and map values use `Rc<RefCell<...>>` (shared reference semantics):
//! cloning a `Value::Array` or `Value::Map` produces an alias, not a copy.
//! This matches Python/JS list/dict behavior — a DUP followed by ARR_SET
//! mutates the same underlying storage as the original.

use std::collections::HashMap;

use crate::bytecode::Program;
use crate::host::HostCaps;

#[derive(Debug, thiserror::Error)]
pub enum VmError {
    #[error("stack underflow")]
    StackUnderflow,
    #[error("stack quota exceeded ({0})")]
    StackQuota(usize),
    #[error("instruction quota exceeded ({0})")]
    StepQuota(usize),
    #[error("output quota exceeded ({0})")]
    OutputQuota(usize),
    #[error("call depth quota exceeded ({0})")]
    CallDepthQuota(usize),
    #[error("unknown opcode {0:#04x} at {1}")]
    UnknownOpcode(u8, usize),
    #[error("truncated instruction at {0}")]
    TruncatedInstruction(usize),
    #[error("const index out of range: {0}")]
    ConstOutOfRange(usize),
    #[error("load from uninitialised slot {0}")]
    UninitSlot(u16),
    #[error("jump target {0} out of range")]
    BadJump(usize),
    #[error("call to unknown function: {0}")]
    UnknownFunction(String),
    #[error("type error: expected {expected}, got {got}")]
    TypeError {
        expected: &'static str,
        got: &'static str,
    },
    #[error("array index out of range: {index} (len {len})")]
    ArrayBounds { index: i64, len: usize },
    #[error("array index must be int, got {0}")]
    BadIndex(&'static str),
    #[error("division by zero")]
    DivByZero,
    #[error("arithmetic overflow")]
    ArithmeticOverflow,
    #[error("capability not declared in manifest: {0}")]
    CapNotDeclared(String),
    #[error("capability denied by host: {0}")]
    CapDenied(String),
    #[error("unknown capability: {0}")]
    UnknownCap(String),
    #[error("{cap} takes {expected} arg(s), got {got}")]
    CapArity {
        cap: String,
        expected: usize,
        got: usize,
    },
    /// A capability call (currently: an `EXEC_LANG` polyglot subprocess)
    /// exceeded `Quotas::max_wall_time_ms` and was killed. Named after the
    /// cap so a hang is diagnosable, not a silent freeze.
    #[error("'{cap}' exceeded its {limit_ms}ms wall-clock quota and was killed")]
    CapTimeout { cap: String, limit_ms: u64 },
    /// A `@python`/`@javascript`/`@bash` polyglot block's **guest program**
    /// raised its own runtime exception (non-zero exit, e.g. a Python
    /// `ZeroDivisionError` or a Node `TypeError`) — or, distinctly (CRUSH-20),
    /// the buckets-sandboxed provisioning/spawn step itself failed before the
    /// guest ever ran. `phase` tells the two apart: see [`LangFailurePhase`].
    /// Neither is a missing-capability problem (`UnknownCap`) or a crush-side
    /// VM bug — reserve those for their own failure classes (see CRUSH-18).
    /// `crush_line` is the `.crush`-source line of the `@lang { ... }` block
    /// itself, when the compiler had one to attach; the guest's own internal
    /// line numbers (e.g. Python's `line 1` in a `-c` string) are a separate,
    /// unmapped coordinate space and are not translated here.
    #[error("@{lang} block {phase}: {message}")]
    LangRuntimeError {
        lang: String,
        message: String,
        crush_line: Option<u32>,
        phase: LangFailurePhase,
    },
}

/// Distinguishes the two ways an `EXEC_LANG` polyglot block can fail at
/// runtime (CRUSH-20). Both surface as `VmError::LangRuntimeError` — the
/// ticket's own guidance was to extend that variant rather than invent a
/// parallel error type, since both are "the polyglot block didn't complete
/// successfully," just at different stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LangFailurePhase {
    /// The guest program actually ran (in a real or sandboxed interpreter)
    /// and exited non-zero on its own — a bug in the guest's own code.
    GuestException,
    /// The buckets-sandboxed provisioning/spawn step (CRUSH-20's 4th
    /// execution path) failed before the guest program ever started:
    /// dependency resolution/fetch failed (network, unknown package —
    /// buckets has no PyPI/npm resolution, see the ticket's "numpy
    /// reframe"), or the sandboxed command itself could not be built or
    /// spawned. Not the guest's fault.
    SandboxSetup,
}

impl std::fmt::Display for LangFailurePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LangFailurePhase::GuestException => write!(f, "raised a runtime error"),
            LangFailurePhase::SandboxSetup => write!(f, "sandbox setup failed"),
        }
    }
}

/// Stack value — the types the CVM1 supports.
#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    /// Shared array (reference semantics via Rc<RefCell<...>>).
    Array(std::rc::Rc<std::cell::RefCell<Vec<Value>>>),
    /// Fixed-length heterogeneous sequence
    Tuple(Vec<Value>),
    /// Shared list
    List(std::rc::Rc<std::cell::RefCell<std::collections::LinkedList<Value>>>),
    /// Shared vector
    Vector(std::rc::Rc<std::cell::RefCell<Vec<Value>>>),
    /// Shared set (using Vec for uniqueness since Value doesn't impl Hash)
    Set(std::rc::Rc<std::cell::RefCell<Vec<Value>>>),
    /// Shared string-keyed map (reference semantics via Rc<RefCell<...>>).
    Map(std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, Value>>>),
    /// Error value (carries a message string).
    Error(String),
    /// Binary blob data.
    Bytes(Vec<u8>),
    /// Green thread handle — returned by spawn, consumed by await.
    Handle(u64),
    /// Foreign object handle — opaque reference to an external environment object.
    Foreign(u64),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            // Cross-type numeric equality: `2 == 2.0` is `true`, matching
            // chroma's Python VM (Python's `2 == 2.0` is `True`). Bool is
            // deliberately NOT coerced here -- only Int<->Float. `Value`
            // does not implement `Eq`/`Hash` (see the `Set` variant's doc
            // comment: it uses a linear-scan `Vec` for uniqueness precisely
            // because `Value` isn't hash-keyed), so widening `PartialEq`
            // does not violate the Eq/Hash consistency contract anywhere in
            // this crate. Precision note: for `|i| > 2^53`, `i as f64` is
            // not exact, so this comparison can disagree with true integer
            // equality at the extremes of i64's range -- same caveat as
            // Python's `int == float`.
            (Value::Int(i), Value::Float(f)) | (Value::Float(f), Value::Int(i)) => {
                *i as f64 == *f
            }
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => *a.borrow() == *b.borrow(),
            (Value::Tuple(a), Value::Tuple(b)) => a == b,
            (Value::List(a), Value::List(b)) => *a.borrow() == *b.borrow(),
            (Value::Vector(a), Value::Vector(b)) => *a.borrow() == *b.borrow(),
            (Value::Set(a), Value::Set(b)) => *a.borrow() == *b.borrow(),
            (Value::Map(a), Value::Map(b)) => *a.borrow() == *b.borrow(),
            (Value::Error(a), Value::Error(b)) => a == b,
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
            (Value::Handle(a), Value::Handle(b)) => a == b,
            (Value::Foreign(a), Value::Foreign(b)) => a == b,
            _ => false,
        }
    }
}

impl Value {
    pub(crate) fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "str",
            Value::Array(_) => "array",
            Value::Tuple(_) => "tuple",
            Value::List(_) => "list",
            Value::Vector(_) => "vector",
            Value::Set(_) => "set",
            Value::Map(_) => "map",
            Value::Error(_) => "error",
            Value::Bytes(_) => "bytes",
            Value::Handle(_) => "handle",
            Value::Foreign(_) => "foreign",
        }
    }

    pub(crate) fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Array(a) => !a.borrow().is_empty(),
            Value::Tuple(t) => !t.is_empty(),
            Value::List(l) => !l.borrow().is_empty(),
            Value::Vector(v) => !v.borrow().is_empty(),
            Value::Set(s) => !s.borrow().is_empty(),
            Value::Map(m) => !m.borrow().is_empty(),
            Value::Error(_) => true,
            Value::Bytes(b) => !b.is_empty(),
            Value::Handle(_) => true,
            Value::Foreign(_) => true,
        }
    }

    pub(crate) fn as_text(&self) -> String {
        // **Single source of truth** lives on the `impl Display for Value`
        // below. Kept as a `pub(crate)` one-line delegation so internal
        // VM call sites (e.g. the green-thread `out_parts` formatter)
        // stay stable when the Display impl evolves. Edit the Display
        // block; do NOT reintroduce the 30-line match body here.
        self.to_string()
    }

    pub(crate) fn is_numeric(&self) -> bool {
        matches!(self, Value::Int(_) | Value::Float(_))
    }
}

/// Canonical text rendering — the **single source of truth** for how
/// every CVM1 value surfaces as a string. Used by `io.print`,
/// `str.concat`, `conv.to_str`, `str.format`, `str.join`, all `path.*`
/// caps, the host's `caps::value_as_text`, and the VM's own internals.
///
/// Properties worth preserving when editing:
///
/// - `Value::Null` renders as the literal four-char `"null"` (matches
///   Python/JSON/JS repr and what `io.print` has always emitted). The
///   earlier pre-reconciliation behavior of `"".to_string()` was a bug
///   that the `crush-lang-sdk` E2E test caught empirically.
/// - `Value::Str(s)` emits `s` verbatim with no quoting — `io.print`'s
///   rendering is intentionally not JSON-style, so substring equality
///   on a printed value `"hello"` finds the literal `hello`, not
///   `"hello"` (with quotes). The `crush-lang-sdk` integration
///   `codebase_definition_surfaces_errors_weighted...` test pins this.
/// - `Value::Int(n)` and `Value::Bool(b)` emit Rust's `Display` for
///   the inner primitive — bare digits, `true`/`false`.
/// - `Value::Float(f)` emits `f64`'s `Display` **unless** `f` has no
///   fractional part, in which case it gets a `.0` suffix (e.g.
///   `3.0` not `3`) so integer and float values round-trip with
///   distinguishable representations.
/// - `Value::Error(e)` / `Value::Bytes(b)` / `Value::Handle(h)`
///   emit tagged prefixes (`error(...)`, `<N bytes>`, `<handle N>`)
///   so consumers can see the variant without a separate `type_of`
///   probe.
impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Null => f.write_str("null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(v) => {
                if v.fract() == 0.0 && v.is_finite() {
                    write!(f, "{v:.1}")
                } else {
                    write!(f, "{v}")
                }
            }
            Value::Str(s) => f.write_str(s),
            Value::Array(a) => {
                // `Rc<RefCell<...>>` reference semantics means a single
                // borrow is enough — no risk of conflicting borrows.
                let inner: Vec<String> = a.borrow().iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", inner.join(", "))
            }
            Value::Tuple(t) => {
                let inner: Vec<String> = t.iter().map(|v| v.to_string()).collect();
                write!(f, "({})", inner.join(", "))
            }
            Value::List(l) => {
                let inner: Vec<String> = l.borrow().iter().map(|v| v.to_string()).collect();
                write!(f, "List[{}]", inner.join(", "))
            }
            Value::Vector(v) => {
                let inner: Vec<String> = v.borrow().iter().map(|v| v.to_string()).collect();
                write!(f, "Vector[{}]", inner.join(", "))
            }
            Value::Set(s) => {
                let inner: Vec<String> = s.borrow().iter().map(|v| v.to_string()).collect();
                write!(f, "Set{{{}}}", inner.join(", "))
            }
            Value::Map(m) => {
                let inner: Vec<String> = m
                    .borrow()
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", v))
                    .collect();
                write!(f, "{{{}}}", inner.join(", "))
            }
            Value::Error(e) => write!(f, "error({e})"),
            Value::Bytes(b) => write!(f, "<{} bytes>", b.len()),
            Value::Handle(id) => write!(f, "<handle {id}>"),
            Value::Foreign(id) => write!(f, "<foreign {id}>"),
        }
    }
}

/// Canonical JSON / serde wire-format for CVM1 values — the **single
/// source of truth** for every consumer that wants a structured
/// `serde::Serialize` representation (json.stringify, json.stringify_pretty,
/// the `db` SQL binding layer, the `bus` message-payload layer, and any
/// future JSON-API surface).
///
/// Properties worth preserving when editing:
///
/// - `Value::Str(s)` is rendered **JSON-quoted** (with proper escape
///   sequences) on this path, intentionally divergent from the
///   `Display` impl's unquoted form. `io.print` uses `Display` and
///   produces bare tokens; `json.stringify` uses `Serialize` and
///   produces JSON strings. Drift between the two is **by design** —
///   the two formatters serve different consumers.
/// - `Value::Float(f)` with non-finite values (`NaN`, `±Inf`) is
///   lossily converted to `0` because `serde_json` does not allow
///   non-finite floats. Pre-existing call sites in `util::value_to_json`
///   (now removed) handled this via `Number::from_f64(...).unwrap_or(0)`.
/// - Opaque variants (`Value::Error`, `Value::Bytes`, `Value::Handle`)
///   are serialised as **string-wrapped tagged forms** matching their
///   `Display` shape verbatim: `"error(msg)"`, `"<N bytes>"`, `"<handle N>"`.
///   This is the lockstep-with-Display contract — any future drift
///   in the tagged-prefix texts must update BOTH `impl Display` and
///   `impl Serialize`.
/// - `Value::Array` and `Value::Map` recurse through the `Serialize`
///   trait (via the inner `Vec<Value>` / `HashMap<String, Value>`),
///   so nested values fold through this impl naturally without
///   allocating intermediate `serde_json::Value` trees.
impl serde::Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        match self {
            Value::Null => serializer.serialize_unit(),
            Value::Bool(b) => serializer.serialize_bool(*b),
            Value::Int(i) => serializer.serialize_i64(*i),
            Value::Float(f) => {
                if f.is_finite() {
                    serializer.serialize_f64(*f)
                } else {
                    // serde_json::Number cannot represent NaN/Inf — the
                    // pre-existing util::value_to_json lossy default was
                    // `0`. Mirror it here for behavioural continuity.
                    serializer.serialize_i64(0)
                }
            }
            Value::Str(s) => serializer.serialize_str(s),
            Value::Array(a) => {
                // Recurse through `Vec<Value>` — every element runs
                // through this same impl, no manual nesting needed.
                a.borrow().serialize(serializer)
            }
            Value::Tuple(t) => t.serialize(serializer),
            Value::List(l) => {
                let vec: Vec<_> = l.borrow().iter().cloned().collect();
                vec.serialize(serializer)
            }
            Value::Vector(v) => v.borrow().serialize(serializer),
            Value::Set(s) => s.borrow().serialize(serializer),
            Value::Map(m) => {
                // `Rc<RefCell<HashMap<String, Value>>>` — single borrow
                // at serialise time. HashMap iteration order is
                // documented as unspecified by serde_json (objects
                // are unordered in JSON itself), so non-deterministic
                // key order is acceptable.
                let b = m.borrow();
                let mut map = serializer.serialize_map(Some(b.len()))?;
                for (k, v) in b.iter() {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
            Value::Error(e) => {
                serializer.serialize_str(&format!("error({e})"))
            }
            Value::Bytes(b) => {
                serializer.serialize_str(&format!("<{} bytes>", b.len()))
            }
            Value::Handle(id) => {
                serializer.serialize_str(&format!("<handle {id}>"))
            }
            Value::Foreign(id) => {
                serializer.serialize_str(&format!("<foreign {id}>"))
            }
        }
    }
}

/// Canonical JSON inverse — the **single source of truth** for unpacking
/// serde forms back into CVM1 values. Acts as the exact canonical inverse
/// of the `impl serde::Serialize for Value` above. Used by
/// `json.parse`, `db.query` row hydration, the `message_bus.recv`
/// inverse path, and any future JSON-input consumer.
///
/// Properties worth preserving when editing (mirrors the Serialize impl):
///
/// - **Tagged-form precedence in `visit_str` / `visit_string`**: strings
///   that carry opaque payloads are recognised by exact prefix/suffix
///   and routed to the typed variant. Order matters:
///   1. `"<handle N>"` (angle-bracket + `handle` literal) — exact prefix.
///   2. `"<N bytes>"` (angle-bracket + ` bytes>` suffix) — exact suffix.
///   3. `"error(msg)"` (literal prefix + literal suffix) — the
///      visitor slice `v[6..v.len() - 1]` strips ONE outer wrap on
///      each side; this is **NOT** a balanced-paren walk. Boundary
///      inputs like `error((foo)` parse to `Error("(foo")` (4 chars,
///      inner-most opening paren preserved) and `error(foo))` parse
///      to `Error("foo)")` (4 chars, one trailing close preserved)
///      — asymmetries caused by the exact prefix/suffix strip.
///      Pinned CI-side at
///      `crush-lang-sdk/src/stdlib.rs::tests::test_json_parse_tagged_forms`
///      fixtures 4 and 5.
///   4. Fallback: `Value::Str(content)` — no disambiguation.
///   Specifying `<handle ` BEFORE `<... bytes>` resolves the
///   `<handle N>` vs `<N bytes>` overlap cleanly, because the `handle `
///   literal token is more specific than the generic `<` prefix.
///
/// - **Bytes round-trip**: `visit_str("\"<N bytes>\"")` reconstructs
///   zero-filled `Vec<u8>` of length N (same caveat as the Display /
///   Serialize impls — actual byte contents are not preserved through
///   the JSON wire format). Pinned CI-side at
///   `crush-vm/src/tests.rs::test_json_parse_bytes_lossy_round_trip_inline`
///   (cap-layer mirror at
///   `crush-lang-sdk/src/stdlib.rs::tests::test_json_parse_tagged_forms::fixture 6`).
///
/// - **`visit_map` builds a `HashMap<String, Value>`** with String
///   keys; serde-json ONLY emits String keys for objects, so no
///   type-mismatch possible. **This is the implicit fix for the
///   pre-existing `stdlib::json_to_value` bug** that mapped any JSON
///   Object to `Value::Null` — the canonical Deserialize impl routes
///   objects to `Value::Map` correctly.
///
/// - **Floats receive `visit_f64``; serde-json rejects non-finite
///   (NaN/Inf) at parse time, so the inverse of the Serialize lossy
///   `NaN/Inf → 0` map is unreachable from a valid input.
///   Serialize-side emits finite floats; Deserialize-side accepts
///   finite floats. Round-trip identity holds.
///
/// - **`deserialize_any`**: uses visitor type-erasure so the JSON
///   deserializer chooses the right `visit_*` method per token. This
///   is the canonical serde pattern — relying on `deserialize_any`
///   means the visitor handles ALL inputs uniformly without per-format
///   branching.
impl<'de> serde::Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ValueVisitor;

        impl<'de> serde::de::Visitor<'de> for ValueVisitor {
            type Value = Value;

            fn expecting(
                &self,
                formatter: &mut std::fmt::Formatter<'_>,
            ) -> std::fmt::Result {
                // Short hint — full enumeration lives in the
                // impl-level doc-comment above. Trim that redundancy
                // here so that serde error messages citing
                // `expecting()` stay readable when the deserializer
                // can't classify the input.
                formatter.write_str("any CVM1 value")
            }

            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(Value::Null)
            }

            fn visit_bool<E: serde::de::Error>(
                self,
                v: bool,
            ) -> Result<Self::Value, E> {
                Ok(Value::Bool(v))
            }

            fn visit_i64<E: serde::de::Error>(
                self,
                v: i64,
            ) -> Result<Self::Value, E> {
                Ok(Value::Int(v))
            }

            fn visit_u64<E: serde::de::Error>(
                self,
                v: u64,
            ) -> Result<Self::Value, E> {
                // `serde_json` only emits `visit_u64` for positive
                // integers exceeding i64::MAX — guard against overflow.
                if v <= i64::MAX as u64 {
                    Ok(Value::Int(v as i64))
                } else {
                    // `E::custom(...)` is the canonical serde idiom
                    // (use the bound `E` rather than `serde::de::Error::custom(...)`
                    // which relies on Rust inferring `Self` — sometimes
                    // fragile across inference versions).
                    Err(E::custom(format!(
                        "Value: u64 {v} exceeds i64::MAX"
                    )))
                }
            }

            fn visit_f64<E: serde::de::Error>(
                self,
                v: f64,
            ) -> Result<Self::Value, E> {
                Ok(Value::Float(v))
            }

            fn visit_str<E: serde::de::Error>(
                self,
                v: &str,
            ) -> Result<Self::Value, E> {
                // Tagged-form disambiguation — order matters:
                //   1. `<handle N>` (most specific prefix)
                //   2. `<N bytes>`   (general `<...>` shape)
                //   3. `error(msg)`
                //   4. Str fallback
                if v.starts_with("<handle ") && v.ends_with('>') {
                    if let Ok(id) = v[8..v.len() - 1].parse::<u64>() {
                        return Ok(Value::Handle(id));
                    }
                } else if v.starts_with("<foreign ") && v.ends_with('>') {
                    if let Ok(id) = v[9..v.len() - 1].parse::<u64>() {
                        return Ok(Value::Foreign(id));
                    }
                } else if v.starts_with('<') && v.ends_with(" bytes>") {
                    if let Ok(n) = v[1..v.len() - 7].parse::<usize>() {
                        // Documented caveat: Display/Serialize also
                        // preserve only length; reconstructed is
                        // zero-filled.
                        return Ok(Value::Bytes(vec![0; n]));
                    }
                } else if v.starts_with("error(") && v.ends_with(')') {
                    return Ok(Value::Error(v[6..v.len() - 1].to_string()));
                }
                Ok(Value::Str(v.to_string()))
            }

            fn visit_string<E: serde::de::Error>(
                self,
                v: String,
            ) -> Result<Self::Value, E> {
                // Defer to visit_str so the tagged-form logic stays
                // in one place. serde_json calls visit_string for
                // owned strings; routing through visit_str keeps the
                // disambiguation precedence consistent.
                self.visit_str(&v)
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut vec = Vec::new();
                while let Some(elem) = seq.next_element::<Value>()? {
                    vec.push(elem);
                }
                Ok(Value::new_array(vec))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut out = std::collections::HashMap::new();
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    out.insert(key, value);
                }
                Ok(Value::new_map(out))
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

/// Per-thread execution state for the green-thread scheduler.
pub struct GreenThread {
    pub ip: usize,
    pub stack: Vec<Value>,
    pub call_stack: Vec<Frame>,
    pub try_stack: Vec<usize>,
    pub steps: usize,
    pub done: bool,
    pub yielded: bool,
    pub waiting_for: Option<u64>,
    pub return_value: Option<Value>,
    pub out_parts: Vec<String>,
    pub out_len: usize,
}

impl GreenThread {
    pub fn new(ip: usize) -> Self {
        Self {
            ip,
            stack: Vec::new(),
            call_stack: vec![Frame { return_ip: None, memory: HashMap::new() }],
            try_stack: Vec::new(),
            steps: 0,
            done: false,
            yielded: false,
            waiting_for: None,
            return_value: None,
            out_parts: Vec::new(),
            out_len: 0,
        }
    }

    /// Create a new green thread with pre-loaded arguments on the stack.
    pub fn with_args(ip: usize, args: Vec<Value>) -> Self {
        Self {
            ip,
            stack: args,
            call_stack: vec![Frame { return_ip: None, memory: HashMap::new() }],
            try_stack: Vec::new(),
            steps: 0,
            done: false,
            yielded: false,
            waiting_for: None,
            return_value: None,
            out_parts: Vec::new(),
            out_len: 0,
        }
    }
}

impl Value {
    pub fn new_array(v: Vec<Value>) -> Self {
        Value::Array(std::rc::Rc::new(std::cell::RefCell::new(v)))
    }

    pub fn new_tuple(v: Vec<Value>) -> Self {
        Value::Tuple(v)
    }

    pub fn new_list(v: Vec<Value>) -> Self {
        Value::List(std::rc::Rc::new(std::cell::RefCell::new(v.into_iter().collect())))
    }

    pub fn new_vector(v: Vec<Value>) -> Self {
        Value::Vector(std::rc::Rc::new(std::cell::RefCell::new(v)))
    }

    pub fn new_set(v: Vec<Value>) -> Self {
        Value::Set(std::rc::Rc::new(std::cell::RefCell::new(v)))
    }

    pub fn new_map(m: std::collections::HashMap<String, Value>) -> Self {
        Value::Map(std::rc::Rc::new(std::cell::RefCell::new(m)))
    }
}

/// Execution resource limits.
#[derive(Debug, Clone)]
pub struct Quotas {
    pub max_steps: usize,
    pub max_stack: usize,
    pub max_output: usize,
    pub max_call_depth: usize,
    /// If set, further restricts the program's declared permissions.
    pub allowed_caps: Option<Vec<String>>,
    /// Wall-clock bound (milliseconds) for a single `EXEC_LANG` polyglot
    /// subprocess. `max_steps` bounds *instructions*, not real time — a
    /// capability that blocks on I/O (a hung `python3 -c`, a slow network
    /// call in a future capability) never executes another instruction to
    /// trip that quota, so it was previously unbounded. See
    /// `scheduler::run_with_wall_clock_limit` for the enforcement.
    ///
    /// `EXEC_LANG` enforces this by killing an OS subprocess at the deadline
    /// (see `scheduler::run_with_wall_clock_limit`) — true external
    /// preemption, possible because it owns a killable process.
    ///
    /// `CAP_CALL`'s generic `HostCap::call()` dispatch cannot be preempted
    /// the same way: `Value`'s `Rc<RefCell<...>>` fields aren't `Send`, so an
    /// arbitrary trait call can't safely be moved onto a watchdog thread
    /// without a much larger refactor (`Value` would need to become
    /// `Send`). Instead it is passed to `HostCap::call_with_deadline` so a
    /// capability that can legitimately block (network, provisioning)
    /// self-enforces this budget internally; a `HostCap` that never
    /// overrides `call_with_deadline` gets no bound at all (see CRUSH-19).
    pub max_wall_time_ms: u64,
}

impl Default for Quotas {
    fn default() -> Self {
        Self {
            max_steps: 1_000_000,
            max_stack: 4096,
            max_output: 1 << 20,
            max_call_depth: 256,
            allowed_caps: None,
            max_wall_time_ms: 30_000,
        }
    }
}

/// Result of a successful run (no VmError).
#[derive(Debug, Default)]
pub struct VmResult {
    pub output: String,
    pub steps: usize,
    pub halted: bool,
    pub stack: Vec<Value>,
}

pub struct Frame {
    pub return_ip: Option<usize>,
    pub memory: HashMap<u16, Value>,
}

/// Run a program with the built-in portable capability registry only.
pub fn run(program: &Program, quotas: &Quotas) -> Result<VmResult, VmError> {
    run_with_caps(program, quotas, None)
}

/// Run a program with optional host-provided capabilities.
pub fn run_with_caps(
    program: &Program,
    quotas: &Quotas,
    host_caps: Option<&HostCaps>,
) -> Result<VmResult, VmError> {
    crate::scheduler::run_scheduled(program, quotas, host_caps)
}

/// Run a program using the optimized FastVM architecture with empty capabilities.
#[cfg(feature = "native-plugins")]
pub fn run_fastvm(
    casm_program: &casm::Program,
) -> Result<crate::fastvm::FastYield, crate::fastvm::FastError> {
    run_fastvm_with_caps(casm_program, vec![])
}

/// Run a program using the optimized FastVM architecture with specified capabilities.
#[cfg(feature = "native-plugins")]
pub fn run_fastvm_with_caps(
    casm_program: &casm::Program,
    capabilities: Vec<std::sync::Arc<dyn crate::fastvm::Capability>>,
) -> Result<crate::fastvm::FastYield, crate::fastvm::FastError> {
    use std::sync::Arc;
    let lowered = crate::fastvm::lower_program(casm_program).map_err(|e| {
        crate::fastvm::FastError::ExecutionError(e.to_string())
    })?;
    
    // Create dummy HAL for now (since host calls are stubbed)
    let hal = Arc::new(DummyHal {});

    let mut vm = crate::fastvm::FastVM::new(lowered, capabilities, hal);
    
    // Give it a large budget to run to completion
    Ok(vm.run(1_000_000))
}

#[cfg(feature = "native-plugins")]
#[derive(Debug)]
struct DummyHal;
#[cfg(feature = "native-plugins")]
impl crate::fastvm::Hal for DummyHal {}

/// Deserialize CASM JSON bytes and execute via FastVM.
///
/// This is the entry point used by the `crush!` and `crush_file!` proc macros
/// from `crush-macros`. It takes pre-compiled CASM JSON bytes (embedded at
/// Rust compile time) and runs them through the FastVM hot path.
#[cfg(feature = "native-plugins")]
pub fn run_casm_json(
    json_bytes: &[u8],
) -> Result<crate::fastvm::FastYield, crate::fastvm::FastError> {
    let casm_program: casm::Program = serde_json::from_slice(json_bytes)
        .map_err(|e| crate::fastvm::FastError::ExecutionError(e.to_string()))?;
    run_fastvm(&casm_program)
}

// ── Convenience extractors for FastYield results ───────────────────────────

/// Extension trait providing ergonomic unwrap methods for Crush execution results.
///
/// Import this trait to call `.crush_unwrap_int()`, `.crush_unwrap_float()`, etc.
/// on `Result<FastYield, FastError>`:
///
/// ```ignore
/// use crush_vm::CrushResultExt;
/// let val: i64 = result.crush_unwrap_int();
/// ```
#[cfg(feature = "native-plugins")]
pub trait CrushResultExt {
    fn crush_unwrap_int(self) -> i64;
    fn crush_unwrap_float(self) -> f64;
    fn crush_unwrap_bool(self) -> bool;
    fn crush_unwrap_string(self) -> String;
    fn crush_is_null(&self) -> bool;
}

#[cfg(feature = "native-plugins")]
impl CrushResultExt for Result<crate::fastvm::FastYield, crate::fastvm::FastError> {
    fn crush_unwrap_int(self) -> i64 {
        match self.expect("Crush execution failed") {
            crate::fastvm::FastYield::Finished(Some(crate::RuntimeValue::Int(v))) => v,
            crate::fastvm::FastYield::Value(crate::RuntimeValue::Int(v)) => v,
            other => panic!("Expected Crush int, got {:?}", other),
        }
    }

    fn crush_unwrap_float(self) -> f64 {
        match self.expect("Crush execution failed") {
            crate::fastvm::FastYield::Finished(Some(crate::RuntimeValue::Float(v))) => v,
            crate::fastvm::FastYield::Value(crate::RuntimeValue::Float(v)) => v,
            other => panic!("Expected Crush float, got {:?}", other),
        }
    }

    fn crush_unwrap_bool(self) -> bool {
        match self.expect("Crush execution failed") {
            crate::fastvm::FastYield::Finished(Some(crate::RuntimeValue::Bool(v))) => v,
            crate::fastvm::FastYield::Value(crate::RuntimeValue::Bool(v)) => v,
            other => panic!("Expected Crush bool, got {:?}", other),
        }
    }

    fn crush_unwrap_string(self) -> String {
        match self.expect("Crush execution failed") {
            crate::fastvm::FastYield::Finished(Some(crate::RuntimeValue::String(v))) => v,
            crate::fastvm::FastYield::Value(crate::RuntimeValue::String(v)) => v,
            other => panic!("Expected Crush string, got {:?}", other),
        }
    }

    fn crush_is_null(&self) -> bool {
        match self {
            Ok(crate::fastvm::FastYield::Finished(None))
            | Ok(crate::fastvm::FastYield::Finished(Some(crate::RuntimeValue::Null))) => true,
            _ => false,
        }
    }
}

