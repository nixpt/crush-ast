//! #504: `Array`, `List`, `Set` and `Map` are accepted as bare type annotations
//! (element type `Any`). A declared struct of the same name wins. Element-type
//! syntax (`Array<Int>`, `[Int]`) is still unsupported but says so.

use crush_cosm::CosmType;
use crush_frontend::compile_crush_source_with_cosm;
use crush_frontend::parser::Parser;

fn compile_err(src: &str) -> String {
    match compile_crush_source_with_cosm(src) {
        Ok(_) => panic!("expected a compile error for:\n{src}"),
        Err(e) => format!("{e:#}"),
    }
}

fn compile_ok(src: &str) -> Option<crush_cosm::CosmModule> {
    compile_crush_source_with_cosm(src)
        .unwrap_or_else(|e| panic!("compile failed: {e:#}\n{src}"))
        .1
}

fn parse_err(src: &str) -> String {
    match Parser::parse(src) {
        Ok(_) => panic!("expected a parse error for:\n{src}"),
        Err(errors) => errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

const NAMES: [(&str, &str); 4] = [
    ("Array", "[1, 2]"),
    ("List", "[1, 2]"),
    ("Set", "[1, 2]"),
    ("Map", "{\"a\": 1}"),
];

fn field_type(module: &crush_cosm::CosmModule, kind: &str, field: &str) -> CosmType {
    let k = module
        .kinds
        .iter()
        .find(|k| k.name == kind)
        .expect("kind in table");
    k.declared_fields()
        .iter()
        .find(|f| f.name == field)
        .expect("field")
        .ty
        .clone()
}

#[test]
fn each_name_compiles_as_field_parameter_return_and_let_annotation() {
    for (name, _) in NAMES {
        let src = format!(
            "struct Holder {{ cells: {name} }}\n\
             fn take(x: {name}) -> {name} {{ return x }}\n\
             fn main() {{ let h = Holder {{ cells: 1 }}; let y: {name} = h.cells; return take(y) }}\n"
        );
        compile_ok(&src);
        // The lowercase twin, as every scalar name has one.
        compile_ok(&src.replace(name, &name.to_lowercase()));
    }
}

#[test]
fn field_type_in_the_kind_table_is_the_container_with_any_elements() {
    for (name, want) in [
        ("Array", CosmType::Array(Box::new(CosmType::Any))),
        ("List", CosmType::List(Box::new(CosmType::Any))),
        ("Set", CosmType::Set(Box::new(CosmType::Any))),
        ("Map", CosmType::Map(Box::new(CosmType::Any))),
    ] {
        let module = compile_ok(&format!("struct S {{ cells: {name}, n: Int }}\n")).unwrap();
        assert_eq!(field_type(&module, "S", "cells"), want, "{name}");
        // The scalar next to it is unchanged.
        assert_eq!(field_type(&module, "S", "n"), CosmType::Int);
    }
}

#[test]
fn array_annotation_accepts_an_array_and_rejects_a_non_array() {
    compile_ok("fn f(x: Array) { return 1 }\nfn main() { return f([1, 2]) }\n");
    compile_ok("fn f(x: Array) { return 1 }\nfn main() { return f([\"a\", 2.5]) }\n");
    compile_ok("fn main() { let x: Array = [1, 2]\n return x }\n");
    // Same kind of error as a scalar annotation.
    let e = compile_err("fn f(x: Array) { return 1 }\nfn main() { return f(5) }\n");
    assert!(e.contains("Argument 0 to 'f' has wrong type"), "{e}");
    let scalar = compile_err("fn f(x: Int) { return 1 }\nfn main() { return f([1]) }\n");
    assert!(
        scalar.contains("Argument 0 to 'f' has wrong type"),
        "{scalar}"
    );
    let e = compile_err("fn main() { let x: Array = 5\n return x }\n");
    assert!(e.contains("Type mismatch for variable 'x'"), "{e}");
}

#[test]
fn map_list_and_set_annotations_check_the_value_kind_too() {
    compile_ok("fn f(m: Map) { return 1 }\nfn main() { return f({\"a\": 1, \"b\": 2}) }\n");
    compile_ok("fn f(l: List) { return 1 }\nfn main() { return f([1, 2]) }\n");
    let e = compile_err("fn f(m: Map) { return 1 }\nfn main() { return f([1]) }\n");
    assert!(e.contains("wrong type"), "{e}");
    let e = compile_err("fn f(l: List) { return 1 }\nfn main() { return f(\"s\") }\n");
    assert!(e.contains("wrong type"), "{e}");
    // Crush source has no value that is a `Set`, so an array is not one.
    let e = compile_err("fn f(s: Set) { return 1 }\nfn main() { return f([1]) }\n");
    assert!(e.contains("wrong type"), "{e}");
    // An unannotated (Any) value always passes.
    compile_ok("fn f(s: Set) { return 1 }\nfn main(a) { return f(a) }\n");
}

#[test]
fn struct_field_annotations_are_not_checked_at_construction_scalar_or_container() {
    // Today a struct's field types are metadata: `x: Int` takes a string.
    compile_ok("struct P { x: Int }\nfn main() { let p = P { x: \"a\" }; return p }\n");
    compile_ok("struct H { cells: Array }\nfn main() { let h = H { cells: 5 }; return h }\n");
}

#[test]
fn a_declared_struct_named_like_a_container_wins() {
    for (name, _) in NAMES {
        let src = format!(
            "struct {name} {{ n: Int }}\n\
             struct Holder {{ item: {name} }}\n\
             fn take(x: {name}) {{ return x.n }}\n\
             fn main() {{ let v = {name} {{ n: 3 }}; let h = Holder {{ item: v }}; return take(h.item) }}\n"
        );
        let module = compile_ok(&src).unwrap();
        // The field is the struct, not the container.
        assert_eq!(
            field_type(&module, "Holder", "item"),
            CosmType::TypeRef(name.to_string())
        );
        // The parameter is the struct too: an array is refused.
        let bad = src.replace("take(h.item)", "take([1, 2])");
        let e = compile_err(&bad);
        assert!(e.contains("wrong type"), "{name}: {e}");
    }
}

#[test]
fn a_declared_struct_wins_even_when_declared_after_its_use() {
    // Same result as before #504: an error, not a silent container.
    let e = compile_err(
        "struct Holder { item: List }\nstruct List { n: Int }\nfn main() { return 1 }\n",
    );
    assert!(e.contains("Unknown type: List"), "{e}");
}

#[test]
fn a_self_referential_struct_named_like_a_container_is_unchanged() {
    // A struct is not in the struct table while its own fields resolve, so a
    // self-reference is "Unknown type" for every name, container or not. #504
    // keeps that: the declared name still wins over the container meaning.
    for name in ["List", "Node"] {
        let e = compile_err(&format!(
            "struct {name} {{ next: {name} }}\nfn main() {{ return 1 }}\n"
        ));
        assert!(e.contains(&format!("Unknown type: {name}")), "{e}");
    }
}

#[test]
fn a_trait_named_like_a_container_still_is_not_a_type() {
    let e =
        compile_err("trait List { fn len(self); }\nstruct S { l: List }\nfn main() { return 1 }\n");
    assert!(e.contains("Unknown type: List"), "{e}");
}

#[test]
fn unknown_names_still_say_unknown_type() {
    let e = compile_err("struct S { cells: Vec }\nfn main() { return 1 }\n");
    assert!(e.contains("Unknown type: Vec"), "{e}");
    // `Tuple` has no bare form: an unknown arity has no CAST type.
    let e = compile_err("struct S { cells: Tuple }\nfn main() { return 1 }\n");
    assert!(e.contains("Unknown type: Tuple"), "{e}");
}

#[test]
fn programs_without_container_names_have_the_same_kind_table() {
    let module =
        compile_ok("struct Other { y: Float }\nstruct P { x: Int, name: String, o: Other }\n")
            .unwrap();
    assert_eq!(field_type(&module, "P", "x"), CosmType::Int);
    assert_eq!(field_type(&module, "P", "name"), CosmType::String);
    assert_eq!(
        field_type(&module, "P", "o"),
        CosmType::TypeRef("Other".into())
    );
}

#[test]
fn element_type_syntax_says_it_has_no_annotation_syntax_yet() {
    for src in [
        "struct S { cells: Array<Int> }\n",
        "struct S { cells: Map<String, Int> }\n",
        "fn f(x: List<Int>) { return x }\n",
        "fn main() { let x: Array<Int> = [1]\n return x }\n",
        "fn f(x) -> Array<Int> { return x }\n",
    ] {
        let e = parse_err(src);
        assert!(
            e.contains("container element types have no annotation syntax yet"),
            "{src}: {e}"
        );
        assert!(e.contains("Write the bare"), "{src}: {e}");
        assert!(
            !e.contains("Expected }") && !e.contains("Unknown type"),
            "{e}"
        );
    }
    let e = parse_err("struct S { cells: Array<Int> }\n");
    assert!(
        e.contains("write") || e.contains("Write the bare `Array`"),
        "{e}"
    );
    for src in [
        "struct S { cells: [Int] }\n",
        "fn f(x: [Int]) { return x }\n",
        "fn f(x) -> [Int] { return x }\n",
    ] {
        let e = parse_err(src);
        assert!(
            e.contains("container element types have no annotation syntax yet"),
            "{src}: {e}"
        );
        assert!(e.contains("Write the bare `Array`"), "{src}: {e}");
    }
}

#[test]
fn type_annotations_that_parsed_before_still_parse() {
    // Return types are skipped by the parser; a bare container there is fine.
    compile_ok("fn f(x) -> Array { return x }\nfn main() { return f([1]) }\n");
    compile_ok("fn f(x: Int) -> Int { return x }\nfn main() { return f(1) < 2 }\n");
    compile_ok("fn main() { let x: Int = 1\n return x < 2 }\n");
}
