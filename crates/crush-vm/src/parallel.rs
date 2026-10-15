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
//! closure can read it. Changing anything captured — a captured variable, one
//! captured by a closure it captured (transitively), or a captured array/map
//! in place — would be lost (nothing comes back from a worker), so such an
//! item fails loudly (`check_captured`), like a rayon closure that isn't `Fn`.
//! The item itself is the worker's own copy and may be changed freely.
//!
//! Budgets: each item runs with the caller's quotas — its own instruction and
//! output budget, independent of the other items and of the caller's own step
//! count; the items' output, appended to the caller's, counts against the
//! caller's output quota.
//!
//! The native scheduler runs the items on threads; the portable VM (wasm, no
//! threads) runs the same jobs one after another. This module is the part both
//! share: moving values, building each item's job, collecting the results.

use crate::vm::{FuncValue, Value, VmError};

/// A value that can cross a thread: an owned deep copy of a crush [`Value`].
/// Cells carry an id, so cells shared between captured closures (or a closure
/// that captures its own variable) are rebuilt shared — and cycles end.
#[derive(Debug, Clone, PartialEq)]
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
    /// A captured variable's cell, first time seen: its id and a copy of its value.
    Cell(usize, Box<Moved>),
    /// A cell already moved (by id).
    CellRef(usize),
}

/// Deep nesting beyond this (a cyclic array, say) is refused, not a crash.
const MAX_DEPTH: usize = 10_000;

/// Moves values out of one VM. One `Mover` per job, so cells shared between
/// the values it moves stay shared.
#[derive(Default)]
pub struct Mover {
    cells: std::collections::HashMap<*const std::cell::RefCell<Value>, usize>,
}

impl Mover {
    /// Deep-copy `v` for a worker; task and foreign handles belong to this VM
    /// and are refused.
    pub fn out(&mut self, v: &Value) -> Result<Moved, VmError> {
        self.out_at(v, 0)
    }

    fn out_at(&mut self, v: &Value, depth: usize) -> Result<Moved, VmError> {
        if depth > MAX_DEPTH {
            return Err(VmError::Parallel(
                "a value is nested too deeply (or is cyclic) to move into a parallel_map worker"
                    .to_string(),
            ));
        }
        let d = depth + 1;
        let mut seq = |m: &mut Self, xs: &[Value]| xs.iter().map(|x| m.out_at(x, d)).collect::<Result<Vec<_>, _>>();
        Ok(match v {
            Value::Null => Moved::Null,
            Value::Bool(b) => Moved::Bool(*b),
            Value::Int(i) => Moved::Int(*i),
            Value::Float(f) => Moved::Float(*f),
            Value::Str(s) => Moved::Str(s.clone()),
            Value::Array(a) => Moved::Array(seq(self, &a.borrow())?),
            Value::Tuple(t) => Moved::Tuple(seq(self, t)?),
            Value::List(l) => Moved::List(l.borrow().iter().map(|x| self.out_at(x, d)).collect::<Result<_, _>>()?),
            Value::Vector(a) => Moved::Vector(seq(self, &a.borrow())?),
            Value::Set(a) => Moved::Set(seq(self, &a.borrow())?),
            Value::Map(m) => Moved::Map(
                m.borrow()
                    .iter()
                    .map(|(k, v)| Ok((k.clone(), self.out_at(v, d)?)))
                    .collect::<Result<_, VmError>>()?,
            ),
            Value::Error(e) => Moved::Error(e.clone()),
            Value::Bytes(b) => Moved::Bytes(b.clone()),
            Value::Func(f) => Moved::Func {
                name: f.name.clone(),
                arity: f.arity,
                captured: seq(self, &f.captured)?,
            },
            Value::Cell(c) => {
                let key = std::rc::Rc::as_ptr(c);
                if let Some(&id) = self.cells.get(&key) {
                    return Ok(Moved::CellRef(id));
                }
                let id = self.cells.len();
                self.cells.insert(key, id);
                Moved::Cell(id, Box::new(self.out_at(&c.borrow(), d)?))
            }
            Value::Handle(_) | Value::Foreign(_) => {
                return Err(VmError::Parallel(format!(
                    "a {} can't be moved into a parallel_map worker",
                    v.type_name()
                )));
            }
        })
    }
}

/// Moves values into one VM (the inverse of [`Mover`]).
#[derive(Default)]
pub struct Unmover {
    cells: std::collections::HashMap<usize, std::rc::Rc<std::cell::RefCell<Value>>>,
}

impl Unmover {
    pub fn into_value(&mut self, m: Moved) -> Value {
        let shared = |u: &mut Self, xs: Vec<Moved>| {
            std::rc::Rc::new(std::cell::RefCell::new(xs.into_iter().map(|x| u.into_value(x)).collect()))
        };
        match m {
            Moved::Null => Value::Null,
            Moved::Bool(b) => Value::Bool(b),
            Moved::Int(i) => Value::Int(i),
            Moved::Float(f) => Value::Float(f),
            Moved::Str(s) => Value::Str(s),
            Moved::Array(xs) => Value::Array(shared(self, xs)),
            Moved::Tuple(xs) => Value::Tuple(xs.into_iter().map(|x| self.into_value(x)).collect()),
            Moved::List(xs) => Value::List(std::rc::Rc::new(std::cell::RefCell::new(
                xs.into_iter().map(|x| self.into_value(x)).collect(),
            ))),
            Moved::Vector(xs) => Value::Vector(shared(self, xs)),
            Moved::Set(xs) => Value::Set(shared(self, xs)),
            Moved::Map(entries) => {
                let entries: Vec<(String, Value)> = entries.into_iter().map(|(k, v)| (k, self.into_value(v))).collect();
                Value::new_map(entries)
            }
            Moved::Error(e) => Value::Error(e),
            Moved::Bytes(b) => Value::Bytes(b),
            Moved::Func { name, arity, captured } => Value::Func(std::rc::Rc::new(FuncValue {
                name,
                arity,
                captured: captured.into_iter().map(|x| self.into_value(x)).collect(),
            })),
            Moved::Cell(id, value) => {
                // Registered before its value is rebuilt, so a reference back to
                // it (a closure capturing its own variable) finds it.
                let cell = std::rc::Rc::new(std::cell::RefCell::new(Value::Null));
                self.cells.insert(id, cell.clone());
                *cell.borrow_mut() = self.into_value(*value);
                Value::Cell(cell)
            }
            Moved::CellRef(id) => Value::Cell(self.cells.get(&id).cloned().unwrap_or_else(|| {
                std::rc::Rc::new(std::cell::RefCell::new(Value::Null))
            })),
        }
    }
}

/// One-off deep copy (a result coming back from a worker).
pub fn move_out(v: &Value) -> Result<Moved, VmError> {
    Mover::default().out(v)
}

/// One-off rebuild.
pub fn move_in(m: Moved) -> Value {
    Unmover::default().into_value(m)
}

/// One item's work: call `function` with the item and the closure's captured
/// values (moved together, so shared cells stay shared). On the stack the
/// captured values go on top — they are the leading parameters.
#[derive(Debug, Clone)]
pub struct Job {
    pub function: String,
    pub item: Moved,
    /// In parameter order; also the snapshot `check_captured` compares with.
    pub captured: Vec<Moved>,
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
    items
        .iter()
        .map(|item| {
            let mut mover = Mover::default();
            let captured = func.captured.iter().map(|c| mover.out(c)).collect::<Result<Vec<_>, _>>()?;
            Ok(Job { function: func.name.clone(), item: mover.out(item)?, captured })
        })
        .collect()
}

/// A worker's starting stack (bottom → top: the item, then the captured
/// values, first one on top), and the captured values it starts with.
pub fn start_stack(job: &Job) -> (Vec<Value>, Vec<Value>) {
    let mut unmover = Unmover::default();
    let captured: Vec<Value> = job.captured.iter().cloned().map(|c| unmover.into_value(c)).collect();
    let item = unmover.into_value(job.item.clone());
    let mut stack = vec![item];
    stack.extend(captured.iter().rev().cloned());
    (stack, captured)
}

/// After an item ran: did it change anything it captured — a captured
/// variable, a variable captured by a closure it captured (transitively), or a
/// captured array or map in place? Nothing comes back from a worker, so the
/// change would be lost: that is an error, not a silent difference from a
/// sequential map.
pub fn check_captured(job: &Job, captured_after: &[Value]) -> Result<(), VmError> {
    let mut mover = Mover::default();
    let after = captured_after.iter().map(|c| mover.out(c)).collect::<Result<Vec<_>, _>>()?;
    if after != job.captured {
        return Err(VmError::Parallel(
            "the function changed something it captured (a variable it or a function it calls \
             captured, or a captured array/map); a parallel_map worker shares nothing, so the \
             change would be lost — return the value instead"
                .to_string(),
        ));
    }
    Ok(())
}

/// A worker's error, as the caller sees it.
pub fn worker_error(error: VmError) -> VmError {
    match error {
        // Workers get no host capabilities (fs, net, process, …) and no
        // privileged ones: the main VM may well have them, so say where the
        // capability is missing rather than "unknown capability".
        VmError::UnknownCap(cap) => VmError::Parallel(format!(
            "`{cap}` isn't available inside parallel_map workers (only the VM's built-in, pure \
             capabilities run there; host capabilities such as fs, net and process run only on \
             the main VM)"
        )),
        VmError::CapNotDeclared(cap) if crate::caps::is_privileged(&cap) => VmError::Parallel(format!(
            "`{cap}` isn't available inside parallel_map workers (privileged capabilities run \
             only on the main VM)"
        )),
        other => other,
    }
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
                return Err(VmError::Parallel(format!("item {index}: {}", worker_error(error))));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_privileged_capability_in_a_worker_says_why() {
        let e = worker_error(VmError::UnknownCap("fs.cat".to_string())).to_string();
        assert!(e.contains("`fs.cat` isn't available inside parallel_map workers"), "{e}");
        let e = worker_error(VmError::DivByZero).to_string();
        assert_eq!(e, "division by zero");
    }

    #[test]
    fn shared_and_self_referencing_cells_survive_a_move() {
        let cell = std::rc::Rc::new(std::cell::RefCell::new(Value::Null));
        let f = Value::Func(std::rc::Rc::new(FuncValue {
            name: "f".into(),
            arity: 0,
            captured: vec![Value::Cell(cell.clone())],
        }));
        *cell.borrow_mut() = f.clone(); // f captures a cell holding f
        let mut mover = Mover::default();
        let moved = vec![mover.out(&f).unwrap(), mover.out(&Value::Cell(cell)).unwrap()];
        let mut unmover = Unmover::default();
        let back: Vec<Value> = moved.into_iter().map(|m| unmover.into_value(m)).collect();
        let (Value::Func(f2), Value::Cell(c2)) = (&back[0], &back[1]) else { panic!("{back:?}") };
        let Value::Cell(inner) = &f2.captured[0] else { panic!() };
        assert!(std::rc::Rc::ptr_eq(inner, c2), "the shared cell must stay shared");
    }
}
