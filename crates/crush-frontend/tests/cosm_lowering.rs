//! COSM K1.2 (CRUSH-zorro-41): Crush `struct` / `impl` / `trait` / `@kind(…)`
//! lowered to COSM — snapshots of the produced module, the lifted functions,
//! and every compile-time error.

use crush_cosm::CosmModule;
use serde_json::json;

/// The design doc's example (`docs/design/object-model.md`, *Crush's own KindDecl*).
const POINT: &str = "\
struct Point { x: int, y: int }
impl Point {
    fn new(x, y) { return Point { x: x, y: y }; }
    fn norm2(self) { return self.x * self.x + self.y * self.y; }
}
trait Shape { fn area(self); fn describe(self) { return \"a shape\"; } }
impl Shape for Point { fn area(self) { return 0; } }
";

fn lower(src: &str) -> CosmModule {
    crush_frontend::lower_to_cosm(src)
        .unwrap_or_else(|e| panic!("lowering failed: {e:#}\n{src}"))
        .expect("declares types")
}

fn lower_err(src: &str) -> String {
    match crush_frontend::lower_to_cosm(src) {
        Ok(m) => panic!("expected a lowering error, got {m:#?}\n{src}"),
        Err(e) => format!("{e:#}"),
    }
}

fn compile_err(src: &str) -> String {
    match crush_frontend::compile_crush_source(src) {
        Ok(_) => panic!("expected a compile error for:\n{src}"),
        Err(e) => format!("{e:#}"),
    }
}

fn assert_has(err: &str, needles: &[&str]) {
    for n in needles {
        assert!(err.contains(n), "missing {n:?} in:\n{err}");
    }
}

#[test]
fn point_with_impl_and_trait_snapshot() {
    let module = lower(POINT);
    assert!(crush_cosm::validate(&module).is_empty());
    assert_eq!(
        serde_json::to_value(&module).unwrap(),
        json!({
            "kinds": [{
                "name": "Point",
                "home_lang": "crush",
                "span": { "line": 1, "col": 1 },
                "storage": { "sealed": { "fields": [
                    { "name": "x", "type": "Int" },
                    { "name": "y", "type": "Int" }
                ] } },
                "semantics": "reference",
                "resolution": "none",
                "receiver": "explicit_ref",
                "construct": "literal",
                "members": [
                    { "name": "new", "span": { "line": 3, "col": 5 }, "kind": "static", "value": { "fn": "Point::new" } },
                    { "name": "norm2", "span": { "line": 4, "col": 5 }, "kind": "method", "fn": "Point::norm2" },
                    { "name": "Shape::area", "span": { "line": 7, "col": 24 }, "kind": "method", "fn": "Point::Shape::area" },
                    { "name": "Shape::describe", "span": { "line": 6, "col": 30 }, "kind": "method", "fn": "Shape::describe" }
                ],
                "conforms": [{
                    "interface": "Shape",
                    "check": "nominal",
                    "members": [
                        { "name": "area", "span": { "line": 7, "col": 24 }, "kind": "method", "fn": "Point::Shape::area" }
                    ],
                    "span": { "line": 7, "col": 1 }
                }],
                "equality": "structural"
            }],
            "interfaces": [{
                "name": "Shape",
                "home_lang": "crush",
                "span": { "line": 6, "col": 1 },
                "members": [
                    { "name": "area", "span": { "line": 6, "col": 15 }, "receiver": "explicit_ref", "returns": "Any" },
                    { "name": "describe", "span": { "line": 6, "col": 30 }, "receiver": "explicit_ref", "returns": "Any",
                      "default": "Shape::describe" }
                ],
                "satisfaction": "nominal"
            }]
        })
    );
    // The JSON reads back as the same module.
    assert_eq!(CosmModule::from_json(&module.to_json()).unwrap(), module);
}

#[test]
fn value_copy_struct_snapshot() {
    let module = lower("@kind(semantics: value(copy))\nstruct Vec2 { x: float, y: float }\n");
    assert_eq!(
        serde_json::to_value(&module).unwrap(),
        json!({
            "kinds": [{
                "name": "Vec2",
                "home_lang": "crush",
                "span": { "line": 2, "col": 1 },
                "storage": { "sealed": { "fields": [
                    { "name": "x", "type": "Float" },
                    { "name": "y", "type": "Float" }
                ] } },
                "semantics": { "value": "copy" },
                "resolution": "none",
                "receiver": "explicit_ref",
                "construct": "literal",
                "equality": "structural"
            }],
            "interfaces": []
        })
    );
}

#[test]
fn python_like_kind_with_c3_parents() {
    let src = "\
struct Base { }
struct Mixin { }
@kind(storage: open_dict, resolution: c3, parents: [Base, Mixin], receiver: bound_self, equality: identity)
struct Widget { name }
impl Widget { fn label(self) { return self.name } }
";
    let module = lower(src);
    let w = module.kind("Widget").unwrap();
    let v = serde_json::to_value(w).unwrap();
    assert_eq!(v["storage"], json!("open_dict"));
    assert_eq!(v["parents"], json!(["Base", "Mixin"]));
    assert_eq!(v["resolution"], json!("c3"));
    assert_eq!(v["receiver"], json!("bound_self"));
    assert_eq!(v["equality"], json!("identity"));
    // An open_dict struct keeps its declared fields as `field` members.
    assert_eq!(v["members"][0], json!({ "name": "name", "kind": "field" }));
    assert_eq!(v["members"][1]["fn"], json!("Widget::label"));
    assert!(crush_cosm::validate(&module).is_empty());
    // Every other strategy is selectable too.
    let module = lower(
        "struct P { x }\nimpl P { fn make(x) { return P { x: x } } fn init(self) { return 0 } }\n\
         @kind(storage: open_dict_with_slots, construct: py_new_init(make, init), resolution: single, parents: [P])\n\
         struct Q { y }\n\
         @kind(construct: ctor(build), semantics: value(clone_explicit), receiver: explicit_ref_mut)\n\
         struct R { z }\nfn build(z) { return R { z: z } }\n",
    );
    let q = serde_json::to_value(module.kind("Q").unwrap()).unwrap();
    assert_eq!(q["storage"], json!({ "open_dict_with_slots": { "slots": [{ "name": "y", "type": "Any" }] } }));
    assert_eq!(q["construct"], json!({ "py_new_init": { "new_fn": "make", "init_fn": "init" } }));
    let r = serde_json::to_value(module.kind("R").unwrap()).unwrap();
    assert_eq!(r["construct"], json!({ "ctor": { "fn": "build" } }));
    assert_eq!(r["semantics"], json!({ "value": "clone_explicit" }));
}

#[test]
fn construct_functions_resolve_to_the_structs_own_members() {
    let module = lower("@kind(construct: ctor(new))\nstruct P { x }\nimpl P { fn new(x) { return P { x: x } } }\n");
    assert_eq!(
        serde_json::to_value(&module.kinds[0].construct).unwrap(),
        json!({ "ctor": { "fn": "P::new" } })
    );
}

#[test]
fn impls_merge_across_the_file_and_methods_are_lifted() {
    // Two inherent impls, one before the struct; a trait default overridden.
    let src = "\
impl P { fn a(self) { return 1 } }
struct P { x }
trait T { fn d(self) { return \"default\" } }
impl T for P { fn d(self) { return \"own\" } }
impl P { fn b(self, k) { return self.x + k } }
";
    let mut program = crush_frontend::parse_source(src).unwrap();
    let module = crush_frontend::cosm_lower::lower(&mut program).unwrap().unwrap();
    let names: Vec<&str> = module.kinds[0].members.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["a", "b", "T::d"]);
    let conf = &module.kinds[0].conforms[0];
    assert_eq!(serde_json::to_value(&conf.members[0]).unwrap()["fn"], json!("P::T::d"));
    // Lifted functions, `self` first; impl/trait nodes are gone from the CAST.
    let mut fns: Vec<&String> = program.functions.keys().collect();
    fns.sort();
    assert_eq!(fns, ["P::T::d", "P::a", "P::b", "T::d", "main"]);
    let b = &program.functions["P::b"];
    assert_eq!(b.params.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["self", "k"]);
    let main = serde_json::to_string(&program.functions["main"].body).unwrap();
    assert!(!main.contains("ImplBlock") && !main.contains("TraitDef"), "{main}");
    assert!(main.contains("StructDef"), "struct declarations stay: {main}");
}

#[test]
fn compiler_exposes_the_module() {
    let (casm, cosm) = crush_frontend::compile_crush_source_with_cosm(POINT).unwrap();
    assert_eq!(cosm.as_ref(), Some(&lower(POINT)));
    for f in ["Point::new", "Point::norm2", "Point::Shape::area", "Shape::describe"] {
        assert!(casm.functions.contains_key(f), "missing lifted fn {f}");
    }
    let mut c = crush_frontend::compiler::Compiler::new();
    c.compile(crush_frontend::parse_source(POINT).unwrap()).unwrap();
    assert_eq!(c.cosm_module(), cosm.as_ref());
}

#[test]
fn programs_without_declarations_have_no_module() {
    let (_, cosm) = crush_frontend::compile_crush_source_with_cosm("let x = 1\nprint(x)\n").unwrap();
    assert!(cosm.is_none());
    // A plain struct lowers to a Kind with Crush's defaults and nothing else.
    let module = lower("struct P { x: int }\n");
    let v = serde_json::to_value(&module.kinds[0]).unwrap();
    assert_eq!(v.get("members"), None);
    assert_eq!(v["semantics"], json!("reference"));
}

// ── compile-time errors ─────────────────────────────────────────────────────

#[test]
fn self_outside_an_impl() {
    assert_has(
        &lower_err("struct P { x }\nprint(self.x)\n"),
        &["line 2", "`self` used at the top level, outside an `impl` method"],
    );
    assert_has(
        &lower_err("struct P { x }\nfn f(p) { return self }\n"),
        &["line 2", "`self` used in `fn f`, outside an `impl` method"],
    );
    assert_has(&lower_err("struct P { x }\nfn f() { return self.go(1) }\n"), &["`self` used in `fn f`"]);
    // A static member has no receiver either.
    assert_has(
        &lower_err("struct P { x }\nimpl P { fn new(x) { return self.x } }\n"),
        &["line 2", "`self` used in `new` in `impl P`, which takes no `self` (a static member)"],
    );
    // `self` only as the first parameter.
    assert_has(
        &lower_err("struct P { x }\nimpl P { fn m(a, self) { return a } }\n"),
        &["`self` must be the first parameter of `m` in `impl P`"],
    );
    // Reported by the normal compile path too.
    assert_has(&compile_err("struct P { x }\nfn f() { return self }\n"), &["outside an `impl` method"]);
    // A variable that happens to be named `self` is fine.
    lower("struct P { x }\nlet self = 1\nprint(self)\n");
}

#[test]
fn missing_trait_method_without_default() {
    let err = lower_err(
        "struct P { x }\ntrait Shape { fn area(self); fn name(self); fn d(self) { return 1 } }\nimpl Shape for P { fn name(self) { return \"p\" } }\n",
    );
    assert_has(
        &err,
        &["line 3", "`impl Shape for P` is missing `area`: trait `Shape` declares it without a default body"],
    );
    assert!(!err.contains("missing `d`"), "a default needs no impl: {err}");
}

#[test]
fn duplicate_impl_members() {
    // In one block…
    assert_has(
        &lower_err("struct P { x }\nimpl P { fn m(self) { return 1 }\n fn m(self) { return 2 } }\n"),
        &["line 3", "duplicate member `m` in `impl P` (first defined at line 2"],
    );
    // …across blocks…
    assert_has(
        &lower_err("struct P { x }\nimpl P { fn m(self) { return 1 } }\nimpl P { fn m(self) { return 2 } }\n"),
        &["line 3", "duplicate member `m` in `impl P` (first defined at line 2"],
    );
    // …in a trait impl, and a trait implemented twice.
    assert_has(
        &lower_err(
            "struct P { x }\ntrait T { fn m(self); }\nimpl T for P { fn m(self) { return 1 }\n fn m(self) { return 2 } }\n",
        ),
        &["duplicate member `m` in `impl T for P`"],
    );
    assert_has(
        &lower_err(
            "struct P { x }\ntrait T { fn m(self); }\nimpl T for P { fn m(self) { return 1 } }\nimpl T for P { fn m(self) { return 2 } }\n",
        ),
        &["line 4", "`impl T for P` is written twice"],
    );
    // A trait declaring a method twice.
    assert_has(&lower_err("trait T { fn m(self); fn m(self); }\n"), &["duplicate member `m` in `trait T`"]);
    // A method named like a sealed field (from crush_cosm::validate).
    assert_has(
        &lower_err("struct P { x }\nimpl P { fn x(self) { return 1 } }\n"),
        &["line 2", "method `x` shadows the sealed field of the same name", "cosm::field_shadowed_by_member"],
    );
}

#[test]
fn unknown_kind_key_lists_the_valid_keys() {
    assert_has(
        &lower_err("@kind(layout: sealed)\nstruct P { x }\n"),
        &[
            "line 1, col 7",
            "`@kind` on struct `P`: unknown `@kind` key `layout`; valid keys: storage, semantics, parents, resolution, receiver, construct, equality",
        ],
    );
    assert_has(
        &lower_err("@kind(semantics: reference, semantics: value(copy))\nstruct P { x }\n"),
        &["`semantics` is given twice"],
    );
}

#[test]
fn unknown_kind_value_lists_the_valid_values() {
    let cases = [
        ("storage: closed", "unknown storage strategy `closed`; expected one of: sealed, open_dict, open_dict_with_slots"),
        (
            "semantics: copy",
            "unknown semantics strategy `copy`; expected one of: reference, value(copy), value(move), value(clone_explicit)",
        ),
        ("semantics: value(deep)", "unknown semantics strategy `value(deep)`"),
        (
            "resolution: mro",
            "unknown resolution strategy `mro`; expected one of: none, single, c3, prototype_chain, embedding_promotion",
        ),
        (
            "receiver: this",
            "unknown receiver strategy `this`; expected one of: dynamic_this, bound_self, bound_cls, explicit_value, explicit_ref, explicit_ref_mut",
        ),
        ("equality: deep", "unknown equality strategy `deep`; expected one of: identity, structural"),
        ("construct: factory", "unknown construct strategy `factory`; expected one of: literal, ctor, js_new, py_new_init"),
        ("construct: ctor(a, b)", "unknown construct strategy `ctor(a, b)`"),
        (
            "resolution: c3(x)",
            "`resolution` takes one of: none, single, c3, prototype_chain, embedding_promotion; got `c3(x)`",
        ),
        ("parents: value(copy)", "`parents` takes a list of struct names"),
    ];
    for (arg, want) in cases {
        let err = lower_err(&format!("@kind({arg})\nstruct P {{ x }}\n"));
        assert_has(&err, &["line 1, col 7", "`@kind` on struct `P`", want]);
    }
}

#[test]
fn cosm_validation_errors_become_compile_errors() {
    // Unknown parent, an MRO C3 can't linearize, value semantics on open
    // storage, parents under resolution `none` — all from
    // crush_cosm::validate, reported with the struct's position.
    assert_has(
        &lower_err("@kind(resolution: c3, parents: [Missing])\nstruct P { x }\n"),
        &["line 2, col 1", "unknown parent kind `Missing`", "cosm::unknown_parent"],
    );
    let mro = "\
struct O { }
@kind(resolution: c3, parents: [O]) struct X { }
@kind(resolution: c3, parents: [O]) struct Y { }
@kind(resolution: c3, parents: [X, Y]) struct A { }
@kind(resolution: c3, parents: [Y, X]) struct B { }
@kind(resolution: c3, parents: [A, B]) struct Z { }
";
    assert_has(&lower_err(mro), &["line 6", "cosm::inconsistent_mro"]);
    assert_has(
        &lower_err("@kind(semantics: value(copy), storage: open_dict)\nstruct P { }\n"),
        &["`value(copy)` semantics need a fixed layout, but storage is `open_dict`"],
    );
    assert_has(&lower_err("struct B { }\n@kind(parents: [B])\nstruct P { x }\n"), &["cosm::parent_arity"]);
}

#[test]
fn unknown_types_traits_and_members() {
    assert_has(
        &lower_err("impl Ghost { fn m(self) { return 1 } }\n"),
        &["line 1, col 1", "`impl Ghost`: no struct `Ghost` is declared (declare `struct Ghost { … }` in this file)"],
    );
    assert_has(
        &lower_err("struct P { x }\nimpl Ghost for P { fn m(self) { return 1 } }\n"),
        &["`impl Ghost for P`: no trait `Ghost` is declared in this file"],
    );
    assert_has(
        &lower_err("trait T { fn m(self); }\nimpl T { fn m(self) { return 1 } }\n"),
        &["`T` is a trait; write `impl T for <Type>`"],
    );
    assert_has(
        &lower_err(
            "struct P { x }\ntrait T { fn m(self); }\nimpl T for P { fn m(self) { return 1 }\n fn extra(self) { return 2 } }\n",
        ),
        &["line 4", "`extra` is not a method of trait `T` (`impl T for P`); trait `T` declares: `m`"],
    );
    assert_has(
        &lower_err("struct P { x }\ntrait T { fn m(self); }\nimpl T for P { fn m() { return 1 } }\n"),
        &["`m` takes `self` in trait `T` but does not in `impl T for P`"],
    );
    assert_has(
        &lower_err("struct P { x }\ntrait T { fn m(self, a); }\nimpl T for P { fn m(self) { return 1 } }\n"),
        &["`m` takes 2 parameter(s) in trait `T` but 1 in `impl T for P`"],
    );
    assert_has(&lower_err("struct T { x }\ntrait T { fn m(self); }\n"), &["trait `T` has the same name as a struct"]);
}

#[test]
fn every_error_is_reported_at_once() {
    let err = lower_err(
        "struct P { x }\nimpl P { fn m(a, self) { return 1 } }\nimpl Q { fn n(self) { return 1 } }\nprint(self)\n",
    );
    assert_eq!(err.lines().count(), 3, "{err}");
}

// ── loud until K1.4/K1.5 ────────────────────────────────────────────────────

#[test]
fn calling_an_impl_method_through_the_receiver_is_loud() {
    let err = compile_err(&format!("{POINT}let p = Point::new(3, 4)\nprint(p.norm2())\n"));
    assert_has(
        &err,
        &[
            "line 9",
            "`p.norm2(…)`: calling methods defined in `impl` needs COSM K1.4/K1.5 (CRUSH-zorro-43/44)",
            "the global capability `norm2`",
            "`Type::norm2(p, …)`",
        ],
    );
    // A trait method, by its bare name.
    assert_has(&compile_err(&format!("{POINT}let p = Point::new(3, 4)\nprint(p.describe())\n")), &["`p.describe(…)`"]);
    // Inside a method, through `self`.
    assert_has(
        &compile_err("struct P { x }\nimpl P { fn a(self) { return 1 } fn b(self) { return self.a() } }\n"),
        &["`self.a(…)`"],
    );
    // Programs that define no such method keep compiling dotted calls as before.
    crush_frontend::compile_crush_source("struct P { x }\nlet xs = [1]\nxs.push(2)\nprint(len(xs))\n").unwrap();
}

#[test]
fn strategies_the_vms_cannot_run_are_refused_at_compile_time() {
    for (kind, shown) in [
        ("semantics: value(copy)", "semantics: value(copy)"),
        ("equality: identity", "equality: identity"),
        ("construct: ctor(mk)", "construct: ctor"),
    ] {
        let src = format!("fn mk(x) {{ return 1 }}\n@kind({kind})\nstruct V {{ x }}\nlet a = V {{ x: 1 }}\n");
        lower(&src); // the COSM itself is fine…
        assert_has(
            &compile_err(&src),
            &[
                "line 3, col 1",
                &format!("struct `V` declares `@kind({shown})`, which needs COSM K1.4 (CRUSH-zorro-43) to run"),
            ],
        );
    }
    // Inert strategies compile (they only matter to member lookup).
    crush_frontend::compile_crush_source(
        "struct B { }\n@kind(storage: open_dict, resolution: c3, parents: [B], receiver: bound_self)\nstruct W { }\n",
    )
    .unwrap();
}
