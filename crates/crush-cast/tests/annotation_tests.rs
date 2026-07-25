//! CRUSH-27 roundtrip tests for the flat `Annotation` enum, its payload
//! structs (`ErrorAnnotation`, `ReadAnnotation`, `WriteAnnotation`,
//! `CoverageAnnotation`), and `Program::flatten_annotations` so downstream
//! consumers (CRUSH-28's `crush-index`) can iterate annotations uniformly.

use crush_cast::{
    Annotation, CoverageAnnotation, ErrorAnnotation, ErrorLikelihood, ExhaustiveMatchSite,
    Function, FunctionAnnotations, Invariant, ModuleManifest, Program, ReadAnnotation,
    SourceLoc, WeightedError, WriteAnnotation,
};

// ─── Roundtrip tests (Annotation enum + payload struct variants) ────────

#[test]
fn roundtrip_annotation_module() {
    let ann = Annotation::Module(ModuleManifest {
        purpose: "do things".to_string(),
        exports: vec!["foo".to_string()],
        ..Default::default()
    });
    let json = serde_json::to_string(&ann).expect("serialize");
    let back: Annotation = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(ann, back);
    assert!(json.contains("\"kind\":\"Module\""), "tag must be Module");
}

#[test]
fn roundtrip_annotation_invariant() {
    let ann = Annotation::Invariant(Invariant {
        name: "x-positive".to_string(),
        description: "x must remain positive".to_string(),
        applies_to: vec!["compute".to_string()],
        ..Default::default()
    });
    let json = serde_json::to_string(&ann).unwrap();
    let back: Annotation = serde_json::from_str(&json).unwrap();
    assert_eq!(ann, back);
}

#[test]
fn roundtrip_annotation_error() {
    let ann = Annotation::Error(ErrorAnnotation {
        function_name: "vm_step".to_string(),
        variants: vec!["StackUnderflow".to_string()],
        variants_weighted: vec![WeightedError {
            variant: "NetworkTimeout".to_string(),
            likelihood: ErrorLikelihood::Likely,
        }],
    });
    let json = serde_json::to_string(&ann).unwrap();
    let back: Annotation = serde_json::from_str(&json).unwrap();
    assert_eq!(ann, back);
}

#[test]
fn roundtrip_annotation_read() {
    let ann = Annotation::Read(ReadAnnotation {
        function_name: "vm_step".to_string(),
        paths: vec!["thread.ip".to_string(), "thread.stack".to_string()],
    });
    let json = serde_json::to_string(&ann).unwrap();
    let back: Annotation = serde_json::from_str(&json).unwrap();
    assert_eq!(ann, back);
}

#[test]
fn roundtrip_annotation_write() {
    let ann = Annotation::Write(WriteAnnotation {
        function_name: "vm_step".to_string(),
        paths: vec!["thread.out_parts".to_string()],
    });
    let json = serde_json::to_string(&ann).unwrap();
    let back: Annotation = serde_json::from_str(&json).unwrap();
    assert_eq!(ann, back);
}

#[test]
fn roundtrip_annotation_coverage() {
    let ann = Annotation::Coverage(CoverageAnnotation {
        function_name: "test_stack_underflow".to_string(),
        paths: vec!["VmError::StackUnderflow".to_string()],
    });
    let json = serde_json::to_string(&ann).unwrap();
    let back: Annotation = serde_json::from_str(&json).unwrap();
    assert_eq!(ann, back);
}

#[test]
fn roundtrip_annotation_exhaustive_match_sites() {
    let ann = Annotation::ExhaustiveMatchSites(ExhaustiveMatchSite {
        type_name: "Value".to_string(),
        function_name: "step_one".to_string(),
        location: SourceLoc {
            file: "vm.rs".to_string(),
            line: 42,
            col: 5,
        },
        covered_arms: vec!["Int".to_string()],
        missing_arms: vec!["Float".to_string()],
        has_wildcard: false,
    });
    let json = serde_json::to_string(&ann).unwrap();
    let back: Annotation = serde_json::from_str(&json).unwrap();
    assert_eq!(ann, back);
}

#[test]
fn roundtrip_serde_tag_is_kind_content_node() {
    let ann = Annotation::Module(ModuleManifest {
        purpose: "x".to_string(),
        ..Default::default()
    });
    let json = serde_json::to_string(&ann).unwrap();
    // Verify `#[serde(tag = "kind", content = "node")]` — stable names
    // that downstream consumers (crush-index / codebase.* caps) depend on.
    assert!(
        json.contains("\"kind\""),
        "Annotation should serialize with kind tag, got {json}"
    );
    assert!(
        json.contains("\"node\""),
        "Annotation should serialize content under `node`, got {json}"
    );
}

// ─── flatten_annotations tests ──────────────────────────────────────────

#[test]
fn flatten_empty_program_yields_zero_annotations() {
    let p = Program::default();
    assert_eq!(p.flatten_annotations().len(), 0);
}

#[test]
fn flatten_module_only_emits_just_module() {
    let mut p = Program::default();
    p.manifest = Some(ModuleManifest {
        purpose: "compute".to_string(),
        exports: vec!["run".to_string()],
        ..Default::default()
    });
    let flat = p.flatten_annotations();
    assert_eq!(flat.len(), 1);
    assert!(matches!(flat[0], Annotation::Module(_)));
}

#[test]
fn flatten_module_with_invariants_emits_module_and_each_invariant() {
    let mut p = Program::default();
    p.manifest = Some(ModuleManifest {
        purpose: "compute".to_string(),
        invariants: vec![
            Invariant {
                name: "x-positive".to_string(),
                description: "x remains positive".to_string(),
                ..Default::default()
            },
            Invariant {
                name: "result-finite".to_string(),
                description: "result is not nan".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    let flat = p.flatten_annotations();
    // 1 Module + 2 Invariant = 3 entries
    assert_eq!(flat.len(), 3);
    assert!(matches!(flat[0], Annotation::Module(_)));
    assert!(matches!(flat[1], Annotation::Invariant(_)));
    assert!(matches!(flat[2], Annotation::Invariant(_)));
}

#[test]
fn flatten_top_level_invariant_without_module_still_emits_invariant() {
    // CRUSH-27 doc-comment assertion: parser synthesizes a default
    // manifest-as-container for any top-level `@invariant` blocks even
    // when no `@module` is present. flatten_annotations must surface those
    // as standalone Invariant variants.
    let mut p = Program::default();
    p.manifest = Some(ModuleManifest {
        purpose: String::new(),
        invariants: vec![Invariant {
            name: "no-module-but-this-invariant-still-surfaces".to_string(),
            description: "x".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    });
    let flat = p.flatten_annotations();
    let invariants: Vec<&Invariant> = flat
        .iter()
        .filter_map(|a| match a {
            Annotation::Invariant(inv) => Some(inv),
            _ => None,
        })
        .collect();
    assert_eq!(invariants.len(), 1);
    assert_eq!(
        invariants[0].name,
        "no-module-but-this-invariant-still-surfaces"
    );
}

#[test]
fn flatten_function_with_errors_emits_error_with_function_name() {
    let mut p = Program::default();
    let ann = FunctionAnnotations {
        errors: vec!["VmError::StackUnderflow".to_string()],
        errors_weighted: vec![WeightedError {
            variant: "NetworkTimeout".to_string(),
            likelihood: ErrorLikelihood::Likely,
        }],
        ..Default::default()
    };
    p.functions.insert(
        "vm_step".to_string(),
        Function {
            annotations: Some(ann),
            ..Default::default()
        },
    );
    let flat = p.flatten_annotations();
    assert_eq!(flat.len(), 1);
    match &flat[0] {
        Annotation::Error(e) => {
            assert_eq!(e.function_name, "vm_step");
            assert_eq!(e.variants, vec!["VmError::StackUnderflow"]);
            assert_eq!(e.variants_weighted.len(), 1);
        }
        other => panic!("expected Annotation::Error, got {other:?}"),
    }
}

#[test]
fn flatten_function_with_reads_writes_covers_emits_all_three() {
    let mut p = Program::default();
    let ann = FunctionAnnotations {
        reads: vec!["thread.ip".to_string()],
        writes: vec!["thread.out_parts".to_string()],
        covers: vec!["VmError::StackUnderflow".to_string()],
        ..Default::default()
    };
    p.functions.insert(
        "vm_step".to_string(),
        Function {
            annotations: Some(ann),
            ..Default::default()
        },
    );
    let flat = p.flatten_annotations();
    assert_eq!(flat.len(), 3);
    let mut seen_read = false;
    let mut seen_write = false;
    let mut seen_cov = false;
    for a in &flat {
        match a {
            Annotation::Read(r) => {
                assert_eq!(r.function_name, "vm_step");
                seen_read = true;
            }
            Annotation::Write(w) => {
                assert_eq!(w.function_name, "vm_step");
                seen_write = true;
            }
            Annotation::Coverage(c) => {
                assert_eq!(c.function_name, "vm_step");
                seen_cov = true;
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }
    assert!(seen_read && seen_write && seen_cov);
}

#[test]
fn flatten_function_with_no_annotations_emits_nothing() {
    let mut p = Program::default();
    p.functions
        .insert("no_ann_fn".to_string(), Function::default());
    assert_eq!(p.flatten_annotations().len(), 0);
}

#[test]
fn flatten_preserves_exhaustive_match_sites() {
    let mut p = Program::default();
    p.exhaustive_sites.push(ExhaustiveMatchSite {
        type_name: "Value".to_string(),
        function_name: "step_one".to_string(),
        location: SourceLoc {
            file: "vm.rs".to_string(),
            line: 42,
            col: 5,
        },
        covered_arms: vec!["Int".to_string()],
        missing_arms: vec![],
        has_wildcard: false,
    });
    let flat = p.flatten_annotations();
    assert_eq!(flat.len(), 1);
    match &flat[0] {
        Annotation::ExhaustiveMatchSites(s) => assert_eq!(s.type_name, "Value"),
        other => panic!("expected ExhaustiveMatchSites variant, got {other:?}"),
    }
}

#[test]
fn flatten_multiple_functions_each_get_their_function_name() {
    let mut p = Program::default();
    p.functions.insert(
        "fn_a".to_string(),
        Function {
            annotations: Some(FunctionAnnotations {
                reads: vec!["a".to_string()],
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    p.functions.insert(
        "fn_b".to_string(),
        Function {
            annotations: Some(FunctionAnnotations {
                reads: vec!["b".to_string()],
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    let flat = p.flatten_annotations();
    assert_eq!(flat.len(), 2);
    let names: Vec<String> = flat
        .iter()
        .map(|a| match a {
            Annotation::Read(r) => r.function_name.clone(),
            other => panic!("unexpected: {other:?}"),
        })
        .collect();
    assert!(names.contains(&"fn_a".to_string()));
    assert!(names.contains(&"fn_b".to_string()));
}
