//! COSM K1.3 (CRUSH-zorro-42): the CVM1 kind-table section — versioning,
//! old blobs, round-trips, the disassembler — and what the scheduler VM,
//! the portable VM and FastVM's CVM1 lowering do with a kind table (since
//! K1.4, CRUSH-zorro-43, the VMs run Kinds: see `records.rs`).

use crate::assembler::{assemble, disassemble};
use crate::bytecode::{
    KIND_TABLE_FORMAT_JSON, MAGIC, Program, VERSION_KIND_TABLE, VERSION_NO_KIND_TABLE,
};
use crate::vm::{Quotas, Value, VmError, run};
use crush_cosm::{CosmModule, Semantics, ValueMode};

/// The Point/Shape fixture from crush-cosm (COSM K1.1).
const POINT_FIXTURE: &str = include_str!("../../../crush-cosm/tests/fixtures/crush_point.cosm.json");

fn point_module() -> CosmModule {
    CosmModule::from_json(POINT_FIXTURE).expect("fixture parses")
}

const SRC: &str = ".func main\n    PUSH_STR \"hi\"\n    PRINT\n    PUSH 7\n    RET\n";

fn program_with(module: Option<CosmModule>) -> Program {
    let mut p = assemble(SRC, Some(&["io.print"]), None).expect("assembly");
    p.cosm = module;
    p
}

/// A v2 blob laid out by hand, the way every runtime before the kind table
/// wrote one: MAGIC | 2 | manifest | consts | code, nothing after.
fn old_blob(version: u8) -> Vec<u8> {
    let manifest = br#"{"runtime":"casm-v0","permissions":[]}"#;
    let mut b = Vec::new();
    b.extend_from_slice(MAGIC);
    b.push(version);
    b.extend_from_slice(&(manifest.len() as u16).to_be_bytes());
    b.extend_from_slice(manifest);
    b.extend_from_slice(&0u16.to_be_bytes()); // no consts
    let code = [crate::bytecode::PUSH, 0, 0, 0, 0, 0, 0, 0, 42, crate::bytecode::HALT];
    b.extend_from_slice(&(code.len() as u32).to_be_bytes());
    b.extend_from_slice(&code);
    b
}

#[test]
fn kind_table_round_trips_through_the_blob() {
    let p = program_with(Some(point_module()));
    let blob = p.to_blob();
    assert_eq!(blob[4], VERSION_KIND_TABLE);
    let back = Program::from_blob(&blob).expect("loads");
    assert_eq!(back.cosm, Some(point_module()));
    assert_eq!(back.code, p.code);
    assert_eq!(back.consts, p.consts);
    // And it is stable: writing the loaded program gives the same bytes.
    assert_eq!(back.to_blob(), blob);
}

#[test]
fn a_program_without_a_kind_table_is_written_exactly_as_before() {
    let p = program_with(None);
    let blob = p.to_blob();
    assert_eq!(blob[4], VERSION_NO_KIND_TABLE);
    // An empty module is no kind table: same bytes, loads as `None`.
    let empty = program_with(Some(CosmModule::default())).to_blob();
    assert_eq!(empty, blob);
    assert_eq!(Program::from_blob(&empty).unwrap().cosm, None);
    // The section is pure suffix: a v3 blob is the v2 blob plus the section.
    let v3 = program_with(Some(point_module())).to_blob();
    assert_eq!(&v3[5..blob.len()], &blob[5..]);
    assert_eq!(v3[blob.len()], KIND_TABLE_FORMAT_JSON);
}

#[test]
fn old_blobs_without_the_section_still_load() {
    for version in [1u8, 2] {
        let p = Program::from_blob(&old_blob(version)).expect("old blob loads");
        assert_eq!(p.cosm, None);
        let r = run(&p, &Quotas::default()).expect("old blob runs");
        assert!(r.halted);
        assert_eq!(r.stack, vec![Value::Int(42)]);
    }
}

#[test]
fn an_unknown_opcode_is_refused_at_load() {
    // CRUSH-285 review #8: code bytes are validated while loading, so a
    // byte that isn't an opcode never reaches an executor (where it
    // could desync instruction boundaries, or NOP on a backend that
    // skips unknown bytes).
    let mut blob = old_blob(2);
    let code_at = blob.len() - 10; // the `old_blob` code section
    blob[code_at] = 0xFE;
    assert!(crate::bytecode::instruction_size(0xFE).is_none());
    let e = Program::from_blob(&blob).unwrap_err().to_string();
    assert!(e.contains("unknown opcode"), "{e}");
}

#[test]
fn a_v3_blob_must_carry_a_well_formed_section() {
    let good = program_with(Some(point_module())).to_blob();
    let v2_len = program_with(None).to_blob().len();

    // Truncated: the version says there is a section, but the blob ends.
    let mut truncated = good[..v2_len].to_vec();
    truncated[4] = VERSION_KIND_TABLE;
    assert!(matches!(Program::from_blob(&truncated), Err(crate::Error::Truncated)));
    let cut = &good[..good.len() - 1];
    assert!(matches!(Program::from_blob(cut), Err(crate::Error::Truncated)));

    // An unknown section format is an error, not a skip.
    let mut unknown = good.clone();
    unknown[v2_len] = 9;
    let e = Program::from_blob(&unknown).unwrap_err().to_string();
    assert!(e.contains("unknown kind-table format 9"), "{e}");

    // Nothing may follow the section.
    let mut trailing = good.clone();
    trailing.push(0);
    let e = Program::from_blob(&trailing).unwrap_err().to_string();
    assert!(e.contains("trailing byte"), "{e}");

    // Malformed COSM is reported as such.
    let mut bad = good[..v2_len].to_vec();
    bad[4] = VERSION_KIND_TABLE;
    bad.push(KIND_TABLE_FORMAT_JSON);
    bad.extend_from_slice(&2u32.to_be_bytes());
    bad.extend_from_slice(b"{]");
    let e = Program::from_blob(&bad).unwrap_err().to_string();
    assert!(e.starts_with("malformed kind table"), "{e}");
}

#[test]
fn a_newer_container_version_is_still_refused() {
    let mut blob = program_with(None).to_blob();
    blob[4] = VERSION_KIND_TABLE + 1;
    assert!(matches!(
        Program::from_blob(&blob),
        Err(crate::Error::UnsupportedVersion(v)) if v == VERSION_KIND_TABLE + 1
    ));
}

#[test]
fn the_disassembler_prints_the_kind_table_and_it_reassembles() {
    let p = program_with(Some(point_module()));
    let text = disassemble(&p);
    assert!(text.starts_with("; kind table (COSM): 1 Kind, 1 interface\n"), "{text}");
    assert!(text.contains(";   kind Point (crush): fields [x, y], members [new, norm2]"), "{text}");
    assert!(text.contains(";   interface Shape (crush): [area, describe]"), "{text}");
    assert!(text.contains("\n.cosm {"), "{text}");
    let back = assemble(&text, Some(&["io.print"]), None).expect("reassembles");
    assert_eq!(back.cosm, Some(point_module()));
    assert_eq!(back.code, p.code);
    // No kind table, no header.
    assert!(!disassemble(&program_with(None)).contains("kind table"));
}

#[test]
fn the_assembler_rejects_a_bad_or_duplicate_cosm_directive() {
    let e = assemble(".cosm {nope}\nHALT\n", None, None).unwrap_err().to_string();
    assert!(e.contains("malformed .cosm kind table"), "{e}");
    let line = format!(".cosm {}\n", point_module().to_json());
    let e = assemble(&format!("{line}{line}HALT\n"), None, None).unwrap_err().to_string();
    assert!(e.contains("duplicate .cosm"), "{e}");
}

/// A Kind whose instances behave exactly as today's maps (Crush's defaults)
/// runs unchanged on the scheduler VM and the portable VM.
#[test]
fn a_map_equivalent_kind_table_runs_as_before() {
    let plain = run(&program_with(None), &Quotas::default()).unwrap();
    let with = run(&program_with(Some(point_module())), &Quotas::default()).unwrap();
    assert_eq!(with.output, plain.output);
    assert_eq!(with.return_value, Some(Value::Int(7)));

    let portable = |m| crate::portable_vm::PortableVm::new(program_with(m)).run().expect("portable VM runs it");
    assert_eq!(portable(Some(point_module())).output, portable(None).output);
}

fn value_semantics_module() -> CosmModule {
    let mut m = point_module();
    m.kinds[0].semantics = Semantics::Value(ValueMode::Move);
    m
}

/// Since K1.4 (CRUSH-zorro-43) the VMs implement Kinds: a `value(…)` Kind
/// runs. What they still can't run is refused before anything runs, naming
/// each strategy — the shared rule at `casm::CosmSupport::Kinds`.
#[test]
fn the_scheduler_vm_runs_kinds_and_refuses_only_what_it_cant() {
    let r = run(&program_with(Some(value_semantics_module())), &Quotas::default()).expect("runs");
    assert_eq!(r.return_value, Some(Value::Int(7)));

    let mut m = point_module();
    m.kinds[0].resolution = crush_cosm::Resolution::PrototypeChain;
    m.kinds[0].hooks.eq = Some(crush_cosm::HookImpl::Fn(crush_cosm::FnRef::new("Point::eq")));
    m.kinds[0].hooks.str = Some(crush_cosm::HookImpl::Fn(crush_cosm::FnRef::new("Point::show")));
    m.kinds[0].members.push(crush_cosm::MemberDecl::accessor("len", Some("Point::len".into()), None));
    let e = run(&program_with(Some(m)), &Quotas::default()).unwrap_err();
    assert!(matches!(e, VmError::Unsupported(_)), "{e:?}");
    let e = e.to_string();
    assert!(e.contains("COSM strategies not supported on this VM yet: `Point` declares"), "{e}");
    for want in ["hooks eq"] {
        assert!(e.contains(want), "missing {want:?}: {e}");
    }
    // Accessors run since CRUSH-277 S4: one isn't named either.
    assert!(!e.contains("accessor"), "{e}");
    // `prototype_chain` runs since CRUSH-277 S2: it isn't named either.
    assert!(!e.contains("prototype_chain"), "{e}");
    // `str` is implemented: it isn't named.
    assert!(!e.contains("eq, str"), "{e}");
}

#[test]
fn the_portable_vm_refuses_what_it_cant_run_before_running() {
    let mut m = point_module();
    m.kinds[0].resolution = crush_cosm::Resolution::EmbeddingPromotion;
    let mut vm = crate::portable_vm::PortableVm::new(program_with(Some(m)));
    let e = vm.run().unwrap_err().to_string();
    assert!(e.contains("COSM strategies not supported on this VM yet"), "{e}");
    assert!(e.contains("resolution: embedding_promotion"), "{e}");
    // Refused before the first instruction: nothing was printed.
    assert_eq!(vm.take_output(), "");
    // A value Kind runs.
    let r = crate::portable_vm::PortableVm::new(program_with(Some(value_semantics_module()))).run().expect("runs");
    assert_eq!(r.output, "hi");
}

#[test]
fn fastvm_applies_the_same_rule_to_a_cvm1_kind_table() {
    // A map-equivalent kind table lowers as before.
    assert!(crate::fastvm::lower_bytecode_program(&program_with(Some(point_module()))).is_ok());
    // One a map can't represent is refused, like on the VMs.
    let Err(e) = crate::fastvm::lower_bytecode_program(&program_with(Some(value_semantics_module()))) else {
        panic!("a value(move) Kind must be refused");
    };
    let e = e.to_string();
    assert!(
        e.starts_with("COSM objects aren't supported on FastVM or the JIT yet (CRUSH-zorro-43)"),
        "{e}"
    );
    assert!(e.contains("`Point` declares semantics: value(move)"), "{e}");
}
