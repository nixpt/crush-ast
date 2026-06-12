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
fn runtime_error_location_is_formatted_with_source() {
    let mut info = DebugInfo::new();
    info.push_source_location(SourceLocation::new(42, 10, Some("main.crush".to_string())));

    let msg = casm::format_runtime_error_with_location("division by zero", Some(&info), 0);
    assert_eq!(msg, "Error at line 42, col 10: division by zero");
}

#[test]
fn runtime_error_location_falls_back_without_source() {
    let msg = casm::format_runtime_error_with_location("division by zero", None, 0);
    assert_eq!(msg, "division by zero");
}
