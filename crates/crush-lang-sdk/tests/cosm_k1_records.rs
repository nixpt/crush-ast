//! COSM K1.4 (CRUSH-zorro-43): Crush structs are Records of their Kind on the
//! scheduler VM and the portable VM — construction, fields, method calls with
//! each receiver strategy, a C3 diamond, `value(copy)` vs `reference`
//! aliasing, bound methods as values, constructors, and struct patterns that
//! test the Kind (which closes CRUSH-zorro-37). Every program runs on both
//! VMs and must print the same thing on each.

use crush_vm::{InputSource, PortableVm, Quotas};

fn compile(src: &str) -> crush_vm::Program {
    crush_lang_sdk::compile::compile_crush_source(src).unwrap_or_else(|e| panic!("compile: {e:#}"))
}

fn on_scheduler(program: &crush_vm::Program) -> Result<String, String> {
    crush_vm::run(program, &Quotas::default()).map(|r| r.output).map_err(|e| e.to_string())
}

fn on_portable(program: &crush_vm::Program) -> Result<String, String> {
    let mut vm = PortableVm::new(program.clone());
    vm.set_input(InputSource::supplied(""));
    while !vm.is_halted() {
        match vm.step() {
            Ok(None) => {}
            Ok(Some(_)) => return Err("unexpected pause".to_string()),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(vm.take_output())
}

/// Run `src` on both VMs; both must print `want`.
fn run_both(src: &str, want: &str) {
    let program = compile(src);
    assert_eq!(on_scheduler(&program).as_deref(), Ok(want), "scheduler VM");
    assert_eq!(on_portable(&program).as_deref(), Ok(want), "portable VM");
}

/// Run `src` on both VMs; both must fail with an error containing `want`.
fn fails_on_both(src: &str, want: &str) {
    let program = compile(src);
    for (vm, got) in [("scheduler VM", on_scheduler(&program)), ("portable VM", on_portable(&program))] {
        let e = got.expect_err(vm);
        assert!(e.contains(want), "{vm}: want {want:?} in {e:?}");
    }
}

#[test]
fn construction_field_get_set_typeof_and_print() {
    run_both(
        r#"
struct Point { x: int, y: int }
fn main() {
    let p = Point { y: 2, x: 1 }
    print(p)
    p.x = 10
    print(p.x + p.y)
    let q = new Point()
    print(q)
    q.y = 5
    print(q)
    print(q.x)
    print(p == Point { x: 10, y: 2 })
    print(p == Point { x: 11, y: 2 })
    print([p, q])
}
"#,
        // A Record prints its fields in the order they were first set: exactly
        // the map a struct used to be.
        "{y: 2, x: 1}\n12\n{}\n{y: 5}\nnull\ntrue\nfalse\n[{y: 2, x: 10}, {y: 5}]\n",
    );
}

#[test]
fn a_sealed_struct_has_no_room_for_other_fields() {
    fails_on_both(
        "struct P { x: int }\nfn main() {\n  let p = P { x: 1 }\n  p.z = 2\n}\n",
        "`P` is sealed: it has no field `z` (fields: x)",
    );
}

#[test]
fn methods_resolve_on_the_receiver() {
    run_both(
        r#"
struct Point { x: int, y: int }
impl Point {
    fn new(x, y) { return Point { x: x, y: y }; }
    fn norm2(self) { return self.x * self.x + self.y * self.y; }
    fn shift(self, d) { self.x = self.x + d; return self; }
}
trait Shape { fn area(self); fn describe(self) { return "a shape"; } }
impl Shape for Point { fn area(self) { return 100; } }
fn main() {
    let p = Point::new(3, 4)
    print(p.norm2())
    p.shift(1)
    print(p.x)
    print(p.area())
    print(p.describe())
    print(Point::norm2(p))
}
"#,
        // explicit_ref (Crush's default): `shift` mutates `p` itself.
        "25\n4\n100\na shape\n32\n",
    );
}

#[test]
fn a_method_call_never_reaches_a_global_capability() {
    // `len` is a method here; on a non-struct receiver it is an error, not
    // the `len` capability.
    fails_on_both(
        "struct S { n: int }\nimpl S { fn len(self) { return self.n; } }\nfn main() {\n  let a = [1, 2]\n  print(a.len())\n}\n",
        "`.len(…)` on a array: methods resolve on struct instances",
    );
    fails_on_both(
        "struct S { n: int }\nimpl S { fn len(self) { return self.n; } }\nstruct T { n: int }\nfn main() {\n  let t = T { n: 1 }\n  print(t.len())\n}\n",
        "`T` has no method `len`",
    );
}

#[test]
fn explicit_value_receives_a_copy_and_explicit_ref_mut_the_instance() {
    run_both(
        r#"
@kind(semantics: value(copy), receiver: explicit_value)
struct ByValue { n: int }
impl ByValue { fn bump(self) { self.n = self.n + 1; return self.n; } }

@kind(semantics: value(copy), receiver: explicit_ref_mut)
struct ByRef { n: int }
impl ByRef { fn bump(self) { self.n = self.n + 1; return self.n; } }

fn main() {
    let a = ByValue { n: 1 }
    print(a.bump())
    print(a.n)
    let b = ByRef { n: 1 }
    print(b.bump())
    print(b.n)
}
"#,
        "2\n1\n2\n2\n",
    );
}

#[test]
fn bound_self_methods_are_values_bound_to_their_instance() {
    run_both(
        r#"
@kind(receiver: bound_self)
struct Counter { n: int }
impl Counter { fn inc(self) { self.n = self.n + 1; return self.n; } }
fn twice(f) { f(); return f(); }
fn main() {
    let c = Counter { n: 0 }
    let f = c.inc
    print(f())
    print(twice(f))
    print(c.n)
    print(c.inc())
}
"#,
        "1\n3\n3\n4\n",
    );
}

#[test]
fn explicit_receiver_methods_read_as_the_plain_function() {
    run_both(
        r#"
struct P { x: int }
impl P { fn get(self) { return self.x; } }
fn main() {
    let p = P { x: 7 }
    let g = p.get
    print(g(p))
}
"#,
        "7\n",
    );
}

#[test]
fn bound_cls_passes_the_kind() {
    run_both(
        r#"
@kind(receiver: bound_cls)
struct Factory { }
impl Factory { fn show(self) { return self; } }
fn main() {
    let f = Factory { }
    print(f.show())
    let m = f.show
    print(m())
}
"#,
        "<kind Factory>\n<kind Factory>\n",
    );
}

#[test]
fn dynamic_this_binds_on_call_and_is_lost_on_extraction() {
    run_both(
        r#"
@kind(receiver: dynamic_this)
struct Obj { v: int }
impl Obj { fn me(self) { return self; } }
fn main() {
    let o = Obj { v: 3 }
    print(o.me())
    let g = o.me
    print(g())
}
"#,
        "{v: 3}\nnull\n",
    );
}

const DIAMOND: &str = r#"
@kind(resolution: c3)
struct Base { }
impl Base {
    fn who(self) { return "base"; }
    fn hello(self) { return "hello from base"; }
}
@kind(resolution: c3, parents: [Base])
struct Left { }
impl Left { fn who(self) { return "left"; } }
@kind(resolution: c3, parents: [Base])
struct Right { }
impl Right {
    fn who(self) { return "right"; }
    fn only_right(self) { return "right only"; }
}
@kind(resolution: c3, parents: [Left, Right])
struct Bottom { }
fn kind_of(v) {
    match v {
        Right { } => { return "a Right"; }
        _ => { return "not a Right"; }
    }
}
fn main() {
    let b = Bottom { }
    print(b.who())
    print(b.only_right())
    print(b.hello())
    print(kind_of(b))
    print(kind_of(Left { }))
}
"#;

#[test]
fn c3_diamond_lookup_follows_the_mro() {
    // MRO(Bottom) = [Bottom, Left, Right, Base].
    run_both(DIAMOND, "left\nright only\nhello from base\na Right\nnot a Right\n");
}

#[test]
fn value_copy_vs_reference_aliasing() {
    run_both(
        r#"
@kind(semantics: value(copy))
struct V { x: int }
struct R { x: int }
fn bump(v) { v.x = v.x + 1; return v; }
fn main() {
    let a = V { x: 1 }
    let b = a
    b.x = 2
    print(a.x)
    let r = R { x: 1 }
    let s = r
    s.x = 2
    print(r.x)
    let c = bump(a)
    print(a.x)
    print(c.x)
    let d = bump(r)
    print(r.x)
    let w = a.clone()
    w.x = 9
    print(a.x)
    let t = r.clone()
    t.x = 9
    print(r.x)
}
"#,
        "1\n2\n1\n2\n3\n1\n3\n",
    );
}

#[test]
fn value_move_behaves_as_copy_at_run_time() {
    run_both(
        "@kind(semantics: value(move))\nstruct M { x: int }\nfn main() {\n  let a = M { x: 1 }\n  let b = a\n  b.x = 5\n  print(a.x)\n  print(b.x)\n}\n",
        "1\n5\n",
    );
}

#[test]
fn nested_value_records_copy_with_their_container() {
    run_both(
        r#"
@kind(semantics: value(copy))
struct Inner { v: int }
@kind(semantics: value(copy))
struct Outer { i: Inner }
fn main() {
    let a = Outer { i: Inner { v: 1 } }
    let b = a
    b.i.v = 2
    print(a.i.v)
    print(b.i.v)
}
"#,
        "1\n2\n",
    );
}

#[test]
fn constructors_run_on_the_new_instance() {
    run_both(
        r#"
@kind(construct: ctor(init))
struct Acc { total: int }
impl Acc {
    fn init(self, start) { self.total = start; return 99; }
    fn add(self, n) { self.total = self.total + n; return self; }
}
fn main() {
    let a = Acc(5)
    a.add(2)
    print(a.total)
    print(a)
}
"#,
        "7\n{total: 7}\n",
    );
}

#[test]
fn a_static_constructor_is_a_factory() {
    // `ctor(new)` naming a static member: `P(…)` calls it with the arguments
    // and its result is the instance.
    run_both(
        r#"
@kind(construct: ctor(new))
struct P { x: int, tag: str }
impl P { fn new(x) { return P { x: x, tag: "made" }; } }
fn main() {
    let p = P(4)
    print(p)
    print(P::new(5).x)
}
"#,
        "{x: 4, tag: made}\n5\n",
    );
}

#[test]
fn identity_equality_compares_instances() {
    run_both(
        r#"
@kind(equality: identity)
struct Id { x: int }
struct St { x: int }
fn main() {
    let a = Id { x: 1 }
    print(a == Id { x: 1 })
    print(a == a)
    print(St { x: 1 } == St { x: 1 })
}
"#,
        "false\ntrue\ntrue\n",
    );
}

/// CRUSH-zorro-37: a struct pattern used to read the fields only, so the
/// first arm with the same field names won whatever the struct.
#[test]
fn zorro_37_struct_patterns_test_the_kind() {
    run_both(
        r#"
struct P { x: int }
struct Q { x: int }
fn which(v) {
    match v {
        P { x: a } => { return 1; }
        Q { x: b } => { return 2; }
    }
    return 0;
}
print(which(Q { x: 5 }));
"#,
        "2\n",
    );
}

#[test]
fn struct_patterns_nested_maps_and_wildcards() {
    run_both(
        r#"
struct P { x: int }
struct Q { x: int }
struct Wrap { inner: int }
fn which(v) {
    match v {
        Wrap { inner: P { x: a } } => { return "wrap of P " + a; }
        Wrap { inner: Q { x: a } } => { return "wrap of Q " + a; }
        P { x: 1 } => { return "P one"; }
        P { x: a } => { return "P " + a; }
        Q { x: b } => { return "Q " + b; }
        _ => { return "something else"; }
    }
}
fn main() {
    print(which(P { x: 1 }))
    print(which(P { x: 2 }))
    print(which(Q { x: 3 }))
    print(which(Wrap { inner: Q { x: 4 } }))
    print(which(Wrap { inner: P { x: 5 } }))
    print(which({x: 1}))
    print(which(7))
}
"#,
        "P one\nP 2\nQ 3\nwrap of Q 4\nwrap of P 5\nsomething else\nsomething else\n",
    );
}

#[test]
fn a_struct_pattern_naming_no_struct_is_a_compile_error() {
    let src = "fn main() {\n  match 1 {\n    Nope { x: a } => { print(a); }\n    _ => { print(0); }\n  }\n}\n";
    let e = format!("{:#}", crush_lang_sdk::compile::compile_crush_source(src).expect_err("refused"));
    assert!(e.contains("no struct `Nope` is declared"), "{e}");
}

#[test]
fn records_reach_capabilities_as_maps() {
    run_both(
        r#"
struct P { x: int, y: int }
fn main() {
    let p = P { x: 1, y: 2 }
    print(str.concat("p = ", p))
    print(p["y"])
    p["y"] = 5
    print(p)
}
"#,
        "p = {x: 1, y: 2}\n2\n{x: 1, y: 5}\n",
    );
}
