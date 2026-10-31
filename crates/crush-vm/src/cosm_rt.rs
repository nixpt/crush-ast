//! COSM K1.4 (CRUSH-zorro-43): Records and Kinds — the object runtime shared
//! by the scheduler VM and the portable VM.
//!
//! A program's kind table (its COSM `KindDecl`s, CRUSH-zorro-42) becomes a
//! [`KindTable`] when the program loads: one [`Kind`] per declaration, with
//! its resolution order precomputed (`crush_cosm::resolution_order`, which is
//! `crush_cosm::c3_mro` for a `c3` Kind). An instance is a
//! `Value::Record(Rc<RecordValue>)` pointing at its Kind; the generic member
//! ops (`GET_MEMBER`, `SET_MEMBER`, `HAS_MEMBER`, `DEL_MEMBER`,
//! `CALL_MEMBER`, `CONSTRUCT`, `GET_SLOT`/`SET_SLOT`, `IS_KIND`, `COPY`) ask
//! the Record's Kind what to do. Everything here is VM-independent; the two
//! VMs only differ in how they perform a call ([`Callee`]).
//!
//! **Layout.** A Record's declared fields (sealed fields, or Python
//! `__slots__`) are slots in declaration order; `open_dict` storage adds an
//! overflow map. A slot is unset until written (a map without that key), and
//! a Record prints its fields in the order they were first written, then its
//! overflow attributes — exactly how the map a struct used to be printed, so
//! no existing program's output changes.
//!
//! **Lookup** (`GET_MEMBER`, `CALL_MEMBER`): the instance's own fields, then,
//! for each Kind in the resolution order, its inherent members, the members
//! of its conformances' impl tables, and its interfaces' default methods. Two
//! conformances (or two defaults) of one Kind providing the same name is an
//! ambiguity error, as rustc reports it.
//!
//! **Receivers.** `explicit_ref` / `explicit_ref_mut` / `dynamic_this` /
//! `bound_self` pass the Record itself as the method's first parameter;
//! `explicit_value` passes a copy for a `value(…)` Kind; `bound_cls` passes
//! the Kind. `GET_MEMBER` of a method returns a function value: bound to the
//! instance (`bound_self`) or the Kind (`bound_cls`), the plain function for
//! the explicit receivers (the caller passes the receiver, as Rust's
//! `Point::norm2` path does), and, under `dynamic_this`, a function whose
//! `this` is `null` — `const g = o.m; g()` loses `this`, as in JavaScript.
//!
//! **Value semantics.** `value(copy)`: the compiler emits `COPY` on
//! assignment, argument passing and `return`; [`copy_value`] copies the
//! Record (and the `value(…)` Records inside it), sharing everything else.
//! `value(move)` behaves exactly like `copy` at run time: use-after-move is a
//! compile-time check (Rust's), and without it a move and a copy can't be
//! told apart by a correct program. `clone_explicit` copies only through
//! `clone()`. `clone()` deep-copies (or calls the Kind's `clone` hook).
//!
//! **Equality.** `structural` compares the set fields, ignoring the Kind and
//! the order, as maps compare (so a struct still equals the map literal it
//! used to be); `identity` compares the instance.
//!
//! **Memory.** Kinds are immortal per run; instance cycles leak until
//! CRUSH-121, as the design accepts through K2.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crush_cosm::{
    Const, Construct, CosmModule, Equality, FnRef, HookImpl, InterfaceDecl, KindDecl, KindRef, Member, Receiver,
    Semantics, StaticValue, ValueMode,
};

use crate::vm::{FuncValue, MapStore, Value, VmError};

/// How the scheduler VM and the portable VM name themselves in a refusal.
pub(crate) const ENGINE: &str = "this VM";

/// A Kind at run time.
#[derive(Debug)]
pub struct Kind {
    /// Resolution order, starting with this Kind's own declaration.
    mro: Vec<Rc<KindDecl>>,
    /// The interfaces this Kind's resolution order conforms to (for default
    /// methods), by name.
    interfaces: Rc<HashMap<String, InterfaceDecl>>,
    /// Arity of every program function, for function values made from members.
    arity: Rc<HashMap<String, u8>>,
    /// Class-level values written at run time (`C.x = v` in JS): a
    /// declared static constant is only the initial value, and the map
    /// is per loaded program.
    statics: RefCell<std::collections::HashMap<String, Value>>,
    /// This Kind and its ancestors as loaded Kinds, in resolution
    /// order (filled in by `KindTable::load`): a class-level value is
    /// read through the chain until a nearer Kind writes its own
    /// (CRUSH-285 review #3).
    chain: RefCell<Vec<Rc<Kind>>>,
}

impl Kind {
    /// The Kind's declaration.
    pub fn decl(&self) -> &KindDecl {
        &self.mro[0]
    }

    pub fn name(&self) -> &str {
        &self.mro[0].name
    }

    /// The names of the Kinds in the resolution order, this Kind first.
    pub fn resolution_order(&self) -> impl Iterator<Item = &str> {
        self.mro.iter().map(|k| k.name.as_str())
    }

    /// Whether `name` is this Kind or one of its ancestors.
    pub fn is_a(&self, name: &str) -> bool {
        self.resolution_order().any(|k| k == name)
    }

    /// A class-level value written at run time, if any.
    fn static_value(&self, name: &str) -> Option<Value> {
        self.statics.borrow().get(name).cloned()
    }

    /// A class-level value along the resolution order, walking the
    /// chain once: the first Kind that either wrote `name` at run time
    /// or declares it as a static wins. An ancestor's run-time write
    /// must not shadow a nearer Kind's own declared static (JS: `B.n`
    /// is `B`'s own `static n = 1` even after `A.n = 5`); an ancestor's
    /// write *is* read when no nearer Kind declares the name (JS:
    /// `B.n` reads `A.n` until `B` writes its own).
    fn static_in_chain(&self, name: &str) -> Result<Option<Value>, VmError> {
        for k in self.chain.borrow().iter() {
            if let Some(v) = k.static_value(name) {
                return Ok(Some(v));
            }
            if let Some(m) = k.decl().member(name) {
                match &m.member {
                    Member::Static { value: StaticValue::Const(c) } => {
                        return Ok(Some(const_value(c)));
                    }
                    Member::Static { value: StaticValue::Fn(f) } => {
                        return Ok(Some(func_value(
                            f,
                            self.arity_of(f.as_str())?,
                            Vec::new(),
                        )));
                    }
                    _ => {}
                }
            }
        }
        Ok(None)
    }

    /// Write a class-level value (`C.x = v`).
    fn set_static(&self, name: &str, v: Value) {
        self.statics.borrow_mut().insert(name.to_string(), v);
    }

    fn is_value(&self) -> bool {
        matches!(self.decl().semantics, Semantics::Value(ValueMode::Copy | ValueMode::Move))
    }

    fn fields(&self) -> &[crush_cosm::FieldDecl] {
        self.decl().declared_fields()
    }

    fn slot(&self, name: &str) -> Option<usize> {
        self.decl().field_slot(name)
    }

    fn arity_of(&self, func: &str) -> Result<u8, VmError> {
        self.arity.get(func).copied().ok_or_else(|| {
            VmError::Cosm(format!(
                "`{}` names the function `{func}`, which the program doesn't define (or whose arity it doesn't record)",
                self.name()
            ))
        })
    }

    /// The first `str` hook, else the first `repr` hook, along the
    /// resolution order — what printing calls. `None` prints the default.
    pub fn print_hook(&self) -> Option<&FnRef> {
        let hook = |pick: fn(&crush_cosm::Hooks) -> &Option<HookImpl>| {
            self.mro.iter().find_map(|k| match pick(&k.hooks) {
                Some(HookImpl::Fn(f)) => Some(f),
                _ => None,
            })
        };
        hook(|h| &h.str).or_else(|| hook(|h| &h.repr))
    }
}

/// Every Kind of a loaded program, by name.
#[derive(Debug, Default)]
pub struct KindTable {
    kinds: HashMap<String, Rc<Kind>>,
    /// Whether any Kind has a `str` / `repr` hook: printing only looks for
    /// hooked Records (walking the printed value) when one does.
    print_hooks: bool,
}

impl KindTable {
    /// Build the Kinds of `program`'s kind table (empty without one): the
    /// shared COSM rule at the Kinds level (`casm::cosm_unsupported_reason_on`)
    /// first, then each Kind's resolution order. A program whose table fails
    /// either is refused before it runs.
    pub fn load(program: &crate::bytecode::Program) -> Result<Self, VmError> {
        let Some(module) = program.kind_table() else {
            return Ok(Self::default());
        };
        crate::cosm_gate::check_runnable(Some(module))?;
        let arity: Rc<HashMap<String, u8>> = Rc::new(
            program
                .manifest
                .functions
                .iter()
                .filter_map(|(name, f)| f.arity.map(|a| (name.clone(), a)))
                .collect(),
        );
        let interfaces: Rc<HashMap<String, InterfaceDecl>> =
            Rc::new(module.interfaces.iter().map(|i| (i.name.clone(), i.clone())).collect());
        let decls: HashMap<&str, Rc<KindDecl>> =
            module.kinds.iter().map(|k| (k.name.as_str(), Rc::new(k.clone()))).collect();
        let mut kinds = HashMap::new();
        for k in &module.kinds {
            let order = resolution_order(module, &k.name)?;
            let mro = order
                .iter()
                .map(|r| {
                    decls.get(r.as_str()).cloned().ok_or_else(|| {
                        VmError::Cosm(format!("`{}` inherits from `{r}`, which isn't declared", k.name))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            kinds.insert(
                k.name.clone(),
                Rc::new(Kind {
                    mro,
                    interfaces: interfaces.clone(),
                    arity: arity.clone(),
                    statics: RefCell::new(std::collections::HashMap::new()),
                    chain: RefCell::new(Vec::new()),
                }),
            );
        }
        // The resolution order as loaded Kinds, for class-level values.
        for k in kinds.values() {
            let chain: Vec<Rc<Kind>> = k
                .resolution_order()
                .filter_map(|n| kinds.get(n).cloned())
                .collect();
            *k.chain.borrow_mut() = chain;
        }
        let print_hooks = kinds.values().any(|k| k.print_hook().is_some());
        Ok(Self { kinds, print_hooks })
    }

    /// Whether any Kind prints through a `str` / `repr` hook.
    pub fn has_print_hooks(&self) -> bool {
        self.print_hooks
    }

    pub fn get(&self, name: &str) -> Result<&Rc<Kind>, VmError> {
        self.kinds
            .get(name)
            .ok_or_else(|| VmError::Cosm(format!("`{name}` is not a Kind of this program")))
    }
}

fn resolution_order(module: &CosmModule, name: &str) -> Result<Vec<KindRef>, VmError> {
    crush_cosm::resolution_order(&KindRef::new(name), module)
        .map_err(|e| VmError::Cosm(format!("the resolution order of `{name}`: {e}")))
}

/// An instance of a Kind.
#[derive(Debug)]
pub struct RecordValue {
    pub kind: Rc<Kind>,
    state: RefCell<RecordState>,
}

#[derive(Debug, Clone, Default)]
struct RecordState {
    /// The declared fields, by slot; `None` until written.
    slots: Vec<Option<Value>>,
    /// Slots in the order they were first written: the print order.
    order: Vec<usize>,
    /// Attributes outside the declared layout (`open_dict` storage only).
    extra: MapStore,
}

impl RecordValue {
    /// A fresh instance: every slot unset, except fields with a default.
    pub fn new(kind: &Rc<Kind>) -> Self {
        let mut state = RecordState { slots: vec![None; kind.fields().len()], ..Default::default() };
        for (i, f) in kind.fields().iter().enumerate() {
            if let Some(c) = &f.default {
                state.slots[i] = Some(const_value(c));
                state.order.push(i);
            }
        }
        Self { kind: kind.clone(), state: RefCell::new(state) }
    }

    fn with_state(kind: &Rc<Kind>, state: RecordState) -> Self {
        Self { kind: kind.clone(), state: RefCell::new(state) }
    }

    /// The set fields in print order: declared fields in the order they were
    /// first written, then the overflow attributes in insertion order.
    pub fn fields(&self) -> Vec<(String, Value)> {
        let s = self.state.borrow();
        let decl = self.kind.fields();
        let mut out: Vec<(String, Value)> = s
            .order
            .iter()
            .filter_map(|&i| s.slots[i].clone().map(|v| (decl[i].name.clone(), v)))
            .collect();
        out.extend(s.extra.iter().map(|(k, v)| (k.clone(), v.clone())));
        out
    }

    /// The set fields as a fresh map (what a capability sees).
    pub fn to_map(&self) -> Value {
        Value::new_map(self.fields())
    }

    /// The value of an instance field, if set.
    pub fn field(&self, name: &str) -> Option<Value> {
        let s = self.state.borrow();
        match self.kind.slot(name) {
            Some(i) => s.slots[i].clone(),
            None => s.extra.get(name).cloned(),
        }
    }

    pub fn slot(&self, i: usize) -> Result<Value, VmError> {
        let s = self.state.borrow();
        match s.slots.get(i) {
            Some(v) => Ok(v.clone().unwrap_or(Value::Null)),
            None => Err(self.no_slot(i)),
        }
    }

    pub fn set_slot(&self, i: usize, v: Value) -> Result<(), VmError> {
        let mut s = self.state.borrow_mut();
        if i >= s.slots.len() {
            drop(s);
            return Err(self.no_slot(i));
        }
        if s.slots[i].is_none() {
            s.order.push(i);
        }
        s.slots[i] = Some(v);
        Ok(())
    }

    fn no_slot(&self, i: usize) -> VmError {
        VmError::Cosm(format!("`{}` has {} slot(s); there is no slot {i}", self.kind.name(), self.kind.fields().len()))
    }

    /// Write an instance field: a declared slot, or (open storage) an
    /// overflow attribute. A sealed Kind has no room for anything else.
    pub fn set_field(&self, name: &str, v: Value) -> Result<(), VmError> {
        if let Some(i) = self.kind.slot(name) {
            return self.set_slot(i, v);
        }
        if !self.kind.decl().storage.is_open() {
            let fields: Vec<&str> = self.kind.fields().iter().map(|f| f.name.as_str()).collect();
            return Err(VmError::Cosm(casm::sealed_field_error(self.kind.name(), name, &fields)));
        }
        self.state.borrow_mut().extra.insert(name.to_string(), v);
        Ok(())
    }

    fn has_field(&self, name: &str) -> bool {
        self.field(name).is_some()
    }

    fn del_field(&self, name: &str) -> Result<(), VmError> {
        let mut s = self.state.borrow_mut();
        if let Some(i) = self.kind.slot(name) {
            if !self.kind.decl().storage.is_open() {
                return Err(VmError::Cosm(format!(
                    "`{}` is sealed: its field `{name}` can't be deleted",
                    self.kind.name()
                )));
            }
            if s.slots[i].take().is_none() {
                drop(s);
                return Err(self.missing(name));
            }
            s.order.retain(|&j| j != i);
            return Ok(());
        }
        if s.extra.shift_remove(name).is_none() {
            drop(s);
            return Err(self.missing(name));
        }
        Ok(())
    }

    fn missing(&self, name: &str) -> VmError {
        VmError::Cosm(format!("`{}` has no member `{name}`", self.kind.name()))
    }

    /// Structural equality's comparison: the same set fields with equal
    /// values, in any order.
    fn same_fields(&self, other: &[(String, Value)]) -> bool {
        let mine = self.fields();
        mine.len() == other.len() && mine.iter().all(|(k, v)| other.iter().any(|(k2, v2)| k == k2 && v == v2))
    }
}

fn field_list(kind: &Kind) -> String {
    let names: Vec<&str> = kind.fields().iter().map(|f| f.name.as_str()).collect();
    if names.is_empty() { "none".to_string() } else { names.join(", ") }
}

fn const_value(c: &Const) -> Value {
    match c {
        Const::Null => Value::Null,
        Const::Bool(b) => Value::Bool(*b),
        Const::Int(i) => Value::Int(*i),
        Const::Float(f) => Value::Float(*f),
        Const::Str(s) => Value::Str(s.clone()),
    }
}

/// `==` with a Record on either side.
pub fn record_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Record(x), Value::Record(y)) => {
            if Rc::ptr_eq(x, y) {
                return true;
            }
            if x.kind.decl().equality == Equality::Identity || y.kind.decl().equality == Equality::Identity {
                return false;
            }
            x.same_fields(&y.fields())
        }
        (Value::Record(r), Value::Map(m)) | (Value::Map(m), Value::Record(r)) => {
            r.kind.decl().equality == Equality::Structural
                && r.same_fields(&m.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>())
        }
        _ => false,
    }
}

/// `COPY`: a copy of an instance of a `value(copy)` / `value(move)` Kind
/// (its `value(…)` fields copied too, everything else shared); any other
/// value as it is.
pub fn copy_value(v: &Value) -> Value {
    match v {
        Value::Record(r) if r.kind.is_value() => {
            let mut state = r.state.borrow().clone();
            for slot in state.slots.iter_mut().flatten() {
                *slot = copy_value(slot);
            }
            for v in state.extra.values_mut() {
                *v = copy_value(v);
            }
            Value::Record(Rc::new(RecordValue::with_state(&r.kind, state)))
        }
        other => other.clone(),
    }
}

/// `clone()`'s default: a deep copy that shares nothing.
pub fn deep_clone(v: &Value) -> Value {
    match v {
        Value::Record(r) => {
            let mut state = r.state.borrow().clone();
            for slot in state.slots.iter_mut().flatten() {
                *slot = deep_clone(slot);
            }
            for v in state.extra.values_mut() {
                *v = deep_clone(v);
            }
            Value::Record(Rc::new(RecordValue::with_state(&r.kind, state)))
        }
        Value::Array(a) => Value::new_array(a.borrow().iter().map(deep_clone).collect()),
        Value::Vector(a) => Value::new_vector(a.borrow().iter().map(deep_clone).collect()),
        Value::Set(a) => Value::new_set(a.borrow().iter().map(deep_clone).collect()),
        Value::List(l) => Value::new_list(l.borrow().iter().map(deep_clone).collect()),
        Value::Tuple(t) => Value::Tuple(t.iter().map(deep_clone).collect()),
        Value::Map(m) => Value::new_map(m.borrow().iter().map(|(k, v)| (k.clone(), deep_clone(v)))),
        other => other.clone(),
    }
}

/// `IS_KIND`: an instance of `kind` or of a Kind with `kind` in its
/// resolution order. Maps (object literals) are never instances.
pub fn is_kind(v: &Value, kind: &str) -> bool {
    matches!(v, Value::Record(r) if r.kind.is_a(kind))
}

/// What a member name resolves to.
enum Found<'k> {
    /// A declared field that isn't set (a set one is read off the instance).
    UnsetField,
    Method { func: &'k FnRef, receiver: Receiver },
    Accessor { get: Option<&'k FnRef>, set: Option<&'k FnRef>, receiver: Receiver },
    StaticConst(&'k Const),
    StaticFn(&'k FnRef),
}

/// Look `name` up on `kind` along its resolution order (not the instance's
/// fields).
fn find_on_kind<'k>(kind: &'k Kind, name: &str) -> Result<Option<Found<'k>>, VmError> {
    for decl in &kind.mro {
        if let Some(m) = decl.member(name) {
            return Ok(Some(match &m.member {
                Member::Method { func, receiver } => Found::Method { func, receiver: receiver.unwrap_or(decl.receiver) },
                Member::Field => Found::UnsetField,
                Member::Static { value: StaticValue::Const(c) } => Found::StaticConst(c),
                Member::Static { value: StaticValue::Fn(f) } => Found::StaticFn(f),
                // CRUSH-277 S4: accessors run; they resolve like methods
                // (the declaring Kind's receiver strategy binds them).
                Member::Accessor { get, set } => Found::Accessor {
                    get: get.as_ref(),
                    set: set.as_ref(),
                    receiver: decl.receiver,
                },
            }));
        }
        if decl.field_slot(name).is_some() {
            return Ok(Some(Found::UnsetField));
        }
        // The impl tables of its conformances (`impl Shape for Point`).
        let mut hits: Vec<(&str, Found<'k>)> = Vec::new();
        for conf in &decl.conforms {
            if let Some(m) = conf.members.iter().find(|m| m.name == name) {
                let found = match &m.member {
                    Member::Method { func, receiver } => {
                        Found::Method { func, receiver: receiver.unwrap_or(decl.receiver) }
                    }
                    Member::Static { value: StaticValue::Fn(f) } => Found::StaticFn(f),
                    Member::Static { value: StaticValue::Const(c) } => Found::StaticConst(c),
                    _ => continue,
                };
                hits.push((conf.interface.as_str(), found));
            }
        }
        if hits.is_empty() {
            // The interfaces' default methods.
            for conf in &decl.conforms {
                let Some(iface) = kind.interfaces.get(conf.interface.as_str()) else { continue };
                if let Some(m) = iface.member(name)
                    && let Some(f) = &m.default
                {
                    let found = match m.receiver {
                        Some(_) => Found::Method { func: f, receiver: decl.receiver },
                        None => Found::StaticFn(f),
                    };
                    hits.push((conf.interface.as_str(), found));
                }
            }
        }
        match hits.len() {
            0 => {}
            1 => return Ok(hits.pop().map(|(_, f)| f)),
            _ => {
                let names: Vec<&str> = hits.iter().map(|(i, _)| *i).collect();
                return Err(VmError::Cosm(format!(
                    "`{name}` on `{}` is ambiguous: it comes from {}; call it by its qualified name \
                     (`{}::{name}(…)`)",
                    decl.name,
                    names.join(" and "),
                    names[0]
                )));
            }
        }
    }
    Ok(None)
}

fn func_value(name: &FnRef, arity: u8, captured: Vec<Value>) -> Value {
    Value::Func(Rc::new(FuncValue { name: name.as_str().to_string(), arity, captured }))
}

/// A method used as a value: bound per its receiver.
fn method_value(kind: &Rc<Kind>, recv: &Value, func: &FnRef, receiver: Receiver) -> Result<Value, VmError> {
    let arity = kind.arity_of(func.as_str())?;
    let bound = |v: Value| -> Result<Value, VmError> {
        let arity = arity.checked_sub(1).ok_or_else(|| {
            VmError::Cosm(format!("the method `{func}` of `{}` takes no receiver parameter", kind.name()))
        })?;
        Ok(func_value(func, arity, vec![v]))
    };
    match receiver {
        Receiver::BoundSelf => bound(recv.clone()),
        Receiver::BoundCls => bound(Value::Kind(kind.clone())),
        // JavaScript: a method read off an object is the plain function;
        // called on its own, `this` is undefined.
        Receiver::DynamicThis => bound(Value::Null),
        // Rust / Go: the path `Point::norm2` — the caller passes the receiver.
        Receiver::ExplicitValue | Receiver::ExplicitRef | Receiver::ExplicitRefMut => {
            Ok(func_value(func, arity, Vec::new()))
        }
    }
}

/// A getter/setter invocation the VM must perform (CRUSH-277 S4):
/// accessors are ordinary synchronous functions; the VMs run them to
/// completion where the member op stands, as they run print hooks.
/// `args` are in call order (receiver first).
pub struct AccessorCall {
    pub func: String,
    pub args: Vec<Value>,
}

/// What reading a member resolves to.
pub enum MemberRead {
    /// The value, with no call.
    Done(Value),
    /// Call this accessor; its return value is the read's result.
    Call(AccessorCall),
}

/// What writing a member resolves to.
pub enum MemberWrite {
    /// Written (a field set on the instance), with no call.
    Done,
    /// Call this setter with (receiver, value); its result is discarded.
    Call(AccessorCall),
}

/// The receiver an accessor binds, per the declaring Kind's receiver
/// strategy — the same binding a method gets (a detached read of a
/// `dynamic_this` accessor gets a null `this`, as in JS).
fn accessor_receiver(
    kind: &Rc<Kind>,
    recv: Option<&Value>,
    receiver: Receiver,
    name: &str,
) -> Result<Value, VmError> {
    Ok(match receiver {
        Receiver::BoundCls => Value::Kind(kind.clone()),
        Receiver::DynamicThis => recv.cloned().unwrap_or(Value::Null),
        _ => match recv {
            Some(v) => v.clone(),
            None => {
                return Err(VmError::Cosm(format!(
                    "the accessor `{name}` of `{}` needs an instance",
                    kind.name()
                )));
            }
        },
    })
}

fn check_accessor_arity(kind: &Kind, func: &FnRef, params: usize, name: &str) -> Result<(), VmError> {
    let arity = kind.arity_of(func.as_str())? as usize;
    if arity != params {
        return Err(VmError::FuncArity {
            function: format!("{}.{name}", kind.name()),
            expected: params,
            got: arity,
        });
    }
    Ok(())
}

/// The call that reads accessor `name` of `kind` through its getter.
fn getter_call(
    kind: &Rc<Kind>,
    get: &FnRef,
    receiver: Receiver,
    recv: Option<&Value>,
    name: &str,
) -> Result<AccessorCall, VmError> {
    check_accessor_arity(kind, get, 1, name)?;
    Ok(AccessorCall {
        func: get.as_str().to_string(),
        args: vec![accessor_receiver(kind, recv, receiver, name)?],
    })
}

/// The call that writes accessor `name` of `kind` through its setter.
fn setter_call(
    kind: &Rc<Kind>,
    set: &FnRef,
    receiver: Receiver,
    recv: &Value,
    name: &str,
    v: Value,
) -> Result<AccessorCall, VmError> {
    check_accessor_arity(kind, set, 2, name)?;
    Ok(AccessorCall {
        func: set.as_str().to_string(),
        args: vec![accessor_receiver(kind, Some(recv), receiver, name)?, v],
    })
}

/// `GET_MEMBER` (`strict`) and `GET_FIELD` on a Record or a Kind.
///
/// `GET_FIELD` is the map spelling the compiler emits for `p.x`: on a name the
/// Record doesn't have it reads `null`, as on the map a struct used to be.
/// `GET_MEMBER` reports the missing member.
pub fn get_member(target: &Value, name: &str, strict: bool) -> Result<MemberRead, VmError> {
    let (kind, recv) = match target {
        Value::Record(r) => {
            // An instance field shadows an inherited accessor, as in JS.
            if let Some(v) = r.field(name) {
                return Ok(MemberRead::Done(v));
            }
            (r.kind.clone(), Some(target))
        }
        Value::Kind(k) => (k.clone(), None),
        other => return Err(VmError::TypeError { expected: "record", got: other.type_name() }),
    };
    // A class-level value: the chain walk stops at the first Kind
    // holding the name, whether by run-time write or by declaration
    // (CRUSH-285 review #3, re-review: a nearer declaration beats an
    // ancestor's write).
    if recv.is_none()
        && let Some(v) = kind.static_in_chain(name)?
    {
        return Ok(MemberRead::Done(v));
    }
    match find_on_kind(&kind, name)? {
        // Statics live on the class, not on its instances (JS:
        // `new C().n` is undefined — the strict read refuses, the lax
        // one reads null, like any missing member).
        Some(Found::StaticConst(_)) | Some(Found::StaticFn(_))
            if recv.is_some()
                && kind.decl().resolution == crush_cosm::Resolution::PrototypeChain =>
        {
            if strict {
                return Err(VmError::Cosm(format!(
                    "`{}` has no member `{name}`",
                    kind.name()
                )));
            }
            return Ok(MemberRead::Done(Value::Null));
        }
        Some(Found::UnsetField) => Ok(MemberRead::Done(Value::Null)),
        Some(Found::StaticConst(c)) => Ok(MemberRead::Done(const_value(c))),
        Some(Found::StaticFn(f)) => Ok(MemberRead::Done(func_value(f, kind.arity_of(f.as_str())?, Vec::new()))),
        Some(Found::Method { func, receiver }) => Ok(MemberRead::Done(match recv {
            Some(recv) => method_value(&kind, recv, func, receiver)?,
            // Read off the Kind: a classmethod binds the class; any other
            // method is the plain function.
            None if receiver == Receiver::BoundCls => method_value(&kind, target, func, receiver)?,
            None => func_value(func, kind.arity_of(func.as_str())?, Vec::new()),
        })),
        Some(Found::Accessor { get, receiver, .. }) => match get {
            // A write-only property reads as undefined.
            None => Ok(MemberRead::Done(Value::Null)),
            Some(g) => Ok(MemberRead::Call(getter_call(&kind, g, receiver, recv.as_deref(), name)?)),
        },
        None if strict => Err(VmError::Cosm(format!("`{}` has no member `{name}`", kind.name()))),
        None => Ok(MemberRead::Done(Value::Null)),
    }
}

/// `SET_MEMBER` / `SET_FIELD` on a Record (or a class-level write on a
/// Kind: JS `C.x = v`).
pub fn set_member(target: &Value, name: &str, v: Value) -> Result<MemberWrite, VmError> {
    if let Value::Kind(k) = target {
        // A static method can't be assigned over; anything else becomes
        // a class-level value (a declared constant is its initial value).
        if matches!(find_on_kind(k, name)?, Some(Found::StaticFn(_))) {
            return Err(VmError::Cosm(format!(
                "`{}.{name}` is a static method; it can't be assigned",
                k.name()
            )));
        }
        k.set_static(name, v);
        return Ok(MemberWrite::Done);
    }
    let Value::Record(r) = target else {
        return Err(VmError::TypeError { expected: "record", got: target.type_name() });
    };
    // An existing instance field wins over an inherited setter (JS
    // [[Set]]: an own data property is written directly).
    if r.field(name).is_none()
        && let Some(Found::Accessor { set, receiver, .. }) = find_on_kind(&r.kind, name)?
    {
        return match set {
            Some(s) => Ok(MemberWrite::Call(setter_call(&r.kind, s, receiver, target, name, v)?)),
            // A read-only property (JS strict mode throws the same way).
            None => Err(VmError::Cosm(format!(
                "`{}.{name}` has no setter",
                r.kind.name()
            ))),
        };
    }
    r.set_field(name, v).map(|()| MemberWrite::Done)
}

/// `HAS_MEMBER`: a set field, or any member along the resolution order.
pub fn has_member(target: &Value, name: &str) -> Result<bool, VmError> {
    Ok(match target {
        Value::Record(r) => {
            r.has_field(name)
                || matches!(find_on_kind(&r.kind, name)?, Some(f) if !matches!(f, Found::UnsetField)
                    // Statics aren't members of instances (JS `in`).
                    && !(matches!(f, Found::StaticConst(_) | Found::StaticFn(_))
                        && r.kind.decl().resolution == crush_cosm::Resolution::PrototypeChain))
        }
        Value::Kind(k) => find_on_kind(k, name)?.is_some(),
        Value::Map(m) => m.borrow().contains_key(name),
        _ => false,
    })
}

/// `DEL_MEMBER`: delete an instance attribute (open storage only).
pub fn del_member(target: &Value, name: &str) -> Result<(), VmError> {
    match target {
        Value::Record(r) => r.del_field(name),
        Value::Map(m) => {
            m.borrow_mut().shift_remove(name);
            Ok(())
        }
        other => Err(VmError::TypeError { expected: "record", got: other.type_name() }),
    }
}

/// A call a member op asks the VM to perform: `func` with `leading` as its
/// first parameters (pushed on top of the caller's arguments, `leading[0]`
/// topmost — the convention `CALL_VALUE` uses for captured values).
#[derive(Debug)]
pub struct Callee {
    pub func: String,
    pub leading: Vec<Value>,
}

/// What `CALL_MEMBER` resolves to.
pub enum MemberCall {
    /// Call this function.
    Call(Callee),
    /// The result, with no call (the built-in `clone()`).
    Done(Value),
}

/// `CALL_MEMBER name argc` on `recv`: resolve the method through the Kind and
/// apply its receiver strategy. Never falls back to a global capability.
pub fn call_member(recv: &Value, name: &str, argc: usize) -> Result<MemberCall, VmError> {
    let (kind, instance) = match recv {
        Value::Record(r) => (r.kind.clone(), true),
        Value::Kind(k) => (k.clone(), false),
        other => {
            return Err(VmError::Cosm(format!(
                "`.{name}(…)` on a {}: methods resolve on struct instances (COSM Records)",
                other.type_name()
            )));
        }
    };
    if instance && let Some(v) = recv_field(recv, name) {
        // A field holding a function value: call it.
        let f = crate::vm::callable(&v, argc)?;
        return Ok(MemberCall::Call(Callee { func: f.name.clone(), leading: f.captured.clone() }));
    }
    let found = find_on_kind(&kind, name)?;
    let check = |func: &FnRef, leading: usize| -> Result<(), VmError> {
        let arity = kind.arity_of(func.as_str())? as usize;
        if arity != argc + leading {
            return Err(VmError::FuncArity {
                function: format!("{}.{name}", kind.name()),
                expected: arity.saturating_sub(leading),
                got: argc,
            });
        }
        Ok(())
    };
    match found {
        Some(Found::Method { func, receiver }) => {
            let first = match receiver {
                Receiver::BoundCls => Value::Kind(kind.clone()),
                // A method called on the Kind itself, not an instance: the
                // explicit receivers take it as an ordinary first argument.
                _ if !instance => {
                    check(func, 0)?;
                    return Ok(MemberCall::Call(Callee { func: func.as_str().to_string(), leading: Vec::new() }));
                }
                Receiver::ExplicitValue => copy_value(recv),
                Receiver::ExplicitRef | Receiver::ExplicitRefMut | Receiver::BoundSelf | Receiver::DynamicThis => {
                    recv.clone()
                }
            };
            check(func, 1)?;
            Ok(MemberCall::Call(Callee { func: func.as_str().to_string(), leading: vec![first] }))
        }
        Some(Found::StaticFn(f)) => {
            if instance
                && kind.decl().resolution == crush_cosm::Resolution::PrototypeChain
            {
                // JS: `c.s()` where `s` is static is a TypeError — the
                // static lives on the class (CRUSH-285 review #3).
                return Err(VmError::Cosm(format!(
                    "`{}.{name}` is a static method: call it on `{}`, not on an instance",
                    kind.name(),
                    kind.name()
                )));
            }
            check(f, 0)?;
            Ok(MemberCall::Call(Callee { func: f.as_str().to_string(), leading: Vec::new() }))
        }
        Some(Found::Accessor { .. }) => Err(VmError::Cosm(format!(
            "`{}.{name}` is an accessor, not a method: read it, don't call it",
            kind.name()
        ))),
        Some(Found::StaticConst(_)) | Some(Found::UnsetField) => {
            Err(VmError::Cosm(format!("`{}.{name}` is not a method", kind.name())))
        }
        None if name == "clone" && argc == 0 && instance => clone_call(&kind, recv),
        None => Err(VmError::Cosm(format!("`{}` has no method `{name}`", kind.name()))),
    }
}

fn recv_field(recv: &Value, name: &str) -> Option<Value> {
    match recv {
        Value::Record(r) => r.field(name),
        _ => None,
    }
}

/// `x.clone()` with no `clone` member: the Kind's `clone` hook (a function is
/// called like a method), else a deep copy.
fn clone_call(kind: &Rc<Kind>, recv: &Value) -> Result<MemberCall, VmError> {
    let hook = kind.mro.iter().find_map(|k| k.hooks.clone.as_ref());
    match hook {
        Some(HookImpl::Fn(f)) => {
            Ok(MemberCall::Call(Callee { func: f.as_str().to_string(), leading: vec![recv.clone()] }))
        }
        Some(HookImpl::Disabled) => Err(VmError::Cosm(format!("`{}` can't be cloned", kind.name()))),
        Some(HookImpl::Derived) | None => Ok(MemberCall::Done(deep_clone(recv))),
    }
}

/// What `CONSTRUCT` does.
pub enum Construction {
    /// The instance; the VM pops the `argc` arguments and pushes it.
    Done(Value),
    /// Call a constructor. With an `instance` (an initialiser, which gets it
    /// as its first parameter) the VM puts `instance` below the arguments and
    /// calls `callee`; the initialiser's own return value is dropped by the
    /// `POP` that follows `CONSTRUCT` for such a Kind
    /// ([`initialises_by_call`]). Without one (a factory) the call's return
    /// value is the instance.
    Call { instance: Option<Value>, callee: Callee },
}

/// `NEW_RECORD`: a fresh instance (field defaults applied), no constructor —
/// what a struct literal builds before it writes its fields.
pub fn new_record(kind: &Rc<Kind>) -> Value {
    Value::Record(Rc::new(RecordValue::new(kind)))
}

/// Whether `CONSTRUCT` of the Kind `kind` of `module` calls an initialiser
/// (`casm::kind_constructor`), so that a `POP` of its return value must
/// follow. `false` for a Kind whose resolution order can't be computed (the
/// VMs refuse that table at load).
pub fn initialises_by_call(module: &CosmModule, kind: &str) -> bool {
    matches!(casm::kind_constructor_in(module, kind), Some((_, casm::ConstructorUse::Initialiser)))
}

/// `CONSTRUCT kind argc` by the Kind's `construct` strategy. `args` are the
/// arguments, first argument first. A `literal` Kind takes no arguments (the
/// fields are written after) or one per declared field, in slot order.
pub fn construct(kind: &Rc<Kind>, args: &[Value]) -> Result<Construction, VmError> {
    if let Construct::PyNewInit { new_fn: Some(f), .. } = &kind.decl().construct {
        // Refused when the program loads (the shared COSM rule).
        return Err(VmError::Unsupported(format!("`{}`'s `__new__` (`{f}`)", kind.name())));
    }
    let mro: Vec<&KindDecl> = kind.mro.iter().map(|k| &**k).collect();
    let record = RecordValue::new(kind);
    match casm::kind_constructor(&mro) {
        Some((func, usage)) => {
            let initialiser = usage == casm::ConstructorUse::Initialiser;
            let leading = usize::from(initialiser);
            let arity = kind.arity_of(func.as_str())? as usize;
            if arity != args.len() + leading {
                return Err(VmError::FuncArity {
                    function: format!("{}'s constructor `{func}`", kind.name()),
                    expected: arity.saturating_sub(leading),
                    got: args.len(),
                });
            }
            let callee = Callee { func: func.as_str().to_string(), leading: Vec::new() };
            if !initialiser {
                return Ok(Construction::Call { instance: None, callee });
            }
            let instance = Value::Record(Rc::new(record));
            Ok(Construction::Call {
                instance: Some(instance.clone()),
                callee: Callee { leading: vec![instance], ..callee },
            })
        }
        None => {
            let n = kind.fields().len();
            let literal = matches!(kind.decl().construct, Construct::Literal);
            let fits = args.is_empty() || (literal && args.len() == n);
            if !fits {
                return Err(VmError::Cosm(if literal {
                    format!(
                        "`{}` is built from a literal: give no arguments, or one per field ({n}: {})",
                        kind.name(),
                        field_list(kind)
                    )
                } else {
                    format!("`{}` has no `__init__`, so it takes no arguments", kind.name())
                }));
            }
            for (i, v) in args.iter().enumerate() {
                record.set_slot(i, v.clone())?;
            }
            Ok(Construction::Done(Value::Record(Rc::new(record))))
        }
    }
}

/// Capabilities that turn their arguments into text: a Record with a
/// `str`/`repr` hook is printed through it before they see it.
pub fn is_text_cap(cap: &str) -> bool {
    matches!(
        cap,
        "io.print" | "io.write" | "py.print" | "py.print_sep_end" | "str.concat" | "str.format" | "conv.to_str"
            | "js.format"
    )
}

/// Replace every Record (also inside arrays, tuples and maps) whose Kind has
/// a `str`/`repr` hook by the text the hook returns; `call` runs a hook
/// function on its receiver. `None` when nothing has a hook.
pub fn render_hooked(
    v: &Value,
    call: &mut dyn FnMut(&str, Value) -> Result<Value, VmError>,
) -> Result<Option<Value>, VmError> {
    render_at(v, call, 0)
}

/// How deep [`render_hooked`] looks for hooked Records (a cyclic array would
/// otherwise recurse forever; the printer reports cycles itself).
const RENDER_DEPTH: usize = 64;

fn render_at(
    v: &Value,
    call: &mut dyn FnMut(&str, Value) -> Result<Value, VmError>,
    depth: usize,
) -> Result<Option<Value>, VmError> {
    if depth > RENDER_DEPTH {
        return Ok(None);
    }
    Ok(match v {
        Value::Record(r) => match r.kind.print_hook() {
            Some(f) => Some(Value::Str(call(f.as_str(), v.clone())?.as_text())),
            None => None,
        },
        Value::Array(a) => render_seq(&a.borrow(), call, depth)?.map(Value::new_array),
        Value::Tuple(t) => render_seq(t, call, depth)?.map(Value::Tuple),
        Value::Map(m) => {
            let items: Vec<(String, Value)> = m.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            let values: Vec<Value> = items.iter().map(|(_, v)| v.clone()).collect();
            render_seq(&values, call, depth)?
                .map(|vs| Value::new_map(items.into_iter().map(|(k, _)| k).zip(vs)))
        }
        _ => None,
    })
}

fn render_seq(
    xs: &[Value],
    call: &mut dyn FnMut(&str, Value) -> Result<Value, VmError>,
    depth: usize,
) -> Result<Option<Vec<Value>>, VmError> {
    let mut out: Option<Vec<Value>> = None;
    for (i, x) in xs.iter().enumerate() {
        if let Some(new) = render_at(x, call, depth + 1)? {
            out.get_or_insert_with(|| xs.to_vec())[i] = new;
        }
    }
    Ok(out)
}

/// Whether a call is an indexing capability on a Record or a Kind
/// (`p["x"]`, `p["x"] = v`, `C.x = v`, which the compiler emits as
/// `arr_get` / `arr_set` calls): [`index_cap`] runs it.
pub fn is_index_cap(cap: &str, args: &[Value]) -> bool {
    matches!(
        (cap, args),
        ("arr_get", [Value::Record(_) | Value::Kind(_), _])
            | ("arr_set", [Value::Record(_) | Value::Kind(_), _, _])
            | ("define_field", [Value::Record(_), _, _])
    )
}

/// What an index operation on a Record resolves to (CRUSH-277 S4: a
/// computed member access honours accessors, like the named member ops).
pub enum IndexAccess {
    /// The operation's result.
    Done(Value),
    /// Run this accessor call; the result is then `after` when set (a
    /// write leaves the record), else the call's return value (a read).
    Call { call: AccessorCall, after: Option<Value> },
}

/// An [`is_index_cap`] call: the Record's fields, like `ARR_GET` / `ARR_SET`
/// (on a Kind: its members and class-level values).
pub fn index_cap(cap: &str, args: &[Value]) -> Result<IndexAccess, VmError> {
    match (cap, args) {
        ("arr_get", [Value::Record(r), key]) => index_get(r, key),
        ("arr_get", [k @ Value::Kind(_), key]) => {
            let key = crate::map_ops::map_key_string(key)?;
            Ok(match get_member(k, &key, false)? {
                MemberRead::Done(v) => IndexAccess::Done(v),
                MemberRead::Call(call) => IndexAccess::Call { call, after: None },
            })
        }
        ("arr_set", [rec @ (Value::Record(_) | Value::Kind(_)), key, v]) => {
            let key = crate::map_ops::map_key_string(key)?;
            Ok(match set_member(rec, &key, v.clone())? {
                MemberWrite::Done => IndexAccess::Done(rec.clone()),
                MemberWrite::Call(call) => IndexAccess::Call { call, after: Some(rec.clone()) },
            })
        }
        // Define an own field, bypassing any setter (a JS class field
        // initialiser is [[Define]], not [[Set]]) — CRUSH-285 review #6.
        ("define_field", [Value::Record(r), key, v]) => {
            let key = crate::map_ops::map_key_string(key)?;
            r.set_field(&key, v.clone())?;
            Ok(IndexAccess::Done(Value::Record(r.clone())))
        }
        _ => Err(VmError::UnknownCap(cap.to_string())),
    }
}

/// Capability arguments: a Record reaches a capability as a map of its set
/// fields (capabilities know maps, not Kinds), and a Kind reaches a text
/// capability as its printed form.
pub fn cap_args(cap: &str, args: Vec<Value>) -> Result<Vec<Value>, VmError> {
    // Strict equality needs the original references for Value-level identity;
    // converting Records to fresh maps or rejecting Kinds would lose that.
    if cap == "js.strict_eq" {
        return Ok(args);
    }
    if !args.iter().any(|a| matches!(a, Value::Record(_) | Value::Kind(_))) {
        return Ok(args);
    }
    args.into_iter()
        .map(|a| match a {
            Value::Record(r) => Ok(r.to_map()),
            Value::Kind(k) if is_text_cap(cap) => Ok(Value::Str(format!("<kind {}>", k.name()))),
            Value::Kind(k) => Err(VmError::Unsupported(format!(
                "passing the Kind `{}` to the capability `{cap}`",
                k.name()
            ))),
            other => Ok(other),
        })
        .collect()
}

/// `TYPEOF`: a Record's Kind name, `kind` for a Kind.
pub fn type_name(v: &Value) -> Option<String> {
    match v {
        Value::Record(r) => Some(r.kind.name().to_string()),
        Value::Kind(_) => Some("kind".to_string()),
        _ => None,
    }
}

/// `ARR_GET` with a string index on a Record: an own field, else an
/// accessor's getter (CRUSH-277 S4), else the absent-key error it has
/// always been.
pub fn index_get(r: &Rc<RecordValue>, key: &Value) -> Result<IndexAccess, VmError> {
    let key = crate::map_ops::map_key_string(key)?;
    if let Some(v) = r.field(&key) {
        return Ok(IndexAccess::Done(v));
    }
    match find_on_kind(&r.kind, &key)? {
        Some(Found::Accessor { get: Some(g), receiver, .. }) => Ok(IndexAccess::Call {
            call: getter_call(&r.kind, g, receiver, Some(&Value::Record(r.clone())), &key)?,
            after: None,
        }),
        _ => Err(VmError::TypeError { expected: "present map key", got: "absent key" }),
    }
}

/// The field names a Record has set, in print order (`MAP_KEYS`).
pub fn keys(r: &RecordValue) -> Value {
    Value::new_array(r.fields().into_iter().map(|(k, _)| Value::Str(k)).collect())
}
