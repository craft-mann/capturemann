//! The kind names are listed by hand in the MCP tool schema (cm-mcp does not depend on
//! cm-render); this keeps that list equal to `RedactionKind::ALL`.

use cm_render::ocr::RedactionKind;

#[test]
fn mcp_schema_lists_every_redaction_kind() {
    let src = include_str!("../../cm-mcp/src/tools.rs");
    for name in RedactionKind::names() {
        assert!(
            src.contains(&format!("\"{name}\"")),
            "{name} is missing from the suggest_redactions schema in cm-mcp"
        );
    }
}

#[test]
fn names_round_trip_and_default_is_a_subset_of_all() {
    for k in RedactionKind::ALL {
        assert_eq!(RedactionKind::parse(k.as_str()), Some(k));
        assert!(
            !k.standards().is_empty(),
            "{} names no standard",
            k.as_str()
        );
    }
    for k in RedactionKind::DEFAULT {
        assert!(RedactionKind::ALL.contains(&k));
    }
}
