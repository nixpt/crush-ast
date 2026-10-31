//! COSM K1.3 (CRUSH-zorro-42): the kind table survives every path a compiled
//! program takes — in memory, CASM JSON, `casm_to_vm`, the CVM1 blob — and
//! a program that has one still runs on the scheduler VM and the portable VM.

use crush_cosm::CosmModule;
use crush_lang_sdk::compile::{casm_to_vm, compile_crush_to_casm};
use crush_vm::{PortableVm, Quotas};

/// The Point/Shape fixture from crush-cosm (COSM K1.1).
const POINT_FIXTURE: &str = include_str!("../../crush-cosm/tests/fixtures/crush_point.cosm.json");

/// Point/Shape in Crush, compiled by the real frontend.
const PROGRAM: &str = r#"
struct Point { x: int, y: int }
impl Point {
    fn new(x, y) { return Point { x: x, y: y }; }
    fn norm2(self) { return self.x * self.x + self.y * self.y; }
}
trait Shape { fn area(self); fn sides(self) { return 4; } }
impl Shape for Point { fn area(self) { return 100; } }
fn main() {
    let p = Point::new(3, 4)
    print(Point::norm2(p) + Point::Shape::area(p) + Shape::sides(p))
}
"#;

/// CASM JSON → `casm::Program` → `casm_to_vm` → blob → loaded CVM1 program.
fn through_every_form(casm: &casm::Program) -> crush_vm::Program {
    let json = casm.serialize(casm::Format::Json).expect("CASM JSON");
    let casm = casm::Program::deserialize(&json, casm::Format::Json).expect("CASM JSON loads");
    let vm = casm_to_vm(&casm).expect("casm_to_vm");
    assert_eq!(vm.cosm, casm.cosm, "casm_to_vm carries the kind table");
    crush_vm::Program::from_blob(&vm.to_blob()).expect("blob loads")
}

#[test]
fn the_point_fixture_round_trips_casm_json_to_cvm1_blob() {
    let fixture = CosmModule::from_json(POINT_FIXTURE).unwrap();
    let mut casm = compile_crush_to_casm("fn main() { print(1) }").unwrap();
    casm.cosm = Some(fixture.clone());
    let loaded = through_every_form(&casm);
    assert_eq!(loaded.cosm, Some(fixture));
    assert!(crush_vm::disassemble(&loaded).contains(";   kind Point (crush)"));
}

#[test]
fn a_compiled_crush_program_carries_its_kind_table_into_the_blob() {
    let casm = compile_crush_to_casm(PROGRAM).unwrap();
    let want = crush_frontend::lower_to_cosm(PROGRAM).unwrap().expect("declares a kind");
    assert_eq!(casm.cosm.as_ref(), Some(&want), "the compiler hands its module to CASM");
    assert_eq!(want.kinds[0].name, "Point");
    assert_eq!(want.interfaces[0].name, "Shape");

    let loaded = through_every_form(&casm);
    assert_eq!(loaded.cosm, Some(want));
    // The blob is version 3: it has the kind-table section.
    assert_eq!(casm_to_vm(&casm).unwrap().to_blob()[4], crush_vm::bytecode::VERSION_KIND_TABLE);

    // And it runs, as before, on both VMs.
    let out = crush_vm::run(&loaded, &Quotas::default()).expect("scheduler").output;
    assert_eq!(out, "129\n");
    let mut vm = PortableVm::new(loaded);
    vm.set_input(crush_vm::InputSource::supplied(""));
    while !vm.is_halted() {
        assert!(vm.step().expect("portable vm").is_none(), "unexpected pause");
    }
    assert_eq!(vm.take_output(), "129\n");
}

#[test]
fn programs_without_declarations_have_no_kind_table() {
    let casm = compile_crush_to_casm("fn main() { print(1) }").unwrap();
    assert_eq!(casm.cosm, None);
    let blob = casm_to_vm(&casm).unwrap().to_blob();
    assert_eq!(blob[4], crush_vm::bytecode::VERSION_NO_KIND_TABLE);
    let json = String::from_utf8(casm.serialize(casm::Format::Json).unwrap()).unwrap();
    assert!(!json.contains("\"cosm\""), "{json}");
}

/// A kind table the VMs can't run (COSM K1.4 runs every other strategy) is
/// refused when the CVM1 program is loaded into a VM; a `value(copy)` Kind,
/// refused here before K1.4, now runs.
#[test]
fn a_kind_table_the_vms_cant_honour_is_refused_at_load() {
    let mut casm = compile_crush_to_casm(PROGRAM).unwrap();
    casm.cosm.as_mut().unwrap().kinds[0].semantics =
        crush_cosm::Semantics::Value(crush_cosm::ValueMode::Copy);
    let loaded = through_every_form(&casm);
    assert_eq!(crush_vm::run(&loaded, &Quotas::default()).expect("runs").output, "129\n");

    casm.cosm.as_mut().unwrap().kinds[0].resolution = crush_cosm::Resolution::EmbeddingPromotion;
    let loaded = through_every_form(&casm);
    let e = crush_vm::run(&loaded, &Quotas::default()).unwrap_err().to_string();
    assert!(e.contains("COSM strategies not supported on this VM yet"), "{e}");
    assert!(e.contains("`Point` declares resolution: embedding_promotion"), "{e}");
}
