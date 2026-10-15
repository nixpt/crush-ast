//! The COSM schema: [`CosmModule`], [`KindDecl`], [`InterfaceDecl`] and the
//! closed strategy catalogue.
//!
//! Field names follow `docs/design/object-model.md` ("COSM: objects are
//! lowered, like code") exactly. The JSON shape is documented in the crate
//! README.

use crate::span::Span;
use crate::types::CosmType;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::fmt;

macro_rules! name_ref {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(name: impl Into<String>) -> Self {
                $name(name.into())
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                $name(s.to_string())
            }
        }
        impl From<String> for $name {
            fn from(s: String) -> Self {
                $name(s)
            }
        }
        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }
        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }
        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

name_ref! {
    /// A reference to a [`KindDecl`] by name, within the module or one of its
    /// dependencies. Resolved to a runtime Kind at VM load, not here.
    KindRef
}
name_ref! {
    /// A reference to an [`InterfaceDecl`] by name.
    InterfaceRef
}
name_ref! {
    /// A reference to a function by name: a key of the CAST `Program`'s
    /// `functions` map that this module is lowered alongside.
    FnRef
}

// ─── module ─────────────────────────────────────────────────────────────────

/// The COSM half of a lowered program: every object declaration, beside the
/// CAST functions they reference by [`FnRef`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CosmModule {
    #[serde(default)]
    pub kinds: Vec<KindDecl>,
    #[serde(default)]
    pub interfaces: Vec<InterfaceDecl>,
}

// ─── kinds ──────────────────────────────────────────────────────────────────

/// A Kind declaration: what a type is and how its instances behave.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KindDecl {
    pub name: String,
    /// The language that declared it (`"crush"`, `"rust"`, `"python"`, …).
    /// An instance keeps its home semantics wherever it travels.
    pub home_lang: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    pub storage: Storage,
    pub semantics: Semantics,
    /// Direct parents, in declaration order (Python's `__bases__`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<KindRef>,
    pub resolution: Resolution,
    /// The default receiver for this Kind's methods; a method member may
    /// override it ([`Member::Method::receiver`]).
    pub receiver: Receiver,
    pub construct: Construct,
    /// Inherent members, in declaration order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<MemberDecl>,
    #[serde(default, skip_serializing_if = "Hooks::is_empty")]
    pub hooks: Hooks,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conforms: Vec<Conformance>,
    /// The `==` default when there is no `eq` hook.
    pub equality: Equality,
}

/// How instance state is stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Storage {
    /// A closed, fixed layout: slot `i` is `fields[i]`.
    Sealed { fields: Vec<FieldDecl> },
    /// Fields can be added and deleted at any time (JS objects, Python `__dict__`).
    OpenDict,
    /// Python `__slots__` plus an open overflow dict.
    OpenDictWithSlots { slots: Vec<FieldDecl> },
}

/// A declared field (or slot).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDecl {
    pub name: String,
    #[serde(rename = "type", default)]
    pub ty: CosmType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Const>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

/// Value or reference semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Semantics {
    /// Assignment shares the instance (JS, Python, Dart, Crush today).
    Reference,
    /// Assignment, argument passing and `return` copy or move the instance.
    Value(ValueMode),
}

/// The flavour of [`Semantics::Value`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueMode {
    /// Implicit bitwise copy (C, Go, Rust `Copy`).
    Copy,
    /// Assignment moves; a later use of the source is a compile error (Rust).
    Move,
    /// Copies happen only through an explicit clone.
    CloneExplicit,
}

/// How member lookup walks the hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    /// No inheritance (Rust, C, Go methods, Crush).
    None,
    /// Single inheritance (Java, Kotlin).
    Single,
    /// C3 linearization, multiple inheritance (Python, Dart).
    C3,
    /// A prototype chain, mutable at run time (JS).
    PrototypeChain,
    /// Go embedding: fields and methods are promoted by depth.
    EmbeddingPromotion,
}

/// How a method receives its receiver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Receiver {
    /// JS `this`, set by the call site.
    DynamicThis,
    /// Python `self`: `obj.m` captures the instance.
    BoundSelf,
    /// Python `@classmethod`: `obj.m` captures the class.
    BoundCls,
    /// Rust `self` / Go value receivers: by value.
    ExplicitValue,
    /// Rust `&self`; Crush's `self` parameter.
    ExplicitRef,
    /// Rust `&mut self` / Go pointer receivers.
    ExplicitRefMut,
}

/// How an instance is created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Construct {
    /// A struct/composite literal (`P { x: 1 }`).
    Literal,
    /// A constructor function (Dart, Java).
    Ctor {
        #[serde(rename = "fn")]
        func: FnRef,
    },
    /// JS `new F(..)`: allocate with `F.prototype`, then call the constructor.
    JsNew { ctor_fn: FnRef },
    /// Python `C(..)`: `__new__` then `__init__`. A missing function is
    /// looked up through the MRO at construction time (ending at `object`'s),
    /// which is what a class that inherits `__init__` needs.
    PyNewInit {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        new_fn: Option<FnRef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        init_fn: Option<FnRef>,
    },
}

/// A member of a Kind (or of a conformance's impl table).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    #[serde(flatten)]
    pub member: Member,
}

/// What a member is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Member {
    /// A method. `receiver` overrides the Kind's default receiver (Rust
    /// `&self` vs `&mut self`, Python `@classmethod` → `bound_cls`).
    Method {
        #[serde(rename = "fn")]
        func: FnRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        receiver: Option<Receiver>,
    },
    /// An instance field (on a sealed Kind it must be a declared field).
    Field,
    /// A computed property (`@property`, JS `get`/`set`).
    Accessor {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        get: Option<FnRef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        set: Option<FnRef>,
    },
    /// A class-level member: a class attribute or a static function.
    Static { value: StaticValue },
}

/// The payload of a [`Member::Static`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaticValue {
    Const(Const),
    Fn(FnRef),
}

/// A constant: a field default or a static value. Serialized as the plain
/// JSON scalar (`null`, `true`, `3`, `2.5`, `"s"`); an integral JSON number
/// is an `Int`, one written with a fraction or exponent is a `Float`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Const {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

/// Protocol hooks. Absent means "the strategy's default".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hooks {
    /// Lookup miss (`__getattr__`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing: Option<HookImpl>,
    /// Attribute write (`__setattr__`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set: Option<HookImpl>,
    /// Calling an instance (`__call__`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<HookImpl>,
    /// Iteration (`__iter__`, `[Symbol.iterator]`, `IntoIterator`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iter: Option<HookImpl>,
    /// Binary operators, keyed by operator.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub op: BTreeMap<Op, HookImpl>,
    /// Reflected binary operators (Python `__radd__`), keyed by operator.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub op_reflected: BTreeMap<Op, HookImpl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eq: Option<HookImpl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<HookImpl>,
    /// Ordering (Rust `PartialOrd::partial_cmp`, Java `compareTo`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmp: Option<HookImpl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub str: Option<HookImpl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repr: Option<HookImpl>,
    /// Explicit deep copy (Rust `Clone`, `.clone()`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone: Option<HookImpl>,
}

impl Hooks {
    pub fn is_empty(&self) -> bool {
        *self == Hooks::default()
    }
}

/// How a hook is implemented.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookImpl {
    /// A user function.
    Fn(FnRef),
    /// The strategy's derived implementation (Rust `#[derive(PartialEq)]` →
    /// structural `eq`; `#[derive(Clone)]` → deep `clone`).
    Derived,
    /// Explicitly unsupported (Python `__hash__ = None`, or the implicit
    /// unhashability of a class that defines `__eq__` without `__hash__`).
    Disabled,
}

/// Operators that may have a hook. `==` is the [`Hooks::eq`] hook and
/// ordering is [`Hooks::cmp`]; the individual comparison operators are here
/// for languages that overload them one by one (Python `__lt__`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Op {
    #[serde(rename = "+")]
    Add,
    #[serde(rename = "-")]
    Sub,
    #[serde(rename = "*")]
    Mul,
    #[serde(rename = "/")]
    Div,
    #[serde(rename = "//")]
    FloorDiv,
    #[serde(rename = "%")]
    Rem,
    #[serde(rename = "**")]
    Pow,
    #[serde(rename = "@")]
    MatMul,
    #[serde(rename = "&")]
    BitAnd,
    #[serde(rename = "|")]
    BitOr,
    #[serde(rename = "^")]
    BitXor,
    #[serde(rename = "<<")]
    Shl,
    #[serde(rename = ">>")]
    Shr,
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Le,
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Ge,
    /// Unary minus.
    #[serde(rename = "neg")]
    Neg,
    /// Unary plus.
    #[serde(rename = "pos")]
    Pos,
    /// Bitwise invert (Python `~`).
    #[serde(rename = "~")]
    Invert,
    /// Rust `!` (`std::ops::Not`).
    #[serde(rename = "!")]
    Not,
    /// Indexed read (`__getitem__`, `Index`).
    #[serde(rename = "[]")]
    Index,
    /// Indexed write (`__setitem__`, `IndexMut`).
    #[serde(rename = "[]=")]
    IndexSet,
    /// Membership (`__contains__`).
    #[serde(rename = "in")]
    Contains,
}

/// A declared conformance: "this Kind implements that interface", with the
/// members that implement it (a Rust `impl Trait for T` block).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conformance {
    pub interface: InterfaceRef,
    pub check: Satisfaction,
    /// The impl table. Members not listed here are found on the Kind (and
    /// its ancestors) or from the interface's defaults.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<MemberDecl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

/// The default meaning of `==` without an `eq` hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Equality {
    Identity,
    Structural,
}

// ─── interfaces ─────────────────────────────────────────────────────────────

/// An interface (Rust trait, Go interface, Java interface, Python ABC).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterfaceDecl {
    pub name: String,
    pub home_lang: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<InterfaceMember>,
    pub satisfaction: Satisfaction,
}

/// How an interface is satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Satisfaction {
    /// Only by an explicit declaration (Rust `impl Trait for T`, Java `implements`).
    Nominal,
    /// By having the members (Go).
    Structural,
}

/// One method signature of an interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterfaceMember {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// `None` for an associated function with no receiver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver: Option<Receiver>,
    /// Parameters after the receiver.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    #[serde(default)]
    pub returns: CosmType,
    /// A default body (Rust/Crush trait default method, Java `default`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<FnRef>,
}

/// A named, typed parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Param {
    pub name: String,
    #[serde(rename = "type", default)]
    pub ty: CosmType,
}

// ─── convenience API ────────────────────────────────────────────────────────

impl CosmModule {
    pub fn new() -> Self {
        Self::default()
    }

    /// The first kind named `name`.
    pub fn kind(&self, name: &str) -> Option<&KindDecl> {
        self.kinds.iter().find(|k| k.name == name)
    }

    /// The first interface named `name`.
    pub fn interface(&self, name: &str) -> Option<&InterfaceDecl> {
        self.interfaces.iter().find(|i| i.name == name)
    }

    /// Parse COSM JSON. Errors name the JSON path that failed.
    pub fn from_json(json: &str) -> Result<Self, crate::CosmError> {
        let de = &mut serde_json::Deserializer::from_str(json);
        serde_path_to_error::deserialize(de).map_err(|e| crate::CosmError::Json {
            path: e.path().to_string(),
            message: e.into_inner().to_string(),
        })
    }

    /// Compact JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("COSM serializes infallibly")
    }

    /// Pretty-printed JSON.
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).expect("COSM serializes infallibly")
    }

    /// [`crate::validate`] this module on its own.
    pub fn validate(&self) -> Vec<crate::Diagnostic> {
        crate::validate(self)
    }
}

impl KindDecl {
    /// A Kind with **Crush's defaults** (design doc, "Crush's own KindDecl"):
    /// sealed (no fields yet), reference semantics, no inheritance,
    /// `explicit_ref` receiver, literal construction, structural equality.
    pub fn crush(name: impl Into<String>) -> Self {
        KindDecl {
            name: name.into(),
            home_lang: "crush".into(),
            span: None,
            storage: Storage::Sealed { fields: Vec::new() },
            semantics: Semantics::Reference,
            parents: Vec::new(),
            resolution: Resolution::None,
            receiver: Receiver::ExplicitRef,
            construct: Construct::Literal,
            members: Vec::new(),
            hooks: Hooks::default(),
            conforms: Vec::new(),
            equality: Equality::Structural,
        }
    }

    pub fn kind_ref(&self) -> KindRef {
        KindRef(self.name.clone())
    }

    /// The first inherent member named `name`.
    pub fn member(&self, name: &str) -> Option<&MemberDecl> {
        self.members.iter().find(|m| m.name == name)
    }

    /// The sealed fields (or `__slots__`), in slot order; empty for `open_dict`.
    pub fn declared_fields(&self) -> &[FieldDecl] {
        self.storage.fields()
    }

    /// The slot index of a declared field (`GET_SLOT i`).
    pub fn field_slot(&self, name: &str) -> Option<usize> {
        self.declared_fields().iter().position(|f| f.name == name)
    }

    /// The receiver a method member actually uses: its override, else the
    /// Kind's default. `None` for non-method members.
    pub fn effective_receiver(&self, member: &MemberDecl) -> Option<Receiver> {
        match &member.member {
            Member::Method { receiver, .. } => Some(receiver.unwrap_or(self.receiver)),
            _ => None,
        }
    }

    /// The conformance to `interface`, if declared.
    pub fn conformance(&self, interface: &str) -> Option<&Conformance> {
        self.conforms.iter().find(|c| c.interface == interface)
    }
}

impl Storage {
    /// Sealed fields or slots, in slot order; empty for `open_dict`.
    pub fn fields(&self) -> &[FieldDecl] {
        match self {
            Storage::Sealed { fields } => fields,
            Storage::OpenDictWithSlots { slots } => slots,
            Storage::OpenDict => &[],
        }
    }

    /// `open_dict` or `open_dict_with_slots`.
    pub fn is_open(&self) -> bool {
        !matches!(self, Storage::Sealed { .. })
    }
}

impl FieldDecl {
    pub fn new(name: impl Into<String>, ty: CosmType) -> Self {
        FieldDecl {
            name: name.into(),
            ty,
            default: None,
            span: None,
        }
    }
}

impl MemberDecl {
    /// A method using the Kind's default receiver.
    pub fn method(name: impl Into<String>, func: impl Into<FnRef>) -> Self {
        MemberDecl::new(
            name,
            Member::Method {
                func: func.into(),
                receiver: None,
            },
        )
    }

    /// A method with a receiver override.
    pub fn method_with(
        name: impl Into<String>,
        func: impl Into<FnRef>,
        receiver: Receiver,
    ) -> Self {
        MemberDecl::new(
            name,
            Member::Method {
                func: func.into(),
                receiver: Some(receiver),
            },
        )
    }

    pub fn field(name: impl Into<String>) -> Self {
        MemberDecl::new(name, Member::Field)
    }

    pub fn accessor(name: impl Into<String>, get: Option<FnRef>, set: Option<FnRef>) -> Self {
        MemberDecl::new(name, Member::Accessor { get, set })
    }

    /// A static function (`T::new`, `@staticmethod`).
    pub fn static_fn(name: impl Into<String>, func: impl Into<FnRef>) -> Self {
        MemberDecl::new(
            name,
            Member::Static {
                value: StaticValue::Fn(func.into()),
            },
        )
    }

    /// A class attribute.
    pub fn static_const(name: impl Into<String>, value: Const) -> Self {
        MemberDecl::new(
            name,
            Member::Static {
                value: StaticValue::Const(value),
            },
        )
    }

    pub fn new(name: impl Into<String>, member: Member) -> Self {
        MemberDecl {
            name: name.into(),
            span: None,
            member,
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    /// True for a method, accessor or static member (anything that is not a field).
    pub fn is_callable_or_static(&self) -> bool {
        !matches!(self.member, Member::Field)
    }
}

impl Conformance {
    pub fn new(interface: impl Into<InterfaceRef>, check: Satisfaction) -> Self {
        Conformance {
            interface: interface.into(),
            check,
            members: Vec::new(),
            span: None,
        }
    }
}

impl InterfaceMember {
    /// A method signature with a receiver and no parameters.
    pub fn method(name: impl Into<String>, receiver: Receiver, returns: CosmType) -> Self {
        InterfaceMember {
            name: name.into(),
            span: None,
            receiver: Some(receiver),
            params: Vec::new(),
            returns,
            default: None,
        }
    }

    pub fn with_default(mut self, func: impl Into<FnRef>) -> Self {
        self.default = Some(func.into());
        self
    }
}

impl InterfaceDecl {
    pub fn member(&self, name: &str) -> Option<&InterfaceMember> {
        self.members.iter().find(|m| m.name == name)
    }
}
