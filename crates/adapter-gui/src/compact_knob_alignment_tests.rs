//! #915 — in the compact view a parameter's label is centred on its cell while
//! the knob under it was pinned to `x: 2px`, so every label sat ~8px to the
//! right of the knob it names. The offset is pure `.slint` geometry, which no
//! Rust value observes, so it is pinned by source the way the other layout
//! rules in this crate are: a widget centred under a centred label must derive
//! its `x` from the cell width, never from a fixed left inset.

/// The widgets that sit under the centred label of a parameter cell, and the
/// property that must centre them.
fn assert_centred(source: &str, file: &str, widget: &str) {
    let mut seen = 0;
    for (i, line) in source.lines().enumerate() {
        if !line.contains(&format!("{widget} {{")) && !line.contains(&format!(": {widget} {{")) {
            continue;
        }
        // The geometry line follows the opening brace.
        let geometry = source.lines().skip(i + 1).take(3).collect::<String>();
        assert!(
            geometry.contains("(parent.width -") || geometry.contains("parent.cx -"),
            "{file}: this {widget} is placed at a fixed inset ({}), so it does not sit under \
             the centre of its label — centre it on the cell (#915)",
            geometry.trim()
        );
        seen += 1;
    }
    assert!(seen > 0, "{file}: found no {widget} to check");
}

#[test]
fn a_compact_parameter_knob_sits_under_its_label() {
    assert_centred(
        include_str!("../ui/pages/compact_block_param_cell.slint"),
        "compact_block_param_cell.slint",
        "PanelKnob",
    );
}

#[test]
fn a_compact_curated_knob_sits_under_its_label() {
    assert_centred(
        include_str!("../ui/pages/compact_block_row.slint"),
        "compact_block_row.slint",
        "PanelKnob",
    );
}

#[test]
fn a_block_editor_knob_sits_under_its_label() {
    // The editor grid already centres its knobs — pinned so the two views do
    // not drift apart again.
    assert_centred(
        include_str!("../ui/components/block_param_grid.slint"),
        "block_param_grid.slint",
        "PanelKnob",
    );
    assert_centred(
        include_str!("../ui/components/block_panel_parameter_item.slint"),
        "block_panel_parameter_item.slint",
        "PanelKnob",
    );
}
