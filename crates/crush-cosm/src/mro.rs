//! Resolution orders: C3 linearization (exactly CPython's) and the linear
//! orders of the other strategies.

use crate::schema::{CosmModule, InterfaceDecl, KindDecl, KindRef, Resolution};
use std::collections::HashMap;
use std::fmt;

/// Finds kinds (and interfaces) by name. Implemented by [`CosmModule`] and by
/// [`ModuleSet`] (a module plus its dependencies).
pub trait KindLookup {
    fn lookup_kind(&self, name: &str) -> Option<&KindDecl>;
    fn lookup_interface(&self, name: &str) -> Option<&InterfaceDecl>;
}

impl KindLookup for CosmModule {
    fn lookup_kind(&self, name: &str) -> Option<&KindDecl> {
        self.kind(name)
    }
    fn lookup_interface(&self, name: &str) -> Option<&InterfaceDecl> {
        self.interface(name)
    }
}

/// A module together with the modules it builds on (e.g. a Python program
/// and the prelude that declares `object`). Lookup tries the modules in
/// order, so the first module shadows the rest.
#[derive(Debug, Clone, Copy)]
pub struct ModuleSet<'a> {
    primary: &'a CosmModule,
    deps: &'a [&'a CosmModule],
}

impl<'a> ModuleSet<'a> {
    pub fn new(primary: &'a CosmModule, deps: &'a [&'a CosmModule]) -> Self {
        ModuleSet { primary, deps }
    }

    pub fn modules(&self) -> impl Iterator<Item = &'a CosmModule> + '_ {
        std::iter::once(self.primary).chain(self.deps.iter().copied())
    }
}

impl KindLookup for ModuleSet<'_> {
    fn lookup_kind(&self, name: &str) -> Option<&KindDecl> {
        self.modules().find_map(|m| m.kind(name))
    }
    fn lookup_interface(&self, name: &str) -> Option<&InterfaceDecl> {
        self.modules().find_map(|m| m.interface(name))
    }
}

/// Why a resolution order could not be computed. Each variant names the kind
/// whose order failed ([`MroError::kind`]); a failure in a parent propagates
/// unchanged to its descendants.
///
/// The two C3 failures Python can raise display **exactly** as CPython's
/// `TypeError` messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MroError {
    /// The kind (or a parent) is not declared.
    UnknownKind { kind: KindRef },
    /// A parent is listed twice. Python: `duplicate base class X`.
    DuplicateBase { kind: KindRef, base: KindRef },
    /// The C3 merge got stuck. `bases` are the heads of the lists that were
    /// left, in CPython's order. Python: `Cannot create a consistent method
    /// resolution order (MRO) for bases X, Y`.
    Inconsistent { kind: KindRef, bases: Vec<KindRef> },
    /// The kind inherits from itself; `path` runs from `kind` back to `kind`.
    Cycle { kind: KindRef, path: Vec<KindRef> },
    /// The parent count doesn't fit the resolution strategy.
    ParentArity { kind: KindRef, resolution: Resolution, parents: usize },
    /// `embedding_promotion` has no linear order (promotion is by depth,
    /// with ambiguity resolved at each use).
    NotLinear { kind: KindRef, resolution: Resolution },
}

impl MroError {
    /// The kind whose resolution order failed.
    pub fn kind(&self) -> &KindRef {
        match self {
            MroError::UnknownKind { kind }
            | MroError::DuplicateBase { kind, .. }
            | MroError::Inconsistent { kind, .. }
            | MroError::Cycle { kind, .. }
            | MroError::ParentArity { kind, .. }
            | MroError::NotLinear { kind, .. } => kind,
        }
    }
}

impl fmt::Display for MroError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MroError::UnknownKind { kind } => write!(f, "unknown kind `{kind}`"),
            MroError::DuplicateBase { base, .. } => write!(f, "duplicate base class {base}"),
            MroError::Inconsistent { bases, .. } => {
                let names: Vec<&str> = bases.iter().map(KindRef::as_str).collect();
                write!(
                    f,
                    "Cannot create a consistent method resolution order (MRO) for bases {}",
                    names.join(", ")
                )
            }
            MroError::Cycle { path, .. } => {
                let names: Vec<&str> = path.iter().map(KindRef::as_str).collect();
                write!(f, "inheritance cycle: {}", names.join(" -> "))
            }
            MroError::ParentArity { kind, resolution, parents } => {
                let allowed = match resolution {
                    Resolution::None => "no parents",
                    _ => "at most one parent",
                };
                write!(
                    f,
                    "`{kind}` has {parents} parent(s), but resolution `{}` allows {allowed}",
                    crate::catalogue::Strategy::as_str(*resolution)
                )
            }
            MroError::NotLinear { kind, resolution } => write!(
                f,
                "`{kind}` uses resolution `{}`, which has no linear resolution order",
                crate::catalogue::Strategy::as_str(*resolution)
            ),
        }
    }
}

impl std::error::Error for MroError {}

/// The C3 linearization of `kind` (Python's `__mro__`), starting with `kind`.
///
/// C3 is applied at `kind` whatever its declared `resolution`; each parent
/// contributes its own [`resolution_order`]. The merge is CPython's
/// `mro_implementation`/`pmerge`, including its error cases and the order in
/// which an inconsistent merge reports the stuck bases.
pub fn c3_mro(kind: &KindRef, lookup: &impl KindLookup) -> Result<Vec<KindRef>, MroError> {
    Linearizer::new(lookup).c3(kind)
}

/// The order in which member lookup visits `kind` and its ancestors, by the
/// kind's own `resolution` strategy:
/// - `none`: `[kind]` (no parents allowed);
/// - `single` / `prototype_chain`: `kind` followed by its one parent's order;
/// - `c3`: [`c3_mro`];
/// - `embedding_promotion`: [`MroError::NotLinear`].
pub fn resolution_order(kind: &KindRef, lookup: &impl KindLookup) -> Result<Vec<KindRef>, MroError> {
    Linearizer::new(lookup).resolution_order(kind)
}

/// Computes resolution orders with memoization: use one `Linearizer` to
/// compute the orders of many kinds of the same module (VM load, validation).
pub struct Linearizer<'l, L: KindLookup> {
    lookup: &'l L,
    memo: HashMap<String, Result<Vec<KindRef>, MroError>>,
    stack: Vec<KindRef>,
}

impl<'l, L: KindLookup> Linearizer<'l, L> {
    pub fn new(lookup: &'l L) -> Self {
        Linearizer { lookup, memo: HashMap::new(), stack: Vec::new() }
    }

    /// See [`resolution_order`].
    pub fn resolution_order(&mut self, kind: &KindRef) -> Result<Vec<KindRef>, MroError> {
        if let Some(done) = self.memo.get(kind.as_str()) {
            return done.clone();
        }
        if let Some(pos) = self.stack.iter().position(|k| k == kind) {
            let mut path: Vec<KindRef> = self.stack[pos..].to_vec();
            path.push(kind.clone());
            return Err(MroError::Cycle { kind: kind.clone(), path });
        }
        let decl = self
            .lookup
            .lookup_kind(kind.as_str())
            .ok_or_else(|| MroError::UnknownKind { kind: kind.clone() })?;
        self.stack.push(kind.clone());
        let result = self.order_of(kind, decl);
        self.stack.pop();
        self.memo.insert(kind.as_str().to_string(), result.clone());
        result
    }

    /// See [`c3_mro`].
    pub fn c3(&mut self, kind: &KindRef) -> Result<Vec<KindRef>, MroError> {
        let decl = self
            .lookup
            .lookup_kind(kind.as_str())
            .ok_or_else(|| MroError::UnknownKind { kind: kind.clone() })?;
        if decl.resolution == Resolution::C3 {
            return self.resolution_order(kind);
        }
        if self.stack.iter().any(|k| k == kind) {
            return self.resolution_order(kind); // reports the cycle
        }
        self.stack.push(kind.clone());
        let result = self.c3_of(kind, decl);
        self.stack.pop();
        result
    }

    fn order_of(&mut self, kind: &KindRef, decl: &KindDecl) -> Result<Vec<KindRef>, MroError> {
        let arity = |resolution| MroError::ParentArity {
            kind: kind.clone(),
            resolution,
            parents: decl.parents.len(),
        };
        match decl.resolution {
            Resolution::None => {
                if decl.parents.is_empty() {
                    Ok(vec![kind.clone()])
                } else {
                    Err(arity(Resolution::None))
                }
            }
            r @ (Resolution::Single | Resolution::PrototypeChain) => match decl.parents.as_slice() {
                [] => Ok(vec![kind.clone()]),
                [parent] => {
                    let mut order = vec![kind.clone()];
                    order.extend(self.resolution_order(parent)?);
                    Ok(order)
                }
                _ => Err(arity(r)),
            },
            Resolution::C3 => self.c3_of(kind, decl),
            r @ Resolution::EmbeddingPromotion => Err(MroError::NotLinear { kind: kind.clone(), resolution: r }),
        }
    }

    /// CPython `mro_implementation` + `pmerge` (Objects/typeobject.c).
    fn c3_of(&mut self, kind: &KindRef, decl: &KindDecl) -> Result<Vec<KindRef>, MroError> {
        let bases = &decl.parents;
        // Each base's own order first: in Python a base whose MRO fails was
        // never created, so its error comes first.
        let mut to_merge: Vec<Vec<KindRef>> = Vec::with_capacity(bases.len() + 1);
        for base in bases {
            to_merge.push(self.resolution_order(base)?);
        }
        if bases.len() == 1 {
            // CPython's single-base fast path: [kind] + base.__mro__.
            let mut order = vec![kind.clone()];
            order.append(&mut to_merge[0]);
            return Ok(order);
        }
        // check_duplicates: the first base that appears again.
        for (i, base) in bases.iter().enumerate() {
            if bases[i + 1..].contains(base) {
                return Err(MroError::DuplicateBase { kind: kind.clone(), base: base.clone() });
            }
        }
        to_merge.push(bases.clone());

        let mut order = vec![kind.clone()];
        let mut remain = vec![0usize; to_merge.len()];
        'again: loop {
            let mut empty = 0;
            for i in 0..to_merge.len() {
                if remain[i] >= to_merge[i].len() {
                    empty += 1;
                    continue;
                }
                let candidate = &to_merge[i][remain[i]];
                let in_a_tail = to_merge
                    .iter()
                    .zip(&remain)
                    .any(|(list, &r)| list.len() > r + 1 && list[r + 1..].contains(candidate));
                if in_a_tail {
                    continue;
                }
                let candidate = candidate.clone();
                for (list, r) in to_merge.iter().zip(remain.iter_mut()) {
                    if *r < list.len() && list[*r] == candidate {
                        *r += 1;
                    }
                }
                order.push(candidate);
                continue 'again;
            }
            if empty == to_merge.len() {
                return Ok(order);
            }
            // set_mro_error: the remaining heads, deduplicated, in list order.
            let mut stuck: Vec<KindRef> = Vec::new();
            for (list, &r) in to_merge.iter().zip(&remain) {
                if r < list.len() && !stuck.contains(&list[r]) {
                    stuck.push(list[r].clone());
                }
            }
            return Err(MroError::Inconsistent { kind: kind.clone(), bases: stuck });
        }
    }
}

impl CosmModule {
    /// [`c3_mro`] within this module alone.
    pub fn c3_mro(&self, kind: &str) -> Result<Vec<KindRef>, MroError> {
        c3_mro(&KindRef::from(kind), self)
    }

    /// [`resolution_order`] within this module alone.
    pub fn resolution_order(&self, kind: &str) -> Result<Vec<KindRef>, MroError> {
        resolution_order(&KindRef::from(kind), self)
    }
}
