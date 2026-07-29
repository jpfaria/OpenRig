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

/// #819: the inline editor must also receive the #500 *inner* knob-grid
/// dimensions, not just the outer panel height. `BlockPanelEditor` lays out its
/// knob grid from `panel-grid-cols` / `panel-grid-rows` / `panel-knob-inner-height`
/// (Slint never re-derives the wrap math); the detached window feeds them via
/// `apply_panel_dimensions`. Without them the inline grid keeps its 225px /
/// 0-column defaults, so a param-heavy block (e.g. the Dragonfly reverb) clips
/// its lower knobs even though the outer container is tall enough.
#[test]
fn inline_panel_editor_receives_the_500_inner_grid_dimensions() {
    let page = include_str!("../ui/pages/project_chains.slint");
    for binding in [
        "panel-grid-cols:",
        "panel-grid-rows:",
        "panel-knob-inner-height:",
    ] {
        assert!(
            page.contains(binding),
            "the inline BlockPanelEditor must bind `{binding}` from the #500 policy, \
             or its knob grid keeps the default layout and clips a param-heavy block"
        );
    }

    let global = include_str!("../ui/components/block_param_tabs_globals.slint");
    for prop in ["grid-cols", "grid-rows", "inner-height"] {
        assert!(
            global.contains(prop),
            "BlockParamTabs must carry `{prop}` so the inline editor can read the \
             #500-computed knob-grid dimensions"
        );
    }
}
