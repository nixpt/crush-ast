//! `parallel_map(items, f)` — data parallelism (#175, CRUSH-251 step 5).
//!
//! Decided design (#185): **share-nothing**. Each item runs `f(item)` in its
//! own VM; the item and the closure's captured values are *moved* (deep-copied)
//! in and the result is moved back, so no `Rc` crosses a thread. Results come
//! back in input order, each item's printed output is emitted in input order,
//! and the first error **in item order** wins — so a run on threads and a
//! sequential run are indistinguishable. Inside a worker only non-privileged
//! capabilities are available, and a nested `parallel_map` runs sequentially.
//!
//! A captured variable (a cell) moves in as a copy of its current value, so a
//! closure can read it. Writing to it would be lost — nothing comes back from
//! a worker — so an item whose closure assigned a captured variable fails
//! loudly (`check_captured`), like a rayon closure that isn't `Fn`.
//!
//! The native scheduler runs the items on threads; the portable VM (wasm, no
//! threads) runs the same jobs one after another. This module is the part both
//! share: moving values, building each item's job, collecting the results.

use crate::vm::{FuncValue, Value, VmError};

/// A value that can cross a thread: an owned deep copy of a crush [`Value`].
#[derive(Debug, Clone)]
pub enum Moved {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<Moved>),
    Tuple(Vec<Moved>),
    List(Vec<Moved>),
    Vector(Vec<Moved>),
    Set(Vec<Moved>),
    Map(Vec<(String, Moved)>),
    Error(String),
    Bytes(Vec<u8>),
    Func { name: String, arity: u8, captured: Vec<Moved> },
    /// A captured variable's cell: moved as a copy of its current value.
    Cell(Box<Moved>),
}

/// Deep-copy `v` for a worker. A cell is copied as its current value (see the
/// module docs); task and foreign handles belong to this VM and are refused.
pub fn move_out(v: &Value) -> Result<Moved, VmError> {
    let seq = |xs: &[Value]| xs.iter().map(move_out).collect::<Result<Vec<_>, _>>();
    Ok(match v {
        Value::Null => Moved::Null,
        Value::Bool(b) => Moved::Bool(*b),
        Value::Int(i) => Moved::Int(*i),
        Value::Float(f) => Moved::Float(*f),
        Value::Str(s) => Moved::Str(s.clone()),
        Value::Array(a) => Moved::Array(seq(&a.borrow())?),
        Value::Tuple(t) => Moved::Tuple(seq(t)?),
        Value::List(l) => Moved::List(l.borrow().iter().map(move_out).collect::<Result<_, _>>()?),
        Value::Vector(a) => Moved::Vector(seq(&a.borrow())?),
        Value::Set(a) => Moved::Set(seq(&a.borrow())?),
        Value::Map(m) => Moved::Map(
            m.borrow()
                .iter()
                .map(|(k, v)| Ok((k.clone(), move_out(v)?)))
                .collect::<Result<_, VmError>>()?,
        ),
        Value::Error(e) => Moved::Error(e.clone()),
        Value::Bytes(b) => Moved::Bytes(b.clone()),
        Value::Func(f) => Moved::Func {
            name: f.name.clone(),
            arity: f.arity,
            captured: seq(&f.captured)?,
        },
        Value::Cell(c) => Moved::Cell(Box::new(move_out(&c.borrow())?)),
        Value::Handle(_) | Value::Foreign(_) => {
            return Err(VmError::Parallel(format!(
                "a {} can't be moved into a parallel_map worker",
                v.type_name()
            )));
        }
    })
}

/// Rebuild a [`Value`] in the receiving VM.
pub fn move_in(m: Moved) -> Value {
    let seq = |xs: Vec<Moved>| xs.into_iter().map(move_in).collect::<Vec<_>>();
    let shared = |xs: Vec<Moved>| std::rc::Rc::new(std::cell::RefCell::new(seq(xs)));
    match m {
        Moved::Null => Value::Null,
        Moved::Bool(b) => Value::Bool(b),
        Moved::Int(i) => Value::Int(i),
        Moved::Float(f) => Value::Float(f),
        Moved::Str(s) => Value::Str(s),
        Moved::Array(xs) => Value::Array(shared(xs)),
        Moved::Tuple(xs) => Value::Tuple(seq(xs)),
        Moved::List(xs) => Value::List(std::rc::Rc::new(std::cell::RefCell::new(
            xs.into_iter().map(move_in).collect(),
        ))),
        Moved::Vector(xs) => Value::Vector(shared(xs)),
        Moved::Set(xs) => Value::Set(shared(xs)),
        Moved::Map(entries) => Value::new_map(entries.into_iter().map(|(k, v)| (k, move_in(v)))),
        Moved::Error(e) => Value::Error(e),
        Moved::Bytes(b) => Value::Bytes(b),
        Moved::Func { name, arity, captured } => Value::Func(std::rc::Rc::new(FuncValue {
            name,
            arity,
            captured: seq(captured),
        })),
        Moved::Cell(v) => Value::Cell(std::rc::Rc::new(std::cell::RefCell::new(move_in(*v)))),
    }
}

/// A worker's starting stack, plus the cells of the closure's captured
/// variables with the values they started with (for [`check_captured`]).
pub fn start_stack(job: &Job) -> (Vec<Value>, Vec<(Value, Value)>) {
    let stack: Vec<Value> = job.stack.iter().cloned().map(move_in).collect();
    let cells = stack
        .iter()
        .filter_map(|v| match v {
            Value::Cell(c) => Some((v.clone(), c.borrow().clone())),
            _ => None,
        })
        .collect();
    (stack, cells)
}

/// After an item ran: did its closure assign a captured variable? The write
/// can't reach the caller (workers share nothing), so it is an error rather
/// than silently lost.
pub fn check_captured(cells: &[(Value, Value)]) -> Result<(), VmError> {
    for (cell, before) in cells {
        if let Value::Cell(c) = cell
            && *c.borrow() != *before
        {
            return Err(VmError::Parallel(
                "the function assigned a variable it captured; a parallel_map worker shares \
                 nothing, so the write would be lost — return the value instead"
                    .to_string(),
            ));
        }
    }
    Ok(())
}

/// One item's work: call `function` with this stack (bottom → top; the top is
/// the first parameter, as for `CALL`).
#[derive(Debug, Clone)]
pub struct Job {
    pub function: String,
    pub stack: Vec<Moved>,
}

/// What a worker reports for one item.
pub type ItemResult = Result<(String, Moved), VmError>;

/// Check `parallel_map`'s arguments and build one job per item.
pub fn jobs(args: &[Value]) -> Result<Vec<Job>, VmError> {
    let [items, f] = args else {
        return Err(VmError::CapArity { cap: "parallel_map".to_string(), expected: 2, got: args.len() });
    };
    let items: Vec<Value> = match items {
        Value::Array(a) | Value::Vector(a) => a.borrow().clone(),
        Value::Tuple(t) => t.clone(),
        other => return Err(VmError::TypeError { expected: "array", got: other.type_name() }),
    };
    let func = crate::vm::callable(f, 1)?;
    let captured: Vec<Moved> = func.captured.iter().map(move_out).collect::<Result<_, _>>()?;
    items
        .iter()
        .map(|item| {
            // Captured values are the leading parameters: they go on top.
            let mut stack = vec![move_out(item)?];
            stack.extend(captured.iter().rev().cloned());
            Ok(Job { function: func.name.clone(), stack })
        })
        .collect()
}

/// Results in input order: the first error *in item order* wins; otherwise
/// the items' output (concatenated in order) and their results as an array.
pub fn collect(results: Vec<ItemResult>) -> Result<(String, Value), VmError> {
    let mut output = String::new();
    let mut values = Vec::with_capacity(results.len());
    for (index, result) in results.into_iter().enumerate() {
        match result {
            Ok((out, value)) => {
                output.push_str(&out);
                values.push(move_in(value));
            }
            Err(error) => {
                return Err(VmError::Parallel(format!("item {index}: {error}")));
            }
        }
    }
    Ok((output, Value::new_array(values)))
}

/// The program a worker runs: the same code, but only non-privileged
/// capabilities declared (no fs / net / process inside a worker yet).
pub fn worker_program(program: &crate::bytecode::Program) -> crate::bytecode::Program {
    let mut program = program.clone();
    program.manifest.permissions.retain(|cap| !crate::caps::is_privileged(cap));
    program
}

thread_local! {
    /// Set while this thread runs a parallel_map item: a nested parallel_map
    /// runs its items sequentially instead of starting more threads.
    pub static IN_WORKER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
