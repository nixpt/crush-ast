//! COSM K1.2 (CRUSH-zorro-41): Crush programs with `impl` / `trait` run their
//! lifted methods through qualified calls (`Point::new(3, 4)`,
//! `Point::norm2(p)`) — on the scheduler VM and the portable VM alike — and,
//! since K1.4 (CRUSH-zorro-43), through the receiver (`p.norm2()`).

use crush_vm::{InputSource, PortableVm, Quotas};

const PROGRAM: &str = r#"
struct Point { x: int, y: int }
impl Point {
    fn new(x, y) { return Point { x: x, y: y }; }
    fn norm2(self) { return self.x * self.x + self.y * self.y; }
    fn shift(self, d) { self.x = self.x + d; return self; }
}
trait Shape { fn area(self); fn describe(self) { return "a shape"; } }
impl Shape for Point { fn area(self) { return 0; } }

fn main() {
    let p = Point::new(3, 4)
    print(Point::norm2(p))
    print(Point::Shape::area(p))
    print(Shape::describe(p))
    Point::shift(p, 1)
    print(p.x)
    let f = |q| Point::norm2(q)
    print(f(Point { x: 1, y: 1 }))
}
"#;

fn compile(src: &str) -> crush_vm::Program {
    crush_lang_sdk::compile::compile_crush_source(src).unwrap_or_else(|e| panic!("compile: {e:#}"))
}

fn on_scheduler(program: &crush_vm::Program) -> String {
    crush_vm::run(program, &Quotas::default()).expect("scheduler").output
}

fn on_portable(program: &crush_vm::Program) -> String {
    let mut vm = PortableVm::new(program.clone());
    vm.set_input(InputSource::supplied(""));
    while !vm.is_halted() {
        assert!(vm.step().expect("portable vm").is_none(), "unexpected pause");
    }
    vm.take_output()
}

#[test]
fn qualified_calls_run_the_lifted_methods_on_both_vms() {
    let program = compile(PROGRAM);
    // `self` is a reference (Crush's default semantics), so `shift` is visible through `p`.
    let want = "25\n0\na shape\n4\n2\n";
    assert_eq!(on_scheduler(&program), want);
    assert_eq!(on_portable(&program), want);
}

#[test]
fn receiver_calls_resolve_on_the_kind() {
    // K1.4 (CRUSH-zorro-43): `p.norm2()` is CALL_MEMBER, not the global
    // capability `norm2`.
    let src = PROGRAM.replace("print(Point::norm2(p))", "print(p.norm2())");
    let program = compile(&src);
    let want = "25\n0\na shape\n4\n2\n";
    assert_eq!(on_scheduler(&program), want);
    assert_eq!(on_portable(&program), want);
}

#[test]
fn structs_without_impls_are_unchanged() {
    let program = compile(
        "struct P { x: int, y: int }\nfn main() {\n  let a = P { x: 1, y: 2 }\n  let b = a\n  b.x = 7\n  print(a.x)\n  print(a == P { x: 7, y: 2 })\n}\n",
    );
    // Reference semantics and structural equality, as before COSM.
    assert_eq!(on_scheduler(&program), "7\ntrue\n");
    assert_eq!(on_portable(&program), "7\ntrue\n");
}
