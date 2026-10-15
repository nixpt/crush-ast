//! COSM K1.2 (CRUSH-zorro-41): Crush's `impl`, `trait` and `@kind(…)` surface —
//! parsing into CAST and rendering back.

use crush_cast::{KindValue, Program, Statement};
use crush_frontend::parser::Parser;
use crush_frontend::render::render_program;

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

fn parse(src: &str) -> Program {
    Parser::parse(src).unwrap_or_else(|e| panic!("parse failed: {e:?}\n{src}"))
}

fn top_level(src: &str) -> Vec<Statement> {
    parse(src).functions["main"].body.clone()
}

fn parse_errors(src: &str) -> String {
    match Parser::parse(src) {
        Ok(_) => panic!("expected a parse error for:\n{src}"),
        Err(errors) => errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n"),
    }
}

/// `render(parse(src))` is a fixed point: re-parsing and re-rendering it
/// gives the same text, and the CAST survives (compared as JSON, positions
/// aside).
fn assert_round_trip(src: &str) -> String {
    let rendered = render_program(&parse(src));
    let again = render_program(&parse(&rendered));
    assert_eq!(again, rendered, "render is not a fixed point");
    let strip = |p: &Program| {
        let mut v = serde_json::to_value(&p.functions["main"].body).unwrap();
        strip_meta(&mut v);
        v
    };
    assert_eq!(strip(&parse(src)), strip(&parse(&rendered)), "CAST changed across render");
    rendered
}

fn strip_meta(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(o) => {
            o.remove("meta");
            o.values_mut().for_each(strip_meta);
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(strip_meta),
        _ => {}
    }
}

#[test]
fn impl_and_trait_parse_to_cast_nodes() {
    let body = top_level(POINT);
    assert_eq!(body.len(), 4, "{body:#?}");

    let Statement::StructDef { name, kind, .. } = &body[0] else { panic!("{:?}", body[0]) };
    assert_eq!(name, "Point");
    assert!(kind.is_none(), "a plain struct carries no @kind");

    let Statement::ImplBlock { type_name, trait_name, methods, .. } = &body[1] else {
        panic!("{:?}", body[1])
    };
    assert_eq!((type_name.as_str(), trait_name), ("Point", &None));
    let names: Vec<_> = methods.iter().map(|m| (m.name.as_str(), m.has_self())).collect();
    assert_eq!(names, [("new", false), ("norm2", true)]);
    assert!(methods.iter().all(|m| m.body.is_some()));
    // `self` is an ordinary parameter name in the CAST.
    assert_eq!(methods[1].params[0].0, "self");

    let Statement::TraitDef { name, methods, .. } = &body[2] else { panic!("{:?}", body[2]) };
    assert_eq!(name, "Shape");
    let sigs: Vec<_> = methods.iter().map(|m| (m.name.as_str(), m.body.is_some())).collect();
    assert_eq!(sigs, [("area", false), ("describe", true)], "signature, then default method");

    let Statement::ImplBlock { type_name, trait_name, methods, .. } = &body[3] else {
        panic!("{:?}", body[3])
    };
    assert_eq!((type_name.as_str(), trait_name.as_deref()), ("Point", Some("Shape")));
    assert_eq!(methods.len(), 1);
}

#[test]
fn impl_and_trait_render_round_trip() {
    let rendered = assert_round_trip(POINT);
    assert_eq!(
        rendered,
        "\
struct Point { x: Int, y: Int }
impl Point {
    fn new(x, y) {
        return {x: x, y: y}
    }
    fn norm2(self) {
        return self.x * self.x + self.y * self.y
    }
}
trait Shape {
    fn area(self);
    fn describe(self) {
        return \"a shape\"
    }
}
impl Shape for Point {
    fn area(self) {
        return 0
    }
}
"
    );
}

#[test]
fn kind_annotation_parses_and_round_trips() {
    let src = "\
@kind(semantics: value(copy))
struct Vec2 { x: float, y: float }
@kind(storage: open_dict, resolution: c3, parents: [Base, Mixin], receiver: bound_self)
struct Widget { }
";
    let body = top_level(src);
    let Statement::StructDef { name, kind: Some(kind), .. } = &body[0] else { panic!("{:?}", body[0]) };
    assert_eq!(name, "Vec2");
    assert_eq!(kind.args.len(), 1);
    assert_eq!(kind.args[0].key, "semantics");
    assert_eq!(
        kind.args[0].value,
        KindValue::Apply { name: "value".into(), args: vec![KindValue::Name { name: "copy".into() }] }
    );
    let Statement::StructDef { kind: Some(kind), .. } = &body[1] else { panic!("{:?}", body[1]) };
    let keys: Vec<_> = kind.args.iter().map(|a| a.key.as_str()).collect();
    assert_eq!(keys, ["storage", "resolution", "parents", "receiver"]);
    assert_eq!(
        kind.args[2].value,
        KindValue::List {
            items: vec![KindValue::Name { name: "Base".into() }, KindValue::Name { name: "Mixin".into() }]
        }
    );
    assert_eq!(
        assert_round_trip(src),
        "\
@kind(semantics: value(copy))
struct Vec2 { x: Float, y: Float }
@kind(storage: open_dict, resolution: c3, parents: [Base, Mixin], receiver: bound_self)
struct Widget {}
"
    );
}

#[test]
fn kind_annotation_spans_lines_and_takes_keyword_values() {
    // Newlines inside `@kind(…)`, a trailing comma, and a keyword as a value
    // (`fn` in `ctor(fn)`).
    let src = "@kind(\n    construct: ctor(fn),\n    equality: identity,\n)\nstruct P { x }\n";
    let body = top_level(src);
    let Statement::StructDef { kind: Some(kind), .. } = &body[0] else { panic!("{:?}", body[0]) };
    assert_eq!(kind.args[0].value.to_string(), "ctor(fn)");
    assert_eq!(kind.args[1].value.to_string(), "identity");
}

#[test]
fn trait_signatures_end_at_semicolon_or_newline() {
    let src = "\
trait Shape {
    fn area(self) -> float
    fn name(self) -> string;
    fn describe(self) { return \"a shape\" }
}
";
    let body = top_level(src);
    let Statement::TraitDef { methods, .. } = &body[0] else { panic!("{:?}", body[0]) };
    let sigs: Vec<_> = methods.iter().map(|m| (m.name.as_str(), m.body.is_some())).collect();
    assert_eq!(sigs, [("area", false), ("name", false), ("describe", true)]);
}

#[test]
fn qualified_paths_parse_to_lifted_function_names() {
    let body = top_level("struct Point { x, y }\nlet p = Point::new(1, 2)\nlet n = Point::norm2(p)\n");
    let calls: Vec<String> = body
        .iter()
        .filter_map(|s| match s {
            Statement::VarDecl { value: crush_cast::Expression::Call { function, .. }, .. } => {
                Some(function.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(calls, ["Point::new", "Point::norm2"]);
    assert_round_trip("let n = Point::Shape::area(p)\n");
}

#[test]
fn impl_and_trait_are_contextual_keywords() {
    // A variable named `impl` or `trait` keeps working: only `impl X {`,
    // `impl X for Y {` and `trait X {` start a declaration.
    let body = top_level("let impl = 1\nlet trait = impl + 1\nprint(trait)\n");
    assert_eq!(body.len(), 3);
    assert!(matches!(&body[0], Statement::VarDecl { name, .. } if name == "impl"));
}

#[test]
fn impl_methods_may_be_async() {
    let body = top_level("struct P { x }\nimpl P { async fn load(self) { return 1 } }\n");
    let Statement::ImplBlock { methods, .. } = &body[1] else { panic!("{:?}", body[1]) };
    assert!(methods[0].is_async);
    assert_round_trip("struct P { x }\nimpl P { async fn load(self) { return 1 } }\n");
}

#[test]
fn cast_json_round_trips_through_serde() {
    let program = parse(&format!("@kind(semantics: value(copy))\n{POINT}"));
    let json = serde_json::to_string(&program).unwrap();
    let back: Program = serde_json::from_str(&json).unwrap();
    assert_eq!(serde_json::to_value(&back).unwrap(), serde_json::to_value(&program).unwrap());
    // Absent `@kind` and trait name are omitted, not serialized as null.
    let plain = serde_json::to_string(&top_level("struct P { x }\nimpl P { fn m(self) { return 1 } }\n")).unwrap();
    assert!(!plain.contains("\"kind\""), "{plain}");
    assert!(!plain.contains("trait_name"), "{plain}");
}

#[test]
fn impl_inside_a_function_is_a_parse_error() {
    let err = parse_errors("fn f() {\n    impl P { fn m(self) { return 1 } }\n}\n");
    assert!(err.contains("`impl` blocks are only allowed at the top level"), "{err}");
    let err = parse_errors("fn f() {\n    trait T { fn m(self); }\n}\n");
    assert!(err.contains("`trait` blocks are only allowed at the top level"), "{err}");
}

#[test]
fn impl_method_without_body_is_a_parse_error() {
    let err = parse_errors("struct P { x }\nimpl P { fn m(self); }\n");
    assert!(err.contains("a method in `impl P` needs a body"), "{err}");
}

#[test]
fn only_methods_are_allowed_in_impl_and_trait() {
    let err = parse_errors("struct P { x }\nimpl P { let y = 1 }\n");
    assert!(err.contains("`fn` or `}` in `impl P`"), "{err}");
    let err = parse_errors("trait T { x: int }\n");
    assert!(err.contains("`fn` or `}` in `trait T`"), "{err}");
}

#[test]
fn kind_must_annotate_a_struct() {
    let err = parse_errors("@kind(semantics: reference)\nfn f() { return 1 }\n");
    assert!(err.contains("a `struct` after `@kind(…)`"), "{err}");
    let err = parse_errors("@kind(semantics)\nstruct P { x }\n");
    assert!(err.contains("`:`"), "{err}");
}
