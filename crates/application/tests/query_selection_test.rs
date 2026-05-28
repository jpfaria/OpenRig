//! Phase 1 part 2 red-first test (issue #548):
//! `QueryKind::Selection` exists so MCP/gRPC can read the GUI selection
//! through the same query bus the GUI uses (read-side parity per
//! `openrig-code-quality` SKILL law).

use application::bridge::QueryKind;

#[test]
fn query_kind_has_selection_variant() {
    let kind: QueryKind = QueryKind::Selection;
    // matches the variant so an accidental rename of `Selection` breaks
    // this test instead of silently disappearing.
    match kind {
        QueryKind::Selection => {}
        _ => panic!("expected QueryKind::Selection"),
    }
}
