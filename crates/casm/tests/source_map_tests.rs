use casm::debug_info::{DebugInfo, SourceLocation};

#[test]
fn debug_info_source_location_lookup_by_pc() {
    let mut info = DebugInfo::new();
    info.push_source_location(SourceLocation::new(1, 1, Some("main.crush".to_string())));
    info.push_source_location(SourceLocation::new(2, 4, Some("main.crush".to_string())));

    let loc0 = info.source_location_for_pc(0).expect("pc 0 should exist");
    assert_eq!(loc0.line, 1);
    assert_eq!(loc0.col, 1);

    let loc1 = info.source_location_for_pc(1).expect("pc 1 should exist");
    assert_eq!(loc1.line, 2);
    assert_eq!(loc1.col, 4);
    assert!(info.source_location_for_pc(2).is_none());
}

#[test]
fn source_location_for_function_pc_resolves_correctly() {
    let mut info = DebugInfo::new();

    // Function "main": 2 instructions at lines 1, 2
    info.push_source_location(SourceLocation::new(1, 1, Some("main.crush".to_string())));
    info.push_source_location(SourceLocation::new(2, 1, Some("main.crush".to_string())));
    info.record_function_range("main", 0, 2);

    // Function "helper": 2 instructions at lines 10, 11
    info.push_source_location(SourceLocation::new(10, 5, Some("main.crush".to_string())));
    info.push_source_location(SourceLocation::new(11, 5, Some("main.crush".to_string())));
    info.record_function_range("helper", 2, 4);

    // Flat lookup: pc=0 → "main" line 1
    assert_eq!(info.source_location_for_pc(0).unwrap().line, 1);
    // Flat lookup: pc=2 → "helper" line 10 (but this would need global pc knowledge)
    assert_eq!(info.source_location_for_pc(2).unwrap().line, 10);

    // Function-aware lookup for "main"
    let loc = info.source_location_for_function_pc("main", 0).unwrap();
    assert_eq!(loc.line, 1);
    let loc = info.source_location_for_function_pc("main", 1).unwrap();
    assert_eq!(loc.line, 2);

    // Function-aware lookup for "helper"
    let loc = info.source_location_for_function_pc("helper", 0).unwrap();
    assert_eq!(loc.line, 10);
    let loc = info.source_location_for_function_pc("helper", 1).unwrap();
    assert_eq!(loc.line, 11);

    // Out of bounds
    assert!(info.source_location_for_function_pc("main", 2).is_none());
    assert!(info.source_location_for_function_pc("helper", 2).is_none());
    // Unknown function falls back to flat lookup
    assert_eq!(info.source_location_for_function_pc("unknown", 0).unwrap().line, 1);
}

#[test]
fn runtime_error_location_is_formatted_with_source() {
    let mut info = DebugInfo::new();
    info.push_source_location(SourceLocation::new(42, 10, Some("main.crush".to_string())));

    let msg = casm::format_runtime_error_with_location("division by zero", Some(&info), 0, None);
    assert_eq!(msg, "Error at line 42, col 10: division by zero");
}

#[test]
fn runtime_error_location_falls_back_without_source() {
    let msg = casm::format_runtime_error_with_location("division by zero", None, 0, None);
    assert_eq!(msg, "division by zero");
}
