//! COSM K3, JS slice (CRUSH-277 S2): `prototype_chain` kinds run on the
//! scheduler VM and the portable VM. The fixtures are the KindDecls the
//! boa JS lowering emits (CRUSH-277 S1): open-dict, reference semantics,
//! dynamic-`this` receiver, `js_new` construction, identity equality —
//! and the programs are hand-written CVM1 in the shape the compiler will
//! emit for them (methods take `this` as their first parameter).

use crate::assembler::assemble;
use crate::bytecode::Program;
use crate::vm::{Quotas, VmError, run};
use crush_cosm::{
    Construct, CosmModule, Equality, KindDecl, KindRef, MemberDecl, Receiver, Resolution,
    Semantics, Storage,
};

fn program(src: &str, module: CosmModule) -> Program {
    let mut p = assemble(src, Some(&["io.print"]), None).unwrap_or_else(|e| panic!("assembly: {e}"));
    p.cosm = Some(module);
    p
}

fn run_both(src: &str, module: CosmModule, want: &str) {
    let p = program(src, module);
    let on_scheduler = run(&p, &Quotas::default()).map(|r| r.output);
    assert_eq!(on_scheduler.map_err(|e| e.to_string()).as_deref(), Ok(want), "scheduler VM");
    let on_portable = crate::portable_vm::PortableVm::new(p).run().map(|r| r.output);
    assert_eq!(on_portable.map_err(|e| e.to_string()).as_deref(), Ok(want), "portable VM");
}

fn fails_on_both(src: &str, module: CosmModule, want: &str) {
    let p = program(src, module);
    let got_scheduler = run(&p, &Quotas::default()).map(|r| r.output);
    let got_portable = crate::portable_vm::PortableVm::new(p).run().map(|r| r.output);
    for (vm, got) in [("scheduler VM", got_scheduler), ("portable VM", got_portable)] {
        let e = got.expect_err(vm).to_string();
        assert!(e.contains(want), "{vm}: want {want:?} in {e:?}");
    }
}

/// A KindDecl as the JS lowering emits it.
fn js_kind(name: &str, f: impl FnOnce(&mut KindDecl)) -> KindDecl {
    let mut k = KindDecl::crush(name);
    k.home_lang = "javascript".to_string();
    k.storage = Storage::OpenDict;
    k.semantics = Semantics::Reference;
    k.resolution = Resolution::PrototypeChain;
    k.receiver = Receiver::DynamicThis;
    k.equality = Equality::Identity;
    k.construct = Construct::JsNew { ctor_fn: format!("{name}::constructor").into() };
    f(&mut k);
    k
}

fn module(kinds: Vec<KindDecl>) -> CosmModule {
    CosmModule { kinds, interfaces: Vec::new() }
}

/// A class hierarchy: `Animal` (speak → "animal"), `Dog extends Animal`
/// (overrides speak → "dog"), `Puppy extends Dog` (inherits the override).
fn animals() -> CosmModule {
    module(vec![
        js_kind("Animal", |k| {
            k.members.push(MemberDecl::method("speak", "Animal::speak"));
        }),
        js_kind("Dog", |k| {
            k.parents = vec![KindRef::new("Animal")];
            k.members.push(MemberDecl::method("speak", "Dog::speak"));
        }),
        js_kind("Puppy", |k| {
            k.parents = vec![KindRef::new("Dog")];
        }),
    ])
}

const ANIMAL_FUNCS: &str = r#"
.func Animal::constructor 1
    STORE 0
    PUSH_NULL
    RET
.func Dog::constructor 1
    STORE 0
    PUSH_NULL
    RET
.func Puppy::constructor 1
    STORE 0
    PUSH_NULL
    RET
.func Animal::speak 1
    STORE 0
    PUSH_STR "animal"
    RET
.func Dog::speak 1
    STORE 0
    PUSH_STR "dog"
    RET
"#;

#[test]
fn js_new_construction_binds_this_and_methods_read_it() {
    let m = module(vec![js_kind("Counter", |k| {
        k.members.push(MemberDecl::method("get", "Counter::get"));
        k.members.push(MemberDecl::method("inc", "Counter::inc"));
    })]);
    run_both(
        &format!(
            r#"
.func main
    PUSH 41
    CONSTRUCT "Counter" 1
    POP
    STORE 0
    LOAD 0
    CALL_MEMBER "inc" 0
    POP
    LOAD 0
    CALL_MEMBER "get" 0
    CAP_CALL "io.print" 1
    HALT
.func Counter::constructor 2
    STORE 0
    STORE 1
    LOAD 0
    LOAD 1
    SET_MEMBER "x"
    POP
    PUSH_NULL
    RET
.func Counter::get 1
    STORE 0
    LOAD 0
    GET_MEMBER "x"
    RET
.func Counter::inc 1
    STORE 0
    LOAD 0
    LOAD 0
    GET_MEMBER "x"
    PUSH 1
    ADD
    SET_MEMBER "x"
    POP
    PUSH_NULL
    RET
"#
        ),
        m,
        "42\n",
    );
}

#[test]
fn methods_resolve_and_override_along_the_chain() {
    run_both(
        &format!(
            r#"
.func main
    CONSTRUCT "Dog" 0
    POP
    STORE 0
    LOAD 0
    CALL_MEMBER "speak" 0
    CAP_CALL "io.print" 1
    CONSTRUCT "Puppy" 0
    POP
    STORE 1
    LOAD 1
    CALL_MEMBER "speak" 0
    CAP_CALL "io.print" 1
    CONSTRUCT "Animal" 0
    POP
    STORE 2
    LOAD 2
    CALL_MEMBER "speak" 0
    CAP_CALL "io.print" 1
    HALT
{ANIMAL_FUNCS}"#
        ),
        animals(),
        "dog\ndog\nanimal\n",
    );
}

#[test]
fn instanceof_walks_the_chain() {
    run_both(
        &format!(
            r#"
.func main
    CONSTRUCT "Puppy" 0
    POP
    STORE 0
    LOAD 0
    IS_KIND "Animal"
    CAP_CALL "io.print" 1
    LOAD 0
    IS_KIND "Dog"
    CAP_CALL "io.print" 1
    LOAD 0
    IS_KIND "Puppy"
    CAP_CALL "io.print" 1
    HALT
{ANIMAL_FUNCS}"#
        ),
        animals(),
        "true\ntrue\ntrue\n",
    );
}

#[test]
fn an_instance_field_shadows_a_prototype_member() {
    let m = module(vec![js_kind("Box", |k| {
        k.members.push(MemberDecl::method("v", "Box::v"));
    })]);
    run_both(
        r#"
.func main
    CONSTRUCT "Box" 0
    POP
    STORE 0
    LOAD 0
    PUSH 99
    SET_MEMBER "v"
    POP
    LOAD 0
    GET_MEMBER "v"
    CAP_CALL "io.print" 1
    HALT
.func Box::constructor 1
    STORE 0
    PUSH_NULL
    RET
.func Box::v 1
    STORE 0
    PUSH 7
    RET
"#,
        m,
        "99\n",
    );
}

#[test]
fn a_detached_method_is_the_plain_function() {
    // JS: `const f = dog.speak; f()` — the method value carries no
    // receiver (its `this` is null until called some other way), and a
    // method that never touches `this` still runs.
    run_both(
        &format!(
            r#"
.func main
    CONSTRUCT "Dog" 0
    POP
    STORE 0
    LOAD 0
    GET_MEMBER "speak"
    CALL_VALUE 0
    CAP_CALL "io.print" 1
    HALT
{ANIMAL_FUNCS}"#
        ),
        animals(),
        "dog\n",
    );
}

#[test]
fn a_detached_method_that_reads_this_fails() {
    // The other half: `this.x` with `this` unbound is a loud error on a
    // null receiver, as `undefined.x` is in JS.
    let m = module(vec![js_kind("Counter", |k| {
        k.members.push(MemberDecl::method("get", "Counter::get"));
    })]);
    fails_on_both(
        r#"
.func main
    PUSH 1
    CONSTRUCT "Counter" 1
    POP
    STORE 0
    LOAD 0
    GET_MEMBER "get"
    CALL_VALUE 0
    CAP_CALL "io.print" 1
    HALT
.func Counter::constructor 2
    STORE 0
    STORE 1
    LOAD 0
    LOAD 1
    SET_MEMBER "x"
    POP
    PUSH_NULL
    RET
.func Counter::get 1
    STORE 0
    LOAD 0
    GET_MEMBER "x"
    RET
"#,
        m,
        "record",
    );
}

#[test]
fn a_prototype_cycle_is_refused_at_load() {
    let m = module(vec![
        js_kind("A", |k| k.parents = vec![KindRef::new("B")]),
        js_kind("B", |k| k.parents = vec![KindRef::new("A")]),
    ]);
    fails_on_both(
        ".func main\n    PUSH_STR \"never\"\n    CAP_CALL \"io.print\" 1\n    HALT\n",
        m,
        "inheritance cycle: A -> B -> A",
    );
}

#[test]
fn load_kind_then_a_static_method_call_on_the_kind() {
    // CRUSH-277 D8: `load_kind` pushes the Kind as a value; calling a
    // static member on it binds the Kind itself as the receiver
    // (arguments last-first, receiver on top).
    let m = module(vec![js_kind("Util", |k| {
        k.members.push(MemberDecl::new(
            "twice",
            crush_cosm::Member::Static {
                value: crush_cosm::StaticValue::Fn("Util::static::twice".into()),
            },
        ));
    })]);
    run_both(
        r#"
.func main
    PUSH 21
    LOAD_KIND "Util"
    CALL_MEMBER "twice" 1
    CAP_CALL "io.print" 1
    HALT
.func Util::constructor 1
    STORE 0
    PUSH_NULL
    RET
.func Util::static::twice 1
    STORE 0
    LOAD 0
    PUSH 2
    MUL
    RET
"#,
        m,
        "42\n",
    );
}

#[test]
fn load_kind_of_an_undeclared_kind_is_a_load_error() {
    fails_on_both(
        r#"
.func main
    LOAD_KIND "Nope"
    CAP_CALL "io.print" 1
    HALT
"#,
        module(vec![]),
        "Nope",
    );
}

/// `Meter` (CRUSH-277 S4): `x` plus the accessor `doubled` over it, a
/// get-only `tag` and a set-only `secret`.
fn meter() -> CosmModule {
    module(vec![js_kind("Meter", |k| {
        k.members.push(MemberDecl::accessor(
            "doubled",
            Some("Meter::get::doubled".into()),
            Some("Meter::set::doubled".into()),
        ));
        k.members.push(MemberDecl::accessor("tag", Some("Meter::get::tag".into()), None));
        k.members.push(MemberDecl::accessor("secret", None, Some("Meter::set::secret".into())));
    })])
}

const METER_FUNCS: &str = r#"
.func Meter::constructor 2
    STORE 0
    STORE 1
    LOAD 0
    LOAD 1
    SET_MEMBER "x"
    POP
    PUSH_NULL
    RET
.func Meter::get::doubled 1
    STORE 0
    LOAD 0
    GET_MEMBER "x"
    PUSH 2
    MUL
    RET
.func Meter::set::doubled 2
    STORE 0
    STORE 1
    LOAD 0
    LOAD 1
    PUSH 2
    DIV
    SET_MEMBER "x"
    POP
    PUSH_NULL
    RET
.func Meter::get::tag 1
    STORE 0
    PUSH 7
    RET
.func Meter::set::secret 2
    STORE 0
    STORE 1
    LOAD 0
    LOAD 1
    SET_MEMBER "x"
    POP
    PUSH_NULL
    RET
"#;

#[test]
fn accessors_get_set_and_the_odd_halves() {
    run_both(
        &format!(
            r#"
.func main
    PUSH 21
    CONSTRUCT "Meter" 1
    POP
    STORE 0
    LOAD 0
    GET_MEMBER "doubled"
    CAP_CALL "io.print" 1
    LOAD 0
    PUSH 100
    SET_MEMBER "doubled"
    POP
    LOAD 0
    GET_MEMBER "doubled"
    CAP_CALL "io.print" 1
    LOAD 0
    GET_MEMBER "tag"
    CAP_CALL "io.print" 1
    LOAD 0
    PUSH 5
    SET_MEMBER "secret"
    POP
    LOAD 0
    GET_MEMBER "x"
    CAP_CALL "io.print" 1
    LOAD 0
    GET_MEMBER "secret"
    CAP_CALL "io.print" 1
    HALT
{METER_FUNCS}"#
        ),
        meter(),
        "42\n100\n7\n5\nnull\n",
    );
}

#[test]
fn accessors_are_inherited_along_the_chain() {
    // A subclass with no members of its own reads and writes through the
    // parent's getter/setter pair, against its own instance state.
    let mut m = meter();
    m.kinds.push(js_kind("SubMeter", |k| {
        k.parents = vec![KindRef::new("Meter")];
    }));
    run_both(
        &format!(
            r#"
.func main
    PUSH 21
    CONSTRUCT "SubMeter" 1
    POP
    STORE 0
    LOAD 0
    GET_MEMBER "doubled"
    CAP_CALL "io.print" 1
    LOAD 0
    PUSH 100
    SET_MEMBER "doubled"
    POP
    LOAD 0
    GET_MEMBER "doubled"
    CAP_CALL "io.print" 1
    HALT
.func SubMeter::constructor 2
    STORE 0
    STORE 1
    LOAD 0
    LOAD 1
    SET_MEMBER "x"
    POP
    PUSH_NULL
    RET
{METER_FUNCS}"#
        ),
        m,
        "42\n100\n",
    );
}

#[test]
fn writing_a_read_only_accessor_is_a_loud_error() {
    fails_on_both(
        &format!(
            r#"
.func main
    PUSH 21
    CONSTRUCT "Meter" 1
    POP
    STORE 0
    LOAD 0
    PUSH 1
    SET_MEMBER "tag"
    POP
    HALT
{METER_FUNCS}"#
        ),
        meter(),
        "`Meter.tag` has no setter",
    );
}
