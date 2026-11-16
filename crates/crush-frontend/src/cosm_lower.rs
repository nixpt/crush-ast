//! COSM K1.2 (CRUSH-zorro-41): Crush's `struct` / `impl` / `trait` /
//! `@kind(…)` lowered to COSM.
//!
//! Runs on the CAST before semantic analysis and code generation:
//!
//! 1. collects every `struct`, `impl` and `trait` declared at the top level of
//!    the file (an `impl` may appear anywhere, before or after its `struct`);
//! 2. checks them (unknown types and traits, duplicate members, missing trait
//!    methods, `self` outside an impl, …) and reports every problem with its
//!    source position;
//! 3. **lifts** each method to an ordinary program function with `self` as
//!    its first parameter (`Point::norm2`, `Point::Shape::area`,
//!    `Shape::describe` for a trait default), and removes the `impl` / `trait`
//!    nodes from the CAST, so the compiler and the VMs see only functions;
//! 4. builds one COSM [`KindDecl`] per struct (all its impls merged) with
//!    Crush's defaults overridden by `@kind(…)`, one [`InterfaceDecl`] per
//!    trait, and runs [`crush_cosm::validate`] on the module; its diagnostics
//!    become compile errors.
//!
//! `p.norm2()` resolves the method on the receiver's Kind (`call_member`,
//! COSM K1.4, CRUSH-zorro-43; see [`receiver_methods`]) on the scheduler VM
//! and the portable VM. Qualified calls (`Point::new(1, 2)`,
//! `Point::norm2(p)`) are plain calls of the lifted functions and run on every
//! engine.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Result, bail};
use crush_cast::{CastType, Function, KindAnnotation, MethodDef, Program, Statement};
use serde_json::Value;

use crate::cosm_kind;

/// `"line L, col C: "` for a CAST node's meta, or `""` when it has no position.
pub(crate) fn span_prefix(meta: &HashMap<String, Value>) -> String {
    match span_of(meta) {
        Some((line, col)) => format!("line {line}, col {col}: "),
        None => String::new(),
    }
}

pub(crate) fn span_of(meta: &HashMap<String, Value>) -> Option<(usize, usize)> {
    let line = meta.get("line")?.as_u64()? as usize;
    let col = meta.get("col").and_then(Value::as_u64).unwrap_or(0) as usize;
    (line > 0).then_some((line, col))
}

/// The lifted name of an inherent method: `Point::norm2`.
pub fn inherent_fn_name(type_name: &str, method: &str) -> String {
    format!("{type_name}::{method}")
}

/// The lifted name of a trait method implemented for a type: `Point::Shape::area`.
pub fn trait_impl_fn_name(type_name: &str, trait_name: &str, method: &str) -> String {
    format!("{type_name}::{trait_name}::{method}")
}

/// The lifted name of a trait's default method: `Shape::describe`.
pub fn trait_default_fn_name(trait_name: &str, method: &str) -> String {
    format!("{trait_name}::{method}")
}

/// An `impl` node taken out of the CAST: type, trait, methods, meta.
type ImplNode = (
    String,
    Option<String>,
    Vec<MethodDef>,
    HashMap<String, Value>,
);

/// A struct as declared, plus everything its impls add.
#[derive(Debug, Clone)]
pub(crate) struct TypePlan {
    pub name: String,
    pub fields: Vec<(String, CastType)>,
    pub kind: Option<KindAnnotation>,
    pub meta: HashMap<String, Value>,
    /// Inherent members (`impl P { … }`), in source order.
    pub inherent: Vec<MemberPlan>,
    /// `impl T for P { … }` blocks, in source order.
    pub trait_impls: Vec<TraitImplPlan>,
}

#[derive(Debug, Clone)]
pub(crate) struct MemberPlan {
    /// The member's name in the KindDecl (`norm2`, `Shape::area`).
    pub name: String,
    /// The lifted function it runs.
    pub function: String,
    /// `self` first → a method; otherwise a static member.
    pub has_self: bool,
    pub meta: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
pub(crate) struct TraitImplPlan {
    pub trait_name: String,
    /// The `impl T for P` block.
    pub meta: HashMap<String, Value>,
    /// One member per trait method (own impl or the trait's default),
    /// named `Trait::method`, in the trait's declaration order.
    pub members: Vec<MemberPlan>,
}

#[derive(Debug, Clone)]
pub(crate) struct TraitPlan {
    pub name: String,
    pub methods: Vec<MethodDef>,
    pub meta: HashMap<String, Value>,
}

/// Everything the file declares, checked.
#[derive(Debug, Clone, Default)]
pub(crate) struct Plan {
    pub types: Vec<TypePlan>,
    pub traits: Vec<TraitPlan>,
}

/// Lower the program's `struct` / `impl` / `trait` declarations to COSM.
///
/// Mutates `program`: the `impl` / `trait` nodes are removed and their
/// methods are added to `program.functions` under their lifted names.
/// Returns `None` for a program that declares no struct, impl or trait (it
/// is left untouched). Every problem found is reported in one error, one
/// line per problem, each with its source position.
pub fn lower(program: &mut Program) -> Result<Option<crush_cosm::CosmModule>> {
    inherit_declaration_owners(program)?;
    normalize_package_ids(program)?;
    let Some(plan) = plan(program)? else {
        return Ok(None);
    };
    let module = cosm_kind::build_module(&plan)?;
    Ok(Some(module))
}

/// Merge a walker-supplied COSM module (CAST `Program::cosm`, CRUSH-277 S1)
/// with the module lowered from the program's own Crush surface declarations.
///
/// A walker (the JS lowering first) lowers its language's object declarations
/// straight to COSM and rides them in the CAST; this is where they join the
/// program's one kind table. A kind or interface name declared by both sides
/// is an error — silently keeping either would give one name two behaviours.
/// An empty walker module counts as absent. The merged module is validated
/// like a lowered one, so a walker's broken declaration fails compilation
/// here, with its spans, instead of at VM load.
pub fn merge_walker_module(
    walker: Option<crush_cosm::CosmModule>,
    derived: Option<crush_cosm::CosmModule>,
) -> Result<Option<crush_cosm::CosmModule>> {
    let Some(walker) = walker.filter(|m| !m.is_empty()) else {
        return Ok(derived);
    };
    let Some(mut module) = derived else {
        validate_module(&walker)?;
        return Ok(Some(walker));
    };
    let mut errors: Vec<String> = Vec::new();
    for k in &walker.kinds {
        if module.kind(&k.name).is_some() {
            errors.push(format!(
                "kind `{}` is declared both by the walker (CAST `cosm`) and by the program's own declarations",
                k.name
            ));
        }
    }
    for i in &walker.interfaces {
        if module.interface(&i.name).is_some() {
            errors.push(format!(
                "interface `{}` is declared both by the walker (CAST `cosm`) and by the program's own declarations",
                i.name
            ));
        }
    }
    if !errors.is_empty() {
        bail!("{}", errors.join("\n"));
    }
    module.kinds.extend(walker.kinds);
    module.interfaces.extend(walker.interfaces);
    module.fn_packages.extend(walker.fn_packages);
    validate_module(&module)?;
    Ok(Some(module))
}

/// [`crush_cosm::validate`] as compile errors, formatted like
/// [`crate::cosm_kind`]'s.
fn validate_module(module: &crush_cosm::CosmModule) -> Result<()> {
    let errors: Vec<String> = crush_cosm::validate(module)
        .iter()
        .map(|d| {
            let at = d
                .span
                .as_ref()
                .map(|s| format!("line {}, col {}: ", s.line, s.col))
                .unwrap_or_default();
            format!("{at}{}: {} ({})", d.subject, d.message, d.code)
        })
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        bail!("{}", errors.join("\n"))
    }
}

/// Whether `program` still has `impl` / `trait` nodes to lower.
pub fn has_impl_or_trait(program: &Program) -> bool {
    program.functions.values().any(|f| {
        f.body
            .iter()
            .any(|s| matches!(s, Statement::ImplBlock { .. } | Statement::TraitDef { .. }))
    })
}

/// Phase 1: collect, check and lift. Returns `None` when there is nothing to
/// lower.
pub(crate) fn plan(program: &mut Program) -> Result<Option<Plan>> {
    let declares = program.functions.values().any(|f| {
        f.body.iter().any(|s| {
            matches!(
                s,
                Statement::StructDef { .. }
                    | Statement::ImplBlock { .. }
                    | Statement::TraitDef { .. }
            )
        })
    });
    if !declares {
        return Ok(None);
    }

    let mut errors: Vec<String> = Vec::new();

    // ── collect ─────────────────────────────────────────────────────────────
    // Structs stay in the CAST (they are module-init statements today);
    // impls and traits are taken out — after lowering they are functions.
    let mut types: Vec<TypePlan> = Vec::new();
    let mut impls: Vec<ImplNode> = Vec::new();
    let mut traits: Vec<TraitPlan> = Vec::new();
    let mut fn_names: Vec<&String> = program.functions.keys().collect();
    fn_names.sort();
    let fn_names: Vec<String> = fn_names.into_iter().cloned().collect();
    for fname in &fn_names {
        let func = program.functions.get_mut(fname).expect("listed");
        let body = std::mem::take(&mut func.body);
        for stmt in body {
            match stmt {
                Statement::ImplBlock {
                    type_name,
                    trait_name,
                    methods,
                    meta,
                } => {
                    impls.push((type_name, trait_name, methods, meta));
                }
                Statement::TraitDef {
                    name,
                    methods,
                    meta,
                } => {
                    traits.push(TraitPlan {
                        name,
                        methods,
                        meta,
                    });
                }
                Statement::StructDef {
                    ref name,
                    ref fields,
                    ref kind,
                    ref meta,
                } => {
                    if let Some(prev) = types.iter_mut().find(|t| &t.name == name) {
                        // Two `struct P` declarations: the parser's struct table
                        // keeps the last one, so the Kind does too — but a type
                        // that carries methods or `@kind` must be unambiguous.
                        if kind.is_some() || prev.kind.is_some() {
                            errors.push(format!(
                                "{}struct `{name}` is declared twice (first at {})",
                                span_prefix(meta),
                                describe_pos(&prev.meta)
                            ));
                        }
                        prev.fields = fields.clone();
                        prev.kind = kind.clone();
                        prev.meta = meta.clone();
                    } else {
                        types.push(TypePlan {
                            name: name.clone(),
                            fields: fields.clone(),
                            kind: kind.clone(),
                            meta: meta.clone(),
                            inherent: Vec::new(),
                            trait_impls: Vec::new(),
                        });
                    }
                    func.body.push(stmt);
                }
                other => func.body.push(other),
            }
        }
    }

    // ── traits ──────────────────────────────────────────────────────────────
    let mut seen_traits: BTreeMap<String, HashMap<String, Value>> = BTreeMap::new();
    for t in &traits {
        if let Some(first) = seen_traits.get(&t.name) {
            errors.push(format!(
                "{}trait `{}` is declared twice (first at {})",
                span_prefix(&t.meta),
                t.name,
                describe_pos(first)
            ));
            continue;
        }
        if types.iter().any(|ty| ty.name == t.name) {
            errors.push(format!(
                "{}trait `{}` has the same name as a struct; a name is either a type or a trait",
                span_prefix(&t.meta),
                t.name
            ));
        }
        seen_traits.insert(t.name.clone(), t.meta.clone());
        check_methods(&format!("trait {}", t.name), &t.methods, &mut errors);
    }

    // ── impls ───────────────────────────────────────────────────────────────
    let mut lifted: Vec<(String, Function)> = Vec::new();
    for t in &traits {
        for m in &t.methods {
            if let Some(body) = &m.body {
                lifted.push((
                    trait_default_fn_name(&t.name, &m.name),
                    lift(m, body, &t.meta),
                ));
            }
        }
    }
    // Inherent member name → where it was first defined, per type.
    let mut inherent_seen: BTreeMap<(String, String), HashMap<String, Value>> = BTreeMap::new();
    for (type_name, trait_name, methods, meta) in &impls {
        let what = match trait_name {
            Some(t) => format!("impl {t} for {type_name}"),
            None => format!("impl {type_name}"),
        };
        let Some(ty_idx) = types.iter().position(|t| &t.name == type_name) else {
            let hint = if traits.iter().any(|t| &t.name == type_name) {
                format!(" (`{type_name}` is a trait; write `impl {type_name} for <Type>`)")
            } else {
                format!(" (declare `struct {type_name} {{ … }}` in this file)")
            };
            errors.push(format!(
                "{}`{what}`: no struct `{type_name}` is declared{hint}",
                span_prefix(meta)
            ));
            continue;
        };
        check_methods(&what, methods, &mut errors);
        match trait_name {
            None => {
                for m in methods {
                    let key = (type_name.clone(), m.name.clone());
                    if let Some(first) = inherent_seen.get(&key) {
                        errors.push(format!(
                            "{}duplicate member `{}` in `{what}` (first defined at {})",
                            span_prefix(&m.meta),
                            m.name,
                            describe_pos(first)
                        ));
                        continue;
                    }
                    inherent_seen.insert(key, m.meta.clone());
                    let function = inherent_fn_name(type_name, &m.name);
                    if let Some(body) = &m.body {
                        lifted.push((function.clone(), lift(m, body, meta)));
                    }
                    types[ty_idx].inherent.push(MemberPlan {
                        name: m.name.clone(),
                        function,
                        has_self: m.has_self(),
                        meta: m.meta.clone(),
                    });
                }
            }
            Some(trait_name) => {
                let Some(tr) = traits.iter().find(|t| &t.name == trait_name) else {
                    errors.push(format!(
                        "{}`{what}`: no trait `{trait_name}` is declared in this file",
                        span_prefix(meta)
                    ));
                    continue;
                };
                if types[ty_idx]
                    .trait_impls
                    .iter()
                    .any(|ti| &ti.trait_name == trait_name)
                {
                    errors.push(format!(
                        "{}`{what}` is written twice; implement a trait once per type",
                        span_prefix(meta)
                    ));
                    continue;
                }
                for m in methods {
                    match tr.methods.iter().find(|tm| tm.name == m.name) {
                        None => errors.push(format!(
                            "{}`{}` is not a method of trait `{trait_name}` (`{what}`); \
                             trait `{trait_name}` declares: {}",
                            span_prefix(&m.meta),
                            m.name,
                            list_or_none(tr.methods.iter().map(|tm| tm.name.as_str()))
                        )),
                        Some(tm) => {
                            if tm.has_self() != m.has_self() {
                                errors.push(format!(
                                    "{}`{}` {} `self` in trait `{trait_name}` but {} in `{what}`",
                                    span_prefix(&m.meta),
                                    m.name,
                                    if tm.has_self() {
                                        "takes"
                                    } else {
                                        "does not take"
                                    },
                                    if m.has_self() { "does" } else { "does not" }
                                ));
                            } else if tm.params.len() != m.params.len() {
                                errors.push(format!(
                                    "{}`{}` takes {} parameter(s) in trait `{trait_name}` but {} in `{what}`",
                                    span_prefix(&m.meta),
                                    m.name,
                                    tm.params.len(),
                                    m.params.len()
                                ));
                            }
                        }
                    }
                }
                let mut members = Vec::new();
                for tm in &tr.methods {
                    let own = methods.iter().find(|m| m.name == tm.name);
                    let function = match (own, &tm.body) {
                        (Some(m), _) => {
                            let f = trait_impl_fn_name(type_name, trait_name, &m.name);
                            if let Some(body) = &m.body {
                                lifted.push((f.clone(), lift(m, body, meta)));
                            }
                            f
                        }
                        (None, Some(_)) => trait_default_fn_name(trait_name, &tm.name),
                        (None, None) => {
                            errors.push(format!(
                                "{}`{what}` is missing `{}`: trait `{trait_name}` declares it \
                                 without a default body",
                                span_prefix(meta),
                                tm.name
                            ));
                            continue;
                        }
                    };
                    members.push(MemberPlan {
                        name: format!("{trait_name}::{}", tm.name),
                        function,
                        has_self: tm.has_self(),
                        meta: own
                            .map(|m| m.meta.clone())
                            .unwrap_or_else(|| tm.meta.clone()),
                    });
                }
                types[ty_idx].trait_impls.push(TraitImplPlan {
                    trait_name: trait_name.clone(),
                    meta: meta.clone(),
                    members,
                });
            }
        }
    }

    // ── `self` outside an impl ──────────────────────────────────────────────
    for fname in &fn_names {
        let func = &program.functions[fname];
        if func.params.iter().any(|(p, _)| p == "self") {
            continue;
        }
        if let Some(meta) = find_self_use(&func.body) {
            let where_ = if fname == "main" {
                "at the top level".to_string()
            } else {
                format!("in `fn {fname}`")
            };
            errors.push(format!(
                "{}`self` used {where_}, outside an `impl` method; `self` is the receiver \
                 parameter of a method (`impl T {{ fn m(self) {{ … }} }}`)",
                span_prefix(&meta)
            ));
        }
    }

    if !errors.is_empty() {
        bail!("{}", errors.join("\n"));
    }

    // A lifted name is `Type::method`; the only other `::` names are the
    // functions of imported modules (`facet::parse`, CRUSH-110). A clash —
    // a struct named like an imported module — is an error, not an overwrite.
    for (name, func) in &lifted {
        if let Some(existing) = program.functions.get(name) {
            let module = existing
                .meta
                .get("module")
                .and_then(Value::as_str)
                .unwrap_or("?");
            bail!(
                "{}method `{name}` has the same name as function `{name}` of imported module                  `{module}`; rename the struct or import the module `as` another name",
                span_prefix(&func.meta)
            );
        }
    }
    for (name, func) in lifted {
        program.functions.insert(name, func);
    }
    Ok(Some(Plan { types, traits }))
}

/// Per-block checks shared by `impl` and `trait`: duplicate names, `self`
/// only as the first parameter, `self` used in a static member.
fn check_methods(what: &str, methods: &[MethodDef], errors: &mut Vec<String>) {
    let mut seen: BTreeMap<&str, &HashMap<String, Value>> = BTreeMap::new();
    for m in methods {
        if let Some(first) = seen.get(m.name.as_str()) {
            errors.push(format!(
                "{}duplicate member `{}` in `{what}` (first defined at {})",
                span_prefix(&m.meta),
                m.name,
                describe_pos(first)
            ));
        } else {
            seen.insert(&m.name, &m.meta);
        }
        if m.params.iter().skip(1).any(|(p, _)| p == "self") {
            errors.push(format!(
                "{}`self` must be the first parameter of `{}` in `{what}`",
                span_prefix(&m.meta),
                m.name
            ));
        } else if !m.has_self()
            && let Some(body) = &m.body
            && let Some(at) = find_self_use(body)
        {
            errors.push(format!(
                "{}`self` used in `{}` in `{what}`, which takes no `self` (a static member); \
                 add `self` as its first parameter to make it a method",
                span_prefix(&at),
                m.name
            ));
        }
    }
}

/// A method as a program function. It belongs to whoever declared the
/// `impl` / `trait` (`owner`, see [`inherit_owner`]), so a method of another
/// package's type is owned by that package.
fn lift(m: &MethodDef, body: &[Statement], owner: &HashMap<String, Value>) -> Function {
    let mut meta = m.meta.clone();
    inherit_owner(owner, &mut meta);
    Function {
        params: m.params.clone(),
        body: body.to_vec(),
        meta,
        is_async: m.is_async,
        annotations: None,
    }
}

/// The CAST meta keys that say who owns a declaration (V3b, #522).
pub const OWNER_META_KEYS: &[&str] = &["module", "package"];

/// The meta key `import_system` puts on a merged `struct` / `impl` / `trait`:
/// its module's merge prefix (`geo`, `geo#2`), the prefix of the module's
/// mangled function names. Read by [`inherit_declaration_owners`].
pub const MODULE_PREFIX_META: &str = "module_prefix";

/// **The rule for synthesised code** (V3b, #522): a function a pass creates
/// from another (a lifted method, a lowered polyglot block and its helpers)
/// belongs to the same package. Copies `from`'s owner meta (`package`, and
/// `module` with it) onto `to`, overriding what `to` had, when `from` is
/// owned by a package other than the root; does nothing for the root package,
/// so a one-package program's meta, and so its CASM, is unchanged. The
/// compiler applies the same rule to what it lifts while compiling (lambdas,
/// nested functions, async bodies).
pub fn inherit_owner(from: &HashMap<String, Value>, to: &mut HashMap<String, Value>) {
    if package_of(from).is_none() {
        return;
    }
    for key in OWNER_META_KEYS {
        match from.get(*key) {
            Some(v) => to.insert(key.to_string(), v.clone()),
            None => to.remove(*key),
        };
    }
}

/// Give each merged `struct` / `impl` / `trait` the package of its module's
/// functions (V3b, #522), so the declaration and the functions next to it
/// can't disagree. The resolver records a package on functions (V1, #519);
/// whatever it records there, a type of that module gets too. Functions of
/// one module in two packages, or a declaration whose own package differs
/// from its module's, is an internal compiler error, never a silent root.
///
/// A module that declares types and no function has nothing to derive from:
/// its declarations need the resolver's own `meta["package"]` stamp.
pub fn inherit_declaration_owners(program: &mut Program) -> Result<()> {
    // Module prefix → the packages its functions carry (raw, as recorded).
    let mut prefixes: BTreeSet<String> = BTreeSet::new();
    for f in program.functions.values() {
        for stmt in &f.body {
            if let Statement::StructDef { meta, .. }
            | Statement::ImplBlock { meta, .. }
            | Statement::TraitDef { meta, .. } = stmt
                && let Some(p) = meta.get(MODULE_PREFIX_META).and_then(Value::as_str)
            {
                prefixes.insert(p.to_string());
            }
        }
    }
    if prefixes.is_empty() {
        return Ok(());
    }
    let mut packages: HashMap<String, BTreeSet<String>> = HashMap::new();
    for (name, f) in &program.functions {
        let Some((prefix, _)) = name.rsplit_once("::") else {
            continue;
        };
        if !prefixes.contains(prefix) {
            continue;
        }
        let id = f
            .meta
            .get("package")
            .and_then(Value::as_str)
            .unwrap_or(crush_cosm::ROOT_PACKAGE);
        packages
            .entry(prefix.to_string())
            .or_default()
            .insert(id.to_string());
    }
    for (prefix, ids) in &packages {
        if ids.len() > 1 {
            bail!(
                "internal compiler error: the functions of module `{prefix}` are in different \
                 packages ({}); a module is in one package (V3b, #522)",
                ids.iter().cloned().collect::<Vec<_>>().join(", ")
            );
        }
    }
    for f in program.functions.values_mut() {
        for stmt in &mut f.body {
            let (Statement::StructDef { meta, .. }
            | Statement::ImplBlock { meta, .. }
            | Statement::TraitDef { meta, .. }) = stmt
            else {
                continue;
            };
            let Some(prefix) = meta.get(MODULE_PREFIX_META).and_then(Value::as_str) else {
                continue;
            };
            let Some(module_id) = packages.get(prefix).and_then(|ids| ids.iter().next()) else {
                continue;
            };
            match meta.get("package").and_then(Value::as_str) {
                Some(own) if own != module_id => bail!(
                    "internal compiler error: a declaration of module `{prefix}` is in package \
                     `{own}` but the module's functions are in `{module_id}` (V3b, #522)"
                ),
                Some(_) => {}
                None if module_id == crush_cosm::ROOT_PACKAGE => {}
                None => {
                    meta.insert("package".to_string(), Value::String(module_id.clone()));
                }
            }
        }
    }
    Ok(())
}

/// The package id a CAST node's `meta["package"]` names, as it goes into an
/// artifact; `None` for the root package (absent, or `"root"`). Only valid
/// after [`normalize_package_ids`].
pub(crate) fn package_of(meta: &HashMap<String, Value>) -> Option<String> {
    match meta.get("package")?.as_str()? {
        crush_cosm::ROOT_PACKAGE => None,
        p => Some(p.to_string()),
    }
}

/// A library directory's label: its last real path component (split on `/`
/// and `\\` alike, so the id doesn't depend on the host), skipping `.`, `..`
/// and empty components; `dir` when there is none (`-L .`). Read from the
/// spelling only, never the filesystem or the working directory, so the same
/// source compiles to the same owners in any checkout. A spelling that says
/// more (`-L ../geo` instead of `-L ..`) is the resolver's to pass (V1, #519);
/// `#N` separates directories whose labels clash.
fn dir_label(dir: &str) -> String {
    dir.rsplit(['/', '\\'])
        .find(|c| !c.is_empty() && *c != "." && *c != "..")
        .unwrap_or("dir")
        .to_string()
}

/// Rewrite every `meta["package"]` in `program` to the id that goes into a
/// shipped artifact (V3b, #522).
///
/// **This is the seam V1 (#519) plugs into.** Main records no package yet; the
/// V1 resolver stamps `meta["package"]` on each merged function (and must
/// stamp it on each merged `struct` / `impl` / `trait` too, beside
/// `meta["module"]` in `import_system`) as `root`, `pkg:<name>` or
/// `dir:<directory>`. A directory is whatever the user passed (`-L`,
/// `CRUSH_PATH`), often absolute, and must not reach an artifact: that would
/// make builds depend on where the source sits and leak local directory
/// names. So:
///
/// - `root` is removed: the root package is the absence of an owner, so a
///   one-package program's CAST meta (and so its CASM) is the same with the
///   resolver's stamp as without it;
/// - `pkg:<name>` stays as it is: a crush-pkg package name is already stable;
/// - `dir:<directory>` becomes `dir:<its last path component>` (`dir:lib`).
///   Two different directories with the same last component in one program
///   are told apart by `#2`, `#3`, … in the order of their full spelling, so
///   distinct packages never share an id.
///
/// Anything else is a compile error, so an unexpected spelling never reaches
/// the artifact unchecked. Idempotent: a rewritten id maps to itself.
pub fn normalize_package_ids(program: &mut Program) -> Result<()> {
    fn metas(program: &mut Program) -> Vec<&mut HashMap<String, Value>> {
        let mut out = Vec::new();
        for f in program.functions.values_mut() {
            out.push(&mut f.meta);
            for stmt in &mut f.body {
                match stmt {
                    Statement::StructDef { meta, .. } => out.push(meta),
                    Statement::ImplBlock { meta, methods, .. }
                    | Statement::TraitDef { meta, methods, .. } => {
                        out.push(meta);
                        // A method's own meta (CAST JSON may carry one) is
                        // normalized too, so nothing reaches `lift` raw.
                        out.extend(methods.iter_mut().map(|m| &mut m.meta));
                    }
                    _ => {}
                }
            }
        }
        out
    }
    let mut dirs: BTreeSet<String> = BTreeSet::new();
    for meta in metas(program) {
        let Some(v) = meta.get("package") else {
            continue;
        };
        let Some(id) = v.as_str() else {
            bail!("a package id must be a string, got {v}");
        };
        if id == crush_cosm::ROOT_PACKAGE {
            // The root package is the absence of an owner. The resolver
            // stamps it on every merged function (V1, #519); removed here,
            // before anything copies meta (the compiler copies a function's
            // meta into each of its CASM instructions), so a one-package
            // program compiles exactly as it does without the stamp.
            meta.remove("package");
            continue;
        }
        if let Some(dir) = id.strip_prefix("dir:") {
            // A directory as given (often absolute): labelled below.
            dirs.insert(dir.to_string());
        } else if let Some(problem) = crush_cosm::owner_id_problem(crush_cosm::OwnerId::Package, id)
        {
            bail!("{problem} (a package id in CAST is `root`, `pkg:<name>` or `dir:<directory>`)");
        }
    }
    if dirs.is_empty() {
        return Ok(());
    }
    // Directory → its label: its last path component (split on `/` and `\\`
    // alike, so the id doesn't depend on the host), numbered from `#2` when
    // that id is already taken.
    let mut assigned: BTreeSet<String> = BTreeSet::new();
    let mut label: HashMap<String, String> = HashMap::new();
    for dir in &dirs {
        let last = dir_label(dir);
        let mut id = format!("dir:{last}");
        let mut n = 2;
        while assigned.contains(&id) {
            id = format!("dir:{last}#{n}");
            n += 1;
        }
        assigned.insert(id.clone());
        label.insert(dir.clone(), id);
    }
    for meta in metas(program) {
        if let Some(dir) = meta
            .get("package")
            .and_then(Value::as_str)
            .and_then(|id| id.strip_prefix("dir:"))
            && let Some(id) = label.get(dir)
        {
            meta.insert("package".to_string(), Value::String(id.clone()));
        }
    }
    Ok(())
}

fn describe_pos(meta: &HashMap<String, Value>) -> String {
    match span_of(meta) {
        Some((line, col)) => format!("line {line}, col {col}"),
        None => "an earlier declaration".to_string(),
    }
}

fn list_or_none<'a>(items: impl Iterator<Item = &'a str>) -> String {
    let v: Vec<String> = items.map(|s| format!("`{s}`")).collect();
    if v.is_empty() {
        "nothing".to_string()
    } else {
        v.join(", ")
    }
}

/// The first use of `self` in `body` that `body` itself doesn't bind, with
/// the meta of the node that uses it. Walks the CAST's JSON form, so every
/// statement and expression shape is covered, including ones added later.
/// A body that binds `self` anywhere (a `let self`, a lambda or nested `fn`
/// parameter) is not checked — that `self` is an ordinary variable.
pub(crate) fn find_self_use(body: &[Statement]) -> Option<HashMap<String, Value>> {
    let json = serde_json::to_value(body).ok()?;
    if binds_self(&json) {
        return None;
    }
    let mut found = None;
    visit(&json, &mut |obj| {
        if found.is_some() {
            return;
        }
        let ty = obj.get("type").and_then(Value::as_str).unwrap_or("");
        let name = obj.get("name").and_then(Value::as_str).unwrap_or("");
        let uses = match ty {
            "Var" => name == "self",
            // `self.m(…)` parses to the dotted call `self.m`.
            "CapabilityCall" => name.starts_with("self."),
            "Assign" => obj.get("target").and_then(Value::as_str) == Some("self"),
            _ => false,
        };
        if uses {
            let meta = obj
                .get("meta")
                .and_then(|m| serde_json::from_value(m.clone()).ok())
                .unwrap_or_default();
            found = Some(meta);
        }
    });
    found
}

fn binds_self(json: &Value) -> bool {
    let mut binds = false;
    visit(json, &mut |obj| {
        let ty = obj.get("type").and_then(Value::as_str).unwrap_or("");
        match ty {
            "VarDecl" => binds |= obj.get("name").and_then(Value::as_str) == Some("self"),
            "Lambda" | "FunctionDef" => {
                binds |= obj
                    .get("params")
                    .and_then(Value::as_array)
                    .is_some_and(|ps| {
                        ps.iter()
                            .any(|p| p.get(0).and_then(Value::as_str) == Some("self"))
                    })
            }
            _ => {}
        }
    });
    binds
}

fn visit(v: &Value, f: &mut impl FnMut(&serde_json::Map<String, Value>)) {
    match v {
        Value::Object(obj) => {
            f(obj);
            for (k, child) in obj {
                if k != "meta" {
                    visit(child, f);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                visit(item, f);
            }
        }
        _ => {}
    }
}

/// Names of every method that takes `self` in a COSM module — the calls the
/// compiler lowers to `call_member` instead of a global capability
/// (`p.norm2()` would otherwise become `cap_call "norm2"`). Includes trait methods by their bare
/// name, since `p.area()` names them that way.
pub fn receiver_methods(module: &crush_cosm::CosmModule) -> BTreeSet<String> {
    cosm_kind::receiver_method_names(module)
}

/// Strategies no engine runs yet. The scheduler VM and the portable VM run
/// Kinds (COSM K1.4, CRUSH-zorro-43); what even they refuse — the shared rule
/// at `casm::CosmSupport::Kinds`, `casm::kind_unsupported_strategies_on` — is
/// a compile error here, with the struct's position, rather than a load-time
/// refusal on every engine. (FastVM, the JIT and AOT refuse more: they apply
/// the same rule at `casm::CosmSupport::Maps` when they load the program.)
pub fn refuse_unimplemented_strategies(module: &crush_cosm::CosmModule) -> Result<()> {
    let mut errors = Vec::new();
    for k in &module.kinds {
        let chosen = casm::kind_unsupported_strategies_on(k, casm::CosmSupport::Kinds);
        if chosen.is_empty() {
            continue;
        }
        let at = k
            .span
            .as_ref()
            .map(|s| format!("line {}, col {}: ", s.line, s.col))
            .unwrap_or_default();
        errors.push(format!(
            "{at}struct `{}` declares `@kind({})`, which no engine runs yet (the VMs implement \
             COSM K1.4, CRUSH-zorro-43). `crush_frontend::lower_to_cosm` produces its COSM",
            k.name,
            chosen.join(", ")
        ));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        bail!("{}", errors.join("\n"))
    }
}
