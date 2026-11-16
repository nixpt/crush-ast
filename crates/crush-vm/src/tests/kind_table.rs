//! COSM K1.3 (CRUSH-zorro-42): the CVM1 kind-table section — versioning,
//! old blobs, round-trips, the disassembler — and what the scheduler VM,
//! the portable VM and FastVM's CVM1 lowering do with a kind table (since
//! K1.4, CRUSH-zorro-43, the VMs run Kinds: see `records.rs`).

use crate::assembler::{assemble, disassemble};
use crate::bytecode::{
    KIND_TABLE_FORMAT_JSON, KIND_TABLE_FORMAT_JSON_VERSIONED, MAGIC, Program, VERSION_KIND_TABLE,
    VERSION_NO_KIND_TABLE,
};
use crate::vm::{Quotas, Value, VmError, run};
use crush_cosm::{CosmModule, Semantics, ValueMode};

/// The Point/Shape fixture from crush-cosm (COSM K1.1).
const POINT_FIXTURE: &str =
    include_str!("../../../crush-cosm/tests/fixtures/crush_point.cosm.json");

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
    let code = [
        crate::bytecode::PUSH,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        42,
        crate::bytecode::HALT,
    ];
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
    assert!(matches!(
        Program::from_blob(&truncated),
        Err(crate::Error::Truncated)
    ));
    let cut = &good[..good.len() - 1];
    assert!(matches!(
        Program::from_blob(cut),
        Err(crate::Error::Truncated)
    ));

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

/// A v3 blob whose kind-table section is `format | len | json`, laid out by hand.
fn blob_with_section(format: u8, json: &str) -> Vec<u8> {
    let mut b = program_with(None).to_blob();
    b[4] = VERSION_KIND_TABLE;
    b.push(format);
    b.extend_from_slice(&(json.len() as u32).to_be_bytes());
    b.extend_from_slice(json.as_bytes());
    b
}

#[test]
fn kt_format_2_loads_and_today_s_tables_are_still_written_as_format_1() {
    // V3a (#521): format 2 is the stamped-COSM section a newer writer emits
    // once a module needs COSM above the base version; this reader takes it.
    let mut v: serde_json::Value = serde_json::from_str(POINT_FIXTURE).unwrap();
    v["cosm_version"] = serde_json::json!(crush_cosm::COSM_BASE_VERSION);
    let p = Program::from_blob(&blob_with_section(
        KIND_TABLE_FORMAT_JSON_VERSIONED,
        &v.to_string(),
    ))
    .expect("kt_format 2 loads");
    assert_eq!(p.cosm, Some(point_module()));

    // Nothing needs a newer COSM yet, so the writer keeps format 1.
    let good = program_with(Some(point_module())).to_blob();
    let v2_len = program_with(None).to_blob().len();
    assert_eq!(good[v2_len], KIND_TABLE_FORMAT_JSON);
}

#[test]
fn a_kind_table_needing_a_newer_cosm_is_refused_at_load() {
    let mut v: serde_json::Value = serde_json::from_str(POINT_FIXTURE).unwrap();
    v["cosm_version"] = serde_json::json!("0.9.0");
    for format in [KIND_TABLE_FORMAT_JSON, KIND_TABLE_FORMAT_JSON_VERSIONED] {
        let e = Program::from_blob(&blob_with_section(format, &v.to_string())).unwrap_err();
        assert!(matches!(e, crate::Error::BadKindTable(_)), "{e:?}");
        let msg = e.to_string();
        assert!(msg.contains("needs COSM 0.9.0"), "{msg}");
    }
}

#[test]
fn a_cvm1_blob_with_a_private_method_is_refused_at_load() {
    // #521: what a visibility-aware writer would emit (stamped, format 2,
    // a `priv` member) is refused by this runtime, which predates visibility.
    let mut v: serde_json::Value = serde_json::from_str(POINT_FIXTURE).unwrap();
    v["kinds"][0]["members"][1]["visibility"] = serde_json::json!("priv");
    v["cosm_version"] = serde_json::json!("0.3.0");
    let e = Program::from_blob(&blob_with_section(
        KIND_TABLE_FORMAT_JSON_VERSIONED,
        &v.to_string(),
    ))
    .unwrap_err();
    assert!(matches!(e, crate::Error::BadKindTable(_)), "{e:?}");
    assert!(e.to_string().contains("needs COSM 0.3.0"), "{e}");

    // Unstamped, in format 1, the member key check still refuses it.
    v.as_object_mut().unwrap().remove("cosm_version");
    let e =
        Program::from_blob(&blob_with_section(KIND_TABLE_FORMAT_JSON, &v.to_string())).unwrap_err();
    assert!(e.to_string().contains("unknown key `visibility`"), "{e}");
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
    assert!(
        text.starts_with("; kind table (COSM): 1 Kind, 1 interface\n"),
        "{text}"
    );
    assert!(
        text.contains(";   kind Point (crush): fields [x, y], members [new, norm2]"),
        "{text}"
    );
    assert!(
        text.contains(";   interface Shape (crush): [area, describe]"),
        "{text}"
    );
    assert!(text.contains("\n.cosm {"), "{text}");
    let back = assemble(&text, Some(&["io.print"]), None).expect("reassembles");
    assert_eq!(back.cosm, Some(point_module()));
    assert_eq!(back.code, p.code);
    // No kind table, no header.
    assert!(!disassemble(&program_with(None)).contains("kind table"));
}

#[test]
fn the_assembler_rejects_a_bad_or_duplicate_cosm_directive() {
    let e = assemble(".cosm {nope}\nHALT\n", None, None)
        .unwrap_err()
        .to_string();
    assert!(e.contains("malformed .cosm kind table"), "{e}");
    let line = format!(".cosm {}\n", point_module().to_json());
    let e = assemble(&format!("{line}{line}HALT\n"), None, None)
        .unwrap_err()
        .to_string();
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

    let portable = |m| {
        crate::portable_vm::PortableVm::new(program_with(m))
            .run()
            .expect("portable VM runs it")
    };
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
    let r = run(
        &program_with(Some(value_semantics_module())),
        &Quotas::default(),
    )
    .expect("runs");
    assert_eq!(r.return_value, Some(Value::Int(7)));

    let mut m = point_module();
    m.kinds[0].resolution = crush_cosm::Resolution::PrototypeChain;
    m.kinds[0].hooks.eq = Some(crush_cosm::HookImpl::Fn(crush_cosm::FnRef::new(
        "Point::eq",
    )));
    m.kinds[0].hooks.str = Some(crush_cosm::HookImpl::Fn(crush_cosm::FnRef::new(
        "Point::show",
    )));
    m.kinds[0].members.push(crush_cosm::MemberDecl::accessor(
        "len",
        Some("Point::len".into()),
        None,
    ));
    let e = run(&program_with(Some(m)), &Quotas::default()).unwrap_err();
    assert!(matches!(e, VmError::Unsupported(_)), "{e:?}");
    let e = e.to_string();
    assert!(
        e.contains("COSM strategies not supported on this VM yet: `Point` declares"),
        "{e}"
    );
    {
        let want = "hooks eq";
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
    assert!(
        e.contains("COSM strategies not supported on this VM yet"),
        "{e}"
    );
    assert!(e.contains("resolution: embedding_promotion"), "{e}");
    // Refused before the first instruction: nothing was printed.
    assert_eq!(vm.take_output(), "");
    // A value Kind runs.
    let r = crate::portable_vm::PortableVm::new(program_with(Some(value_semantics_module())))
        .run()
        .expect("runs");
    assert_eq!(r.output, "hi");
}

#[test]
fn fastvm_applies_the_same_rule_to_a_cvm1_kind_table() {
    // A map-equivalent kind table lowers as before.
    assert!(crate::fastvm::lower_bytecode_program(&program_with(Some(point_module()))).is_ok());
    // One a map can't represent is refused, like on the VMs.
    let Err(e) =
        crate::fastvm::lower_bytecode_program(&program_with(Some(value_semantics_module())))
    else {
        panic!("a value(move) Kind must be refused");
    };
    let e = e.to_string();
    assert!(
        e.starts_with("COSM objects aren't supported on FastVM or the JIT yet (CRUSH-zorro-43)"),
        "{e}"
    );
    assert!(e.contains("`Point` declares semantics: value(move)"), "{e}");
}

// ── owner ids (V3b, #522) ────────────────────────────────────────────────────

/// The fixture's canonical COSM JSON as written before owner ids, byte for
/// byte (crush-cosm's `tests/owners.rs` pins the same string).
const POINT_GOLDEN: &str = r#"{"kinds":[{"name":"Point","home_lang":"crush","span":{"file":"example","line":1,"col":1},"storage":{"sealed":{"fields":[{"name":"x","type":"Int"},{"name":"y","type":"Int"}]}},"semantics":"reference","resolution":"none","receiver":"explicit_ref","construct":"literal","members":[{"name":"new","span":{"line":3,"col":5},"kind":"static","value":{"fn":"Point::new"}},{"name":"norm2","span":{"line":4,"col":5},"kind":"method","fn":"Point::norm2"}],"conforms":[{"interface":"Shape","check":"nominal","members":[{"name":"area","kind":"method","fn":"<Point as Shape>::area"}],"span":{"file":"example","line":7,"col":1}}],"equality":"structural"}],"interfaces":[{"name":"Shape","home_lang":"crush","span":{"file":"example","line":6,"col":1},"members":[{"name":"area","receiver":"explicit_ref","returns":"Any"},{"name":"describe","receiver":"explicit_ref","returns":"Any","default":"Shape::describe"}],"satisfaction":"nominal"}]}"#;

/// `main` calls `helper`, which another package owns.
const TWO_FN_SRC: &str = "\
.func main
    PUSH 1
    CALL helper
    HALT
.func helper
    PUSH 2
    MUL
    RET
";

fn owned_point() -> CosmModule {
    let mut m = point_module();
    m.kinds[0].module = Some("geo".into());
    m.kinds[0].package = Some("pkg:geom".into());
    m.fn_packages
        .insert(crush_cosm::FnRef::new("helper"), "pkg:geom".into());
    m
}

fn two_fn_program(module: Option<CosmModule>) -> Program {
    let mut p = assemble(TWO_FN_SRC, None, None).expect("assembly");
    p.cosm = module;
    p
}

fn owners_only() -> CosmModule {
    let mut m = CosmModule::new();
    m.fn_packages
        .insert(crush_cosm::FnRef::new("helper"), "dir:lib".into());
    m
}

#[test]
fn a_kind_table_without_owners_is_written_byte_for_byte_as_before() {
    // The section as every writer before V3b laid it out: format 1, the
    // golden JSON.
    let expected = blob_with_section(KIND_TABLE_FORMAT_JSON, POINT_GOLDEN);
    assert_eq!(program_with(Some(point_module())).to_blob(), expected);
}

#[test]
fn owner_ids_are_written_as_kt_format_2_and_round_trip() {
    let p = two_fn_program(Some(owned_point()));
    let blob = p.to_blob();
    assert_eq!(blob[4], VERSION_KIND_TABLE);
    let v2_len = two_fn_program(None).to_blob().len();
    assert_eq!(blob[v2_len], KIND_TABLE_FORMAT_JSON_VERSIONED);
    let json = std::str::from_utf8(&blob[v2_len + 5..]).unwrap();
    assert!(json.starts_with(r#"{"cosm_version":"0.2.0","#), "{json}");
    let back = Program::from_blob(&blob).unwrap();
    assert_eq!(back.cosm, Some(owned_point()));
}

#[test]
fn owners_without_kinds_still_get_a_section() {
    // A program of several packages that declares no Kind: its owners must
    // not be dropped because there is no Kind to carry them.
    let p = two_fn_program(Some(owners_only()));
    assert!(p.kind_table().is_none());
    let blob = p.to_blob();
    assert_eq!(blob[4], VERSION_KIND_TABLE);
    let back = Program::from_blob(&blob).unwrap();
    assert_eq!(back.cosm, Some(owners_only()));
    // An empty module is still no section at all.
    assert_eq!(
        two_fn_program(Some(CosmModule::new())).to_blob(),
        two_fn_program(None).to_blob()
    );
}

#[test]
fn a_reader_before_owner_ids_refuses_the_blob_by_name() {
    // A COSM 0.1.0 runtime (V3a) reads format 2 and gates on the stamp; the
    // same gate at its version names what it can't read. A runtime from
    // before V3a refuses format 2 itself (`unknown kind-table format 2`).
    let blob = two_fn_program(Some(owned_point())).to_blob();
    let v2_len = two_fn_program(None).to_blob().len();
    let json: serde_json::Value = serde_json::from_slice(&blob[v2_len + 5..]).unwrap();
    let err = crush_cosm::check_readable_by(
        json["cosm_version"].as_str().unwrap(),
        crush_cosm::COSM_BASE_VERSION,
    )
    .unwrap_err();
    assert!(err.contains("needs COSM 0.2.0"), "{err}");
}

#[test]
fn the_disassembler_keeps_owners_only_modules() {
    let p = two_fn_program(Some(owners_only()));
    let text = disassemble(&p);
    assert!(text.contains("functions outside the root package: 1"), "{text}");
    let back = assemble(&text, None, None).expect("reassembles");
    assert_eq!(back.cosm, Some(owners_only()));
}

#[test]
fn the_portable_vm_reads_the_executing_function_s_package() {
    for module in [owned_point(), owners_only()] {
        let want = module.function_package("helper").map(str::to_string);
        let mut vm = crate::portable_vm::PortableVm::new(two_fn_program(Some(module)));
        assert_eq!(vm.function_package("helper"), want.as_deref());
        assert_eq!(vm.function_package("main"), None, "root package");
        let mut seen: Vec<(String, Option<String>)> = Vec::new();
        while !vm.is_halted() {
            let f = vm.function_at(vm.current_ip()).unwrap_or("?").to_string();
            let pkg = vm.current_function_package().map(str::to_string);
            if seen.last().map(|(g, _)| g != &f).unwrap_or(true) {
                seen.push((f, pkg));
            }
            vm.step().expect("steps");
        }
        assert_eq!(
            seen,
            vec![
                ("main".to_string(), None),
                ("helper".to_string(), want.clone()),
                ("main".to_string(), None),
            ]
        );
    }
    let vm = crate::portable_vm::PortableVm::new(two_fn_program(Some(owned_point())));
    let point = vm.kind("Point").expect("Point is loaded");
    assert_eq!(point.module(), Some("geo"));
    assert_eq!(point.package(), Some("pkg:geom"));
}

#[test]
fn the_scheduler_vm_loads_owners_and_runs_as_before() {
    for module in [owned_point(), owners_only()] {
        let p = two_fn_program(Some(module.clone()));
        // The table the scheduler builds at load (`run_from`).
        let kinds = crate::cosm_rt::KindTable::load(&p).expect("loads");
        assert_eq!(
            kinds.function_package("helper"),
            module.function_package("helper")
        );
        assert_eq!(kinds.function_package("main"), None);
        let entries: Vec<(&str, usize)> = p
            .manifest
            .functions
            .iter()
            .map(|(n, f)| (n.as_str(), f.entry))
            .collect();
        let helper = entries.iter().find(|(n, _)| *n == "helper").unwrap().1;
        assert_eq!(
            kinds.package_at(entries.iter().copied(), helper + 1),
            module.function_package("helper")
        );
        assert_eq!(kinds.package_at(entries.iter().copied(), 0), None);

        let with = run(&p, &Quotas::default()).expect("runs");
        let plain = run(&two_fn_program(None), &Quotas::default()).unwrap();
        assert_eq!(with.stack, plain.stack);
        assert_eq!(with.stack, vec![Value::Int(2)]);
    }
    let kinds = crate::cosm_rt::KindTable::load(&two_fn_program(Some(owned_point()))).unwrap();
    let point = kinds.get("Point").unwrap();
    assert_eq!((point.module(), point.package()), (Some("geo"), Some("pkg:geom")));
}

#[test]
fn fastvm_runs_owner_ids_as_before() {
    // Nothing enforces owners yet (V6, #525), so the map engines lower a
    // program with them exactly as without: the shared rule doesn't look at
    // them.
    for module in [owned_point(), owners_only()] {
        assert!(
            crate::fastvm::lower_bytecode_program(&two_fn_program(Some(module))).is_ok()
        );
    }
}
