//! COSM K1.4 (CRUSH-zorro-43): the member ops on hand-written CVM1 — what the
//! Crush compiler doesn't emit yet (slots, `HAS_MEMBER` / `DEL_MEMBER`,
//! positional construction, field defaults, `str`/`repr` hooks, Python-style
//! construction, `TYPEOF`) — on the scheduler VM and the portable VM alike.

use crate::assembler::{assemble, disassemble};
use crate::bytecode::Program;
use crate::vm::{Quotas, VmError, run};
use crush_cosm::{
    Conformance, Const, Construct, CosmModule, FieldDecl, HookImpl, KindDecl, KindRef, MemberDecl, Receiver,
    Resolution, Satisfaction, Semantics, Storage, ValueMode,
};

fn program(src: &str, module: CosmModule) -> Program {
    let mut p = assemble(src, Some(&["io.print"]), None).unwrap_or_else(|e| panic!("assembly: {e}"));
    p.cosm = Some(module);
    p
}

fn on_scheduler(p: &Program) -> Result<String, VmError> {
    run(p, &Quotas::default()).map(|r| r.output)
}

fn on_portable(p: &Program) -> Result<String, VmError> {
    crate::portable_vm::PortableVm::new(p.clone()).run().map(|r| r.output)
}

/// Both VMs print `want`.
fn run_both(src: &str, module: CosmModule, want: &str) {
    let p = program(src, module);
    assert_eq!(on_scheduler(&p).map_err(|e| e.to_string()).as_deref(), Ok(want), "scheduler VM");
    assert_eq!(on_portable(&p).map_err(|e| e.to_string()).as_deref(), Ok(want), "portable VM");
}

/// Both VMs fail with an error containing `want`.
fn fails_on_both(src: &str, module: CosmModule, want: &str) {
    let p = program(src, module);
    for (vm, got) in [("scheduler VM", on_scheduler(&p)), ("portable VM", on_portable(&p))] {
        let e = got.expect_err(vm).to_string();
        assert!(e.contains(want), "{vm}: want {want:?} in {e:?}");
    }
}

fn kind(name: &str, f: impl FnOnce(&mut KindDecl)) -> KindDecl {
    let mut k = KindDecl::crush(name);
    f(&mut k);
    k
}

fn sealed(fields: &[&str]) -> Storage {
    Storage::Sealed { fields: fields.iter().map(|f| FieldDecl::new(*f, Default::default())).collect() }
}

fn module(kinds: Vec<KindDecl>) -> CosmModule {
    CosmModule { kinds, interfaces: Vec::new() }
}

#[test]
fn has_and_del_member_on_open_storage_and_typeof() {
    let m = module(vec![kind("Obj", |k| k.storage = Storage::OpenDict)]);
    run_both(
        r#"
.func main
    CONSTRUCT "Obj" 0
    STORE 0
    LOAD 0
    PUSH 1
    SET_MEMBER "a"
    POP
    LOAD 0
    HAS_MEMBER "a"
    CAP_CALL "io.print" 1
    LOAD 0
    DEL_MEMBER "a"
    HAS_MEMBER "a"
    CAP_CALL "io.print" 1
    LOAD 0
    TYPEOF
    CAP_CALL "io.print" 1
    PUSH 3
    TYPEOF
    CAP_CALL "io.print" 1
    HALT
"#,
        m,
        "true\nfalse\nObj\nint\n",
    );
}

#[test]
fn a_missing_member_is_an_error_for_get_member_and_del_member() {
    let m = || module(vec![kind("Obj", |k| k.storage = Storage::OpenDict)]);
    fails_on_both(
        ".func main\n    CONSTRUCT \"Obj\" 0\n    GET_MEMBER \"nope\"\n    HALT\n",
        m(),
        "`Obj` has no member `nope`",
    );
    fails_on_both(
        ".func main\n    CONSTRUCT \"Obj\" 0\n    DEL_MEMBER \"nope\"\n    HALT\n",
        m(),
        "`Obj` has no member `nope`",
    );
    // A sealed Kind's declared field can't be deleted.
    fails_on_both(
        ".func main\n    CONSTRUCT \"P\" 0\n    PUSH 1\n    SET_MEMBER \"x\"\n    DEL_MEMBER \"x\"\n    HALT\n",
        module(vec![kind("P", |k| k.storage = sealed(&["x"]))]),
        "`P` is sealed: its field `x` can't be deleted",
    );
}

#[test]
fn slots_defaults_and_positional_construction() {
    let m = module(vec![kind("V", |k| {
        let mut x = FieldDecl::new("x", Default::default());
        x.default = Some(Const::Int(5));
        k.storage = Storage::Sealed { fields: vec![x, FieldDecl::new("y", Default::default())] };
    })]);
    run_both(
        r#"
.func main
    CONSTRUCT "V" 0
    STORE 0
    LOAD 0
    CAP_CALL "io.print" 1
    LOAD 0
    GET_SLOT 0
    CAP_CALL "io.print" 1
    LOAD 0
    PUSH 9
    SET_SLOT 1
    GET_MEMBER "y"
    CAP_CALL "io.print" 1
    PUSH 2
    PUSH 1
    CONSTRUCT "V" 2
    CAP_CALL "io.print" 1
    HALT
"#,
        m,
        // Arguments are pushed last-first; the first goes to slot 0.
        "{x: 5}\n5\n9\n{x: 1, y: 2}\n",
    );
}

#[test]
fn a_literal_kind_takes_no_arguments_or_one_per_field() {
    fails_on_both(
        ".func main\n    PUSH 1\n    CONSTRUCT \"V\" 1\n    HALT\n",
        module(vec![kind("V", |k| k.storage = sealed(&["x", "y"]))]),
        "`V` is built from a literal: give no arguments, or one per field (2: x, y)",
    );
    fails_on_both(
        ".func main\n    CONSTRUCT \"V\" 0\n    GET_SLOT 4\n    HALT\n",
        module(vec![kind("V", |k| k.storage = sealed(&["x"]))]),
        "`V` has 1 slot(s); there is no slot 4",
    );
    fails_on_both(
        ".func main\n    CONSTRUCT \"Nope\" 0\n    HALT\n",
        module(vec![kind("V", |_| {})]),
        "`Nope` is not a Kind of this program",
    );
}

#[test]
fn is_kind_and_copy() {
    let m = module(vec![
        kind("Val", |k| {
            k.storage = sealed(&["x"]);
            k.semantics = Semantics::Value(ValueMode::Copy);
        }),
        kind("Ref", |k| k.storage = sealed(&["x"])),
    ]);
    run_both(
        r#"
.func main
    PUSH 1
    CONSTRUCT "Val" 1
    STORE 0
    LOAD 0
    IS_KIND "Val"
    CAP_CALL "io.print" 1
    LOAD 0
    IS_KIND "Ref"
    CAP_CALL "io.print" 1
    NEW_OBJ
    IS_KIND "Val"
    CAP_CALL "io.print" 1
    LOAD 0
    COPY
    PUSH 2
    SET_MEMBER "x"
    POP
    LOAD 0
    GET_MEMBER "x"
    CAP_CALL "io.print" 1
    PUSH 1
    CONSTRUCT "Ref" 1
    STORE 1
    LOAD 1
    COPY
    PUSH 2
    SET_MEMBER "x"
    POP
    LOAD 1
    GET_MEMBER "x"
    CAP_CALL "io.print" 1
    PUSH 7
    COPY
    CAP_CALL "io.print" 1
    HALT
"#,
        m,
        // COPY copies a value-semantics instance and passes anything else through.
        "true\nfalse\nfalse\n1\n2\n7\n",
    );
}

const SHOW: &str = r#"
.func main
    PUSH 3
    CONSTRUCT "S" 1
    STORE 0
    LOAD 0
    CAP_CALL "io.print" 1
    LOAD 0
    PRINT
    PUSH 4
    CONSTRUCT "R" 1
    LOAD 0
    NEW_ARRAY 2
    CAP_CALL "io.print" 1
    LOAD 0
    GET_MEMBER "x"
    CAP_CALL "io.print" 1
    HALT
.func S::show 1
    STORE 0
    PUSH_STR "S("
    LOAD 0
    GET_MEMBER "x"
    ADD
    PUSH_STR ")"
    ADD
    RET
.func R::repr 1
    STORE 0
    PUSH_STR "R<"
    LOAD 0
    GET_MEMBER "x"
    ADD
    PUSH_STR ">"
    ADD
    RET
"#;

#[test]
fn printing_uses_the_str_hook_then_repr() {
    let m = module(vec![
        kind("S", |k| {
            k.storage = sealed(&["x"]);
            k.hooks.str = Some(HookImpl::Fn("S::show".into()));
            k.hooks.repr = Some(HookImpl::Fn("S::nope".into()));
        }),
        kind("R", |k| {
            k.storage = sealed(&["x"]);
            k.hooks.repr = Some(HookImpl::Fn("R::repr".into()));
        }),
    ]);
    // `str` wins over `repr`; `repr` is the fallback; inside an array too.
    run_both(SHOW, m, "S(3)\nS(3)[R<4>, S(3)]\n3\n");
}

#[test]
fn python_style_construction_runs_an_inherited_init() {
    let m = module(vec![
        kind("Base", |k| {
            k.storage = Storage::OpenDict;
            k.resolution = Resolution::C3;
            k.receiver = Receiver::BoundSelf;
            k.members.push(MemberDecl::method("__init__", "Base::__init__"));
        }),
        kind("Child", |k| {
            k.storage = Storage::OpenDict;
            k.resolution = Resolution::C3;
            k.receiver = Receiver::BoundSelf;
            k.parents = vec![KindRef::new("Base")];
            k.construct = Construct::PyNewInit { new_fn: None, init_fn: None };
        }),
    ]);
    run_both(
        r#"
.func main
    PUSH_STR "rex"
    CONSTRUCT "Child" 1
    POP
    STORE 0
    LOAD 0
    GET_MEMBER "name"
    CAP_CALL "io.print" 1
    LOAD 0
    IS_KIND "Base"
    CAP_CALL "io.print" 1
    HALT
.func Base::__init__ 2
    STORE 0
    STORE 1
    LOAD 0
    LOAD 1
    SET_MEMBER "name"
    POP
    PUSH_NULL
    RET
"#,
        m,
        "rex\ntrue\n",
    );
}

#[test]
fn an_inconsistent_mro_is_refused_at_load() {
    let c3 = |name: &str, parents: &[&str]| {
        kind(name, |k| {
            k.resolution = Resolution::C3;
            k.parents = parents.iter().map(|p| KindRef::new(*p)).collect();
        })
    };
    let m = module(vec![
        c3("A", &[]),
        c3("B", &[]),
        c3("X", &["A", "B"]),
        c3("Y", &["B", "A"]),
        c3("Z", &["X", "Y"]),
    ]);
    fails_on_both(
        ".func main\n    PUSH_STR \"never\"\n    CAP_CALL \"io.print\" 1\n    HALT\n",
        m,
        "the resolution order of `Z`: Cannot create a consistent method resolution order (MRO) for bases A, B",
    );
}

#[test]
fn a_method_two_traits_provide_is_ambiguous() {
    let m = module(vec![kind("P", |k| {
        for t in ["Shape", "Solid"] {
            let mut c = Conformance::new(t, Satisfaction::Nominal);
            c.members.push(MemberDecl::method("area", format!("P::{t}::area")));
            k.conforms.push(c);
        }
    })]);
    fails_on_both(
        ".func main\n    CONSTRUCT \"P\" 0\n    CALL_MEMBER \"area\" 0\n    HALT\n.func P::Shape::area 1\n    RET\n.func P::Solid::area 1\n    RET\n",
        m,
        "`area` on `P` is ambiguous: it comes from Shape and Solid",
    );
}

#[test]
fn a_method_call_checks_the_arity() {
    let m = module(vec![kind("P", |k| k.members.push(MemberDecl::method("m", "P::m")))]);
    fails_on_both(
        ".func main\n    PUSH 1\n    PUSH 2\n    CONSTRUCT \"P\" 0\n    CALL_MEMBER \"m\" 2\n    HALT\n.func P::m 2\n    RET\n",
        m,
        "function `P.m` takes 1 argument(s), got 2",
    );
}

#[test]
fn arities_and_member_ops_survive_disassembly() {
    let p = program(SHOW, module(vec![kind("S", |k| k.storage = sealed(&["x"]))]));
    let text = disassemble(&p);
    assert!(text.contains(".func S::show 1"), "{text}");
    assert!(text.contains("CONSTRUCT \"S\" 1"), "{text}");
    assert!(text.contains("GET_MEMBER \"x\""), "{text}");
    let back = assemble(&text, Some(&["io.print"]), None).expect("reassembles");
    assert_eq!(back.code, p.code);
    assert_eq!(back.manifest.functions["S::show"].arity, Some(1));
    assert_eq!(back.manifest.functions["main"].arity, None);
}

#[test]
fn fastvm_lowers_construct_of_a_map_kind_and_refuses_the_other_member_ops() {
    let m = || module(vec![kind("P", |k| k.storage = sealed(&["x"]))]);
    let ok = program(".func main\n    CONSTRUCT \"P\" 0\n    PUSH 1\n    SET_FIELD \"x\"\n    HALT\n", m());
    assert!(crate::fastvm::lower_bytecode_program(&ok).is_ok());
    for op in ["GET_MEMBER \"x\"", "IS_KIND \"P\"", "CALL_MEMBER \"x\" 0", "COPY"] {
        let p = program(&format!(".func main\n    CONSTRUCT \"P\" 0\n    {op}\n    HALT\n"), m());
        let Err(e) = crate::fastvm::lower_bytecode_program(&p) else {
            panic!("{op} must be refused");
        };
        assert!(e.to_string().contains("COSM member ops need Kinds"), "{op}: {e}");
    }
    // `TYPEOF` would name a map where the VMs name the Kind.
    let p = program(".func main\n    NEW_RECORD \"P\"\n    TYPEOF\n    HALT\n", m());
    let Err(e) = crate::fastvm::lower_bytecode_program(&p) else { panic!("TYPEOF must be refused") };
    assert!(e.to_string().contains("`type_of` in a program with Kinds isn't supported on FastVM or the JIT"), "{e}");
}
