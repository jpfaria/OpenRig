//! #819 — the INLINE (fullscreen/touch) block editor must show the same #780
//! parameter tabs as the detached editor.
//!
//! Intended behaviour: `BlockPanelEditor` gates its tab bar on
//! `block-parameter-groups.length > 1`. The detached editor gets that model
//! filled by `apply_param_tabs`; the inline instance embedded in
//! `project_chains.slint` must be fed the same three tab properties, otherwise
//! a multi-group block (e.g. the 8-band parametric EQ) renders as one flat,
//! overflowing list with no tabs — which is what fullscreen shows today.

/// The inline `BlockPanelEditor` must forward the tab model, the active tab and
/// the tab-select callback — without them the tab bar can never appear.
#[test]
fn inline_panel_editor_forwards_the_parameter_tab_properties() {
    let src = include_str!("../ui/pages/project_chains.slint");
    for binding in [
        "block-parameter-groups:",
        "active-parameter-group:",
        "select-parameter-group",
    ] {
        assert!(
            src.contains(binding),
            "the inline BlockPanelEditor must forward `{binding}` so a multi-group \
             block shows its parameter tabs (same as the detached editor)"
        );
    }
}
