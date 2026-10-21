//! Host constructors: `new Map()` / `new Set()` (CRUSH-288, the
//! CRUSH-zorro-17 slice maya needs first).
//!
//! A JS Map is a CAST array of `[key, value]` pair arrays and a JS
//! Set a CAST array of values — crush's string-keyed `Value::Map`
//! can't hold maya's numeric ids or class-constructor keys, and a
//! missing-key read there is an error where JS returns `undefined`.
//! Arrays are shared references in the VM, so aliasing and mutation
//! behave as in JS. Every operation lowers to a synthesized module
//! helper (`__js_map_get` …) built from the CAST the walker already
//! emits; the helpers are only emitted when a module uses them.
//! Key comparison is the walker's `==` (loose equality): exact for
//! the string/number/bool keys and identity values maya uses.
//!
//! A receiver's host kind is *proven*, never guessed, mirroring the
//! class-instance tracking (CRUSH-277 D4): a binding proven by a
//! declaration initializer or a plain top-level `=` whose right side
//! constructs the collection, forgotten by the same rules; a class
//! field proven when every assignment to `this.f` in the hierarchy
//! proves the same kind, validated at end of module
//! ([`Lower::check_field_host`], the `check_member_calls` pattern).
//! Unproven receivers keep the builtin table's loud refusals.

use std::collections::HashMap;

use boa_ast::expression::access::{PropertyAccess, PropertyAccessField};
use boa_ast::expression::operator::assign::AssignOp;
use boa_ast::{Expression, Span};
use crush_cast::{CastType, Expression as CastExpr, Function, Statement as CastStmt};

use super::stmt::unparen;
use super::{Lower, Meta};
use crate::lower_common::{js_cap, length_of};

/// A JS host collection a value is proven to hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HostKind {
    Map,
    Set,
}

impl HostKind {
    pub(super) fn label(self) -> &'static str {
        match self {
            HostKind::Map => "Map",
            HostKind::Set => "Set",
        }
    }
}

/// A use of a class field as a host collection: validated once the
/// whole module — every assignment in the hierarchy — is lowered.
pub(super) struct FieldHostUse {
    pub class: String,
    pub field: String,
    pub kind: HostKind,
    pub span: Option<Span>,
}

/// The synthesized helpers' names, chosen once per module (hidden
/// names the source doesn't use).
#[derive(Clone)]
pub(super) struct HostFns {
    map_get: String,
    map_set: String,
    map_has: String,
    map_delete: String,
    map_clear: String,
    map_keys: String,
    map_values: String,
    map_entries: String,
    map_from: String,
    set_add: String,
    set_has: String,
    set_delete: String,
    set_clear: String,
    set_from: String,
}

// ── CAST building blocks for the helper bodies ─────────────────────

fn meta() -> Meta {
    HashMap::new()
}

fn var(name: &str) -> CastExpr {
    CastExpr::Var {
        name: name.to_string(),
        meta: meta(),
    }
}

fn int(value: i64) -> CastExpr {
    CastExpr::IntLiteral {
        value,
        meta: meta(),
    }
}

fn boolean(value: bool) -> CastExpr {
    CastExpr::BoolLiteral {
        value,
        meta: meta(),
    }
}

fn null() -> CastExpr {
    CastExpr::NullLiteral { meta: meta() }
}

fn call(function: &str, args: Vec<CastExpr>) -> CastExpr {
    CastExpr::Call {
        function: function.to_string(),
        args,
        meta: meta(),
    }
}

fn idx(target: CastExpr, index: CastExpr) -> CastExpr {
    CastExpr::Index {
        target: Box::new(target),
        index: Box::new(index),
        meta: meta(),
    }
}

fn at(target: &str, index: i64) -> CastExpr {
    idx(var(target), int(index))
}

fn binop(operator: &str, left: CastExpr, right: CastExpr) -> CastExpr {
    CastExpr::BinaryOp {
        operator: operator.to_string(),
        left: Box::new(left),
        right: Box::new(right),
        meta: meta(),
    }
}

fn eq(left: CastExpr, right: CastExpr) -> CastExpr {
    js_cap("js.loose_eq", vec![left, right], meta())
}

fn decl(name: &str, value: CastExpr) -> CastStmt {
    CastStmt::VarDecl {
        name: name.to_string(),
        value,
        type_hint: CastType::Any,
        meta: meta(),
    }
}

fn stmt(expr: CastExpr) -> CastStmt {
    CastStmt::ExprStmt {
        expr,
        meta: meta(),
    }
}

fn assign(name: &str, value: CastExpr) -> CastStmt {
    stmt(call("__crush_assign__", vec![var(name), value]))
}

fn ret(value: CastExpr) -> CastStmt {
    CastStmt::Return {
        value: Some(value),
        meta: meta(),
    }
}

fn if_(condition: CastExpr, then_body: Vec<CastStmt>) -> CastStmt {
    CastStmt::If {
        condition,
        then_body,
        else_body: None,
        meta: meta(),
    }
}

fn while_(condition: CastExpr, body: Vec<CastStmt>) -> CastStmt {
    CastStmt::While {
        condition: Box::new(condition),
        body,
        meta: meta(),
    }
}

fn set_index(target: &str, index: CastExpr, value: CastExpr) -> CastStmt {
    stmt(call(
        "__crush_setindex__",
        vec![var(target), index, value],
    ))
}

fn push(target: &str, value: CastExpr) -> CastStmt {
    stmt(call("array.push", vec![var(target), value]))
}

fn pop(target: &str) -> CastStmt {
    stmt(call("array.pop", vec![var(target)]))
}

fn inc(name: &str) -> CastStmt {
    assign(name, binop("+", var(name), int(1)))
}

/// `while (__i < len(coll)) { … }` with `__i` starting at 0.
fn scan(coll: &str, body: Vec<CastStmt>) -> Vec<CastStmt> {
    let cond = binop("<", var("__i"), length_of(var(coll), meta()));
    vec![decl("__i", int(0)), while_(cond, body)]
}

fn func(params: &[&str], body: Vec<CastStmt>) -> Function {
    Function {
        params: params
            .iter()
            .map(|p| (p.to_string(), CastType::Any))
            .collect(),
        body,
        meta: meta(),
        ..Default::default()
    }
}

/// The helper functions, under the module's chosen names. Map
/// helpers take the pair array first; `__p` is the current pair.
pub(super) fn helpers(fns: &HostFns) -> Vec<(String, Function)> {
    let pair_key = || at("__p", 0);
    let pair_val = || at("__p", 1);
    let get = func(
        &["m", "k"],
        scan(
            "m",
            vec![
                decl("__p", idx(var("m"), var("__i"))),
                if_(eq(pair_key(), var("k")), vec![ret(pair_val())]),
                inc("__i"),
            ],
        )
        .into_iter()
        .chain([ret(null())])
        .collect(),
    );
    let set = func(
        &["m", "k", "v"],
        scan(
            "m",
            vec![
                decl("__p", idx(var("m"), var("__i"))),
                if_(
                    eq(pair_key(), var("k")),
                    vec![set_index("__p", int(1), var("v")), ret(var("m"))],
                ),
                inc("__i"),
            ],
        )
        .into_iter()
        .chain([
            push(
                "m",
                CastExpr::ArrayLiteral {
                    elements: vec![var("k"), var("v")],
                    meta: meta(),
                },
            ),
            ret(var("m")),
        ])
        .collect(),
    );
    let has = func(
        &["m", "k"],
        scan(
            "m",
            vec![
                decl("__p", idx(var("m"), var("__i"))),
                if_(eq(pair_key(), var("k")), vec![ret(boolean(true))]),
                inc("__i"),
            ],
        )
        .into_iter()
        .chain([ret(boolean(false))])
        .collect(),
    );
    // Deleting an element of `s` found at `__i`: shift the tail down
    // one slot and pop the duplicate last element.
    let delete_found = vec![
        while_(
            binop(
                "<",
                var("__i"),
                binop("-", length_of(var("s"), meta()), int(1)),
            ),
            vec![
                set_index(
                    "s",
                    var("__i"),
                    idx(var("s"), binop("+", var("__i"), int(1))),
                ),
                inc("__i"),
            ],
        ),
        pop("s"),
        ret(boolean(true)),
    ];
    let map_delete = func(
        &["s", "k"],
        scan(
            "s",
            vec![
                decl("__p", idx(var("s"), var("__i"))),
                if_(eq(pair_key(), var("k")), delete_found.clone()),
                inc("__i"),
            ],
        )
        .into_iter()
        .chain([ret(boolean(false))])
        .collect(),
    );
    let set_delete = func(
        &["s", "v"],
        scan(
            "s",
            vec![
                if_(
                    eq(idx(var("s"), var("__i")), var("v")),
                    delete_found.clone(),
                ),
                inc("__i"),
            ],
        )
        .into_iter()
        .chain([ret(boolean(false))])
        .collect(),
    );
    let clear = func(
        &["s"],
        vec![
            while_(
                binop(">", length_of(var("s"), meta()), int(0)),
                vec![pop("s")],
            ),
            ret(null()),
        ],
    );
    let collect = |pick: &dyn Fn() -> CastExpr| {
        func(
            &["m"],
            vec![decl("__out", CastExpr::ArrayLiteral {
                elements: vec![],
                meta: meta(),
            })]
            .into_iter()
            .chain(scan(
                "m",
                vec![
                    decl("__p", idx(var("m"), var("__i"))),
                    push("__out", pick()),
                    inc("__i"),
                ],
            ))
            .chain([ret(var("__out"))])
            .collect(),
        )
    };
    let keys = collect(&|| pair_key());
    let values = collect(&|| pair_val());
    let entries = collect(&|| {
        CastExpr::ArrayLiteral {
            elements: vec![pair_key(), pair_val()],
            meta: meta(),
        }
    });
    let map_from = func(
        &["pairs"],
        vec![decl("__m", CastExpr::ArrayLiteral {
            elements: vec![],
            meta: meta(),
        })]
        .into_iter()
        .chain(scan(
            "pairs",
            vec![
                decl("__p", idx(var("pairs"), var("__i"))),
                stmt(call(
                    &fns.map_set,
                    vec![var("__m"), pair_key(), pair_val()],
                )),
                inc("__i"),
            ],
        ))
        .chain([ret(var("__m"))])
        .collect(),
    );
    let set_add = func(
        &["s", "v"],
        scan(
            "s",
            vec![
                if_(
                    eq(idx(var("s"), var("__i")), var("v")),
                    vec![ret(var("s"))],
                ),
                inc("__i"),
            ],
        )
        .into_iter()
        .chain([push("s", var("v")), ret(var("s"))])
        .collect(),
    );
    let set_has = func(
        &["s", "v"],
        scan(
            "s",
            vec![
                if_(
                    eq(idx(var("s"), var("__i")), var("v")),
                    vec![ret(boolean(true))],
                ),
                inc("__i"),
            ],
        )
        .into_iter()
        .chain([ret(boolean(false))])
        .collect(),
    );
    let set_from = func(
        &["a"],
        vec![decl("__s", CastExpr::ArrayLiteral {
            elements: vec![],
            meta: meta(),
        })]
        .into_iter()
        .chain(scan(
            "a",
            vec![
                stmt(call(
                    &fns.set_add,
                    vec![var("__s"), idx(var("a"), var("__i"))],
                )),
                inc("__i"),
            ],
        ))
        .chain([ret(var("__s"))])
        .collect(),
    );
    vec![
        (fns.map_get.clone(), get),
        (fns.map_set.clone(), set),
        (fns.map_has.clone(), has),
        (fns.map_delete.clone(), map_delete),
        (fns.map_clear.clone(), clear.clone()),
        (fns.map_keys.clone(), keys),
        (fns.map_values.clone(), values),
        (fns.map_entries.clone(), entries),
        (fns.map_from.clone(), map_from),
        (fns.set_add.clone(), set_add),
        (fns.set_has.clone(), set_has),
        (fns.set_delete.clone(), set_delete),
        (fns.set_clear.clone(), clear),
        (fns.set_from.clone(), set_from),
    ]
}

impl<'a> Lower<'a> {
    /// The helper names, chosen on first use.
    pub(super) fn host_fns(&mut self) -> HostFns {
        if let Some(fns) = &self.host_fns {
            return fns.clone();
        }
        let fns = HostFns {
            map_get: self.hidden("__js_map_get"),
            map_set: self.hidden("__js_map_set"),
            map_has: self.hidden("__js_map_has"),
            map_delete: self.hidden("__js_map_delete"),
            map_clear: self.hidden("__js_map_clear"),
            map_keys: self.hidden("__js_map_keys"),
            map_values: self.hidden("__js_map_values"),
            map_entries: self.hidden("__js_map_entries"),
            map_from: self.hidden("__js_map_from"),
            set_add: self.hidden("__js_set_add"),
            set_has: self.hidden("__js_set_has"),
            set_delete: self.hidden("__js_set_delete"),
            set_clear: self.hidden("__js_set_clear"),
            set_from: self.hidden("__js_set_from"),
        };
        self.host_fns = Some(fns.clone());
        fns
    }

    /// The host kind `e` constructs, when `e` is `new Map()` /
    /// `new Set()` of the *host* constructors: a user class or
    /// binding of that name shadows them.
    pub(super) fn host_kind_of(&self, e: &Expression) -> Option<HostKind> {
        let Expression::New(n) = unparen(e) else {
            return None;
        };
        let Expression::Identifier(id) = unparen(n.constructor()) else {
            return None;
        };
        self.host_kind_of_constructor(id)
    }

    /// The host kind a `new <id>(...)` constructs, when `<id>` is
    /// the host `Map`/`Set` and not shadowed by a user class or
    /// binding of that name.
    pub(super) fn host_kind_of_constructor(
        &self,
        id: &boa_ast::expression::Identifier,
    ) -> Option<HostKind> {
        if self.user_binding(id.sym()).is_some() || self.is_kind(&self.resolve(id.sym())) {
            return None;
        }
        match self.name(id.sym()).as_str() {
            "Map" => Some(HostKind::Map),
            "Set" => Some(HostKind::Set),
            _ => None,
        }
    }

    /// `new Map()` / `new Set(...)`: an empty pair/value array, or
    /// one built from an iterable argument by a helper.
    pub(super) fn new_host(
        &mut self,
        kind: HostKind,
        args: &[Expression],
        span: Option<Span>,
        m: Meta,
    ) -> anyhow::Result<CastExpr> {
        match args {
            [] => Ok(CastExpr::ArrayLiteral {
                elements: vec![],
                meta: m,
            }),
            [arg] => {
                let fns = self.host_fns();
                let f = match kind {
                    HostKind::Map => fns.map_from,
                    HostKind::Set => fns.set_from,
                };
                Ok(CastExpr::Call {
                    function: f,
                    args: vec![self.expr(arg)?],
                    meta: m,
                })
            }
            _ => Err(self.unsupported(
                format!(
                    "`new {}` with more than one argument",
                    kind.label()
                ),
                span,
            )),
        }
    }

    /// The host kind a receiver expression provably holds, if any:
    /// a tracked binding, a tracked `this.field`, or a collection
    /// constructed on the spot. A field use that proves out is
    /// recorded for the end-of-module check.
    pub(super) fn host_proof(
        &mut self,
        target: &Expression,
        span: Option<Span>,
    ) -> Option<HostKind> {
        match unparen(target) {
            Expression::Identifier(id) => {
                let name = self.resolve(id.sym());
                if self.fn_assigned.contains(&name) {
                    None
                } else {
                    self.host_kinds.get(&name).copied()
                }
            }
            Expression::New(_) => self.host_kind_of(target),
            // `new Map().set(k, v).get(k)`: `set`/`add` return the
            // collection, so the proof flows through them.
            Expression::Call(c) => {
                let Expression::PropertyAccess(PropertyAccess::Simple(s)) =
                    unparen(c.function())
                else {
                    return None;
                };
                let PropertyAccessField::Const(method) = s.field() else {
                    return None;
                };
                let kind = self.host_proof(s.target(), span)?;
                let returns_self = matches!(
                    (kind, self.name(method.sym()).as_str()),
                    (HostKind::Map, "set") | (HostKind::Set, "add")
                );
                returns_self.then_some(kind)
            }
            Expression::PropertyAccess(PropertyAccess::Simple(s)) => {
                if !self.this_bound
                    || self.in_static
                    || !matches!(unparen(s.target()), Expression::This(_))
                {
                    return None;
                }
                let PropertyAccessField::Const(field) = s.field() else {
                    return None;
                };
                let field = self.name(field.sym());
                let home = self.home_class.clone()?;
                let mut cur = Some(home.clone());
                let mut seen = std::collections::HashSet::new();
                while let Some(class) = cur {
                    if !seen.insert(class.clone()) {
                        break;
                    }
                    let key = (class.clone(), field.clone());
                    if self.field_host_poisons.contains(&key) {
                        return None;
                    }
                    if let Some(kind) = self.field_host_proofs.get(&key).copied() {
                        self.field_host_uses.push(FieldHostUse {
                            class: home,
                            field,
                            kind,
                            span,
                        });
                        return Some(kind);
                    }
                    cur = self.class_parents.get(&class).cloned();
                }
                None
            }
            _ => None,
        }
    }

    /// Record one assignment to a class field: a host-kind proof,
    /// or a poison when the assigned value doesn't prove one (or
    /// proves a different one).
    pub(super) fn record_field_host(
        &mut self,
        class: &str,
        field: &str,
        proof: Option<HostKind>,
    ) {
        let key = (class.to_string(), field.to_string());
        match proof {
            Some(kind) => {
                if let Some(prev) = self.field_host_proofs.get(&key)
                    && *prev != kind
                {
                    self.field_host_poisons.insert(key);
                } else {
                    self.field_host_proofs.insert(key, kind);
                }
            }
            None => {
                self.field_host_poisons.insert(key);
            }
        }
    }

    /// Record an assignment to `this.field` in the class being
    /// lowered: a plain `=` constructing a collection proves the
    /// field's host kind; anything else poisons the proof. In a
    /// static, `this` is the class value, not an instance — its
    /// fields are a different namespace and stay untracked.
    pub(super) fn note_field_assign(
        &mut self,
        field: &str,
        op: AssignOp,
        rhs: &Expression,
    ) {
        if self.in_static {
            return;
        }
        let Some(home) = self.home_class.clone() else {
            return;
        };
        let proof = if op == AssignOp::Assign {
            self.host_kind_of(rhs)
        } else {
            None
        };
        self.record_field_host(&home, field, proof);
    }

    /// A method call on a proven host receiver: the Map/Set
    /// operations, with exact arities. Anything else — a wrong-kind
    /// method, a callback method, an unknown name — refuses here
    /// rather than reaching the builtin table or the member-call
    /// machinery, which would misread the array representation.
    pub(super) fn host_method_call(
        &mut self,
        kind: HostKind,
        recv: CastExpr,
        method: &str,
        args: &[Expression],
        span: Option<Span>,
        m: Meta,
    ) -> anyhow::Result<CastExpr> {
        let fns = self.host_fns();
        let (f, arity) = match (kind, method) {
            (HostKind::Map, "get") => (fns.map_get, 1),
            (HostKind::Map, "set") => (fns.map_set, 2),
            (HostKind::Map, "has") => (fns.map_has, 1),
            (HostKind::Map, "delete") => (fns.map_delete, 1),
            (HostKind::Map, "clear") => (fns.map_clear, 0),
            (HostKind::Map, "keys") => (fns.map_keys, 0),
            (HostKind::Map, "values") => (fns.map_values, 0),
            (HostKind::Map, "entries") => (fns.map_entries, 0),
            (HostKind::Set, "add") => (fns.set_add, 1),
            (HostKind::Set, "has") => (fns.set_has, 1),
            (HostKind::Set, "delete") => (fns.set_delete, 1),
            (HostKind::Set, "clear") => (fns.set_clear, 0),
            _ => {
                let known = matches!(
                    method,
                    "get" | "set" | "has" | "add" | "delete" | "clear" | "keys" | "values"
                        | "entries"
                );
                return Err(self.unsupported(
                    if known {
                        format!(
                            "a {} has no method `.{method}()`",
                            kind.label()
                        )
                    } else {
                        format!(
                            "the method `.{method}()` on a {}",
                            kind.label()
                        )
                    },
                    span,
                ));
            }
        };
        if args.len() != arity {
            return Err(self.unsupported(
                format!(
                    "`{}.{method}()` expects {arity} argument(s), got {}",
                    kind.label(),
                    args.len()
                ),
                span,
            ));
        }
        let mut lowered = vec![recv];
        lowered.extend(self.args(args)?);
        Ok(CastExpr::Call {
            function: f,
            args: lowered,
            meta: m,
        })
    }

    /// End-of-module validation of field host uses (CRUSH-288): a
    /// use assumed the proof visible from its class, but an
    /// assignment anywhere in the hierarchy — a subclass constructor
    /// reassigning an inherited field, a method lowered later —
    /// can break that proof after the fact. Refuse then, loudly,
    /// rather than let a call read the wrong representation.
    pub(super) fn check_field_host(&self) -> anyhow::Result<()> {
        for u in &self.field_host_uses {
            // The hierarchy the use can dispatch over: its class,
            // its ancestors, and every descendant of any of them.
            let mut tree: Vec<String> = vec![u.class.clone()];
            let mut cur = self.class_parents.get(&u.class).cloned();
            let mut seen = std::collections::HashSet::new();
            while let Some(class) = cur {
                if !seen.insert(class.clone()) {
                    break;
                }
                tree.push(class.clone());
                cur = self.class_parents.get(&class).cloned();
            }
            for decl in &self.kinds {
                if decl.name == u.class || tree.contains(&decl.name) {
                    continue;
                }
                let mut cur = self.class_parents.get(&decl.name).cloned();
                let mut seen = std::collections::HashSet::new();
                while let Some(class) = cur {
                    if !seen.insert(class.clone()) {
                        break;
                    }
                    if tree.contains(&class) {
                        tree.push(decl.name.clone());
                        break;
                    }
                    cur = self.class_parents.get(&class).cloned();
                }
            }
            for class in &tree {
                let key = (class.clone(), u.field.clone());
                let conflict = self.field_host_poisons.contains(&key)
                    || self
                        .field_host_proofs
                        .get(&key)
                        .is_some_and(|kind| *kind != u.kind);
                if conflict {
                    return Err(self.unsupported(
                        format!(
                            "the {} kind of the field `{}` on `{}` cannot be proven: the field is assigned a value that is not a fresh {} somewhere in the class hierarchy",
                            u.kind.label(),
                            u.field,
                            u.class,
                            u.kind.label()
                        ),
                        u.span,
                    ));
                }
            }
        }
        Ok(())
    }
}
