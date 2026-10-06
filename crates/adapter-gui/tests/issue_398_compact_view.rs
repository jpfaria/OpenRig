//! #398 — the compact view: a dark chain bar, then a rack of module cards.
//! Each card has a side column (a category strip with the power button, the
//! brand and the model field, the parameter tabs) and its knobs, drawn as
//! vector knobs whose value arc takes the category colour.

use std::path::PathBuf;

fn ui(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn the_knobs_are_drawn_not_sprites() {
    let face = ui("components/knob_arc.slint");
    assert!(face.starts_with("// Responsibility: "));
    assert!(face.contains("ArcTo"), "the value arc is a vector path");
    assert!(face.contains("Theme.ktrack"));
    let knob = ui("components/panel_knob.slint");
    assert!(!knob.contains("knob-strip.png"), "the sprite knob is gone");
    assert!(knob.contains("in property <color> tint"));
    assert!(knob.contains("KnobArc {"));
    let selector = ui("components/selector_knob.slint");
    assert!(!selector.contains("selector-knob-strip.png"));
    assert!(selector.contains("KnobArc {"));
}

#[test]
fn the_compact_bar_is_dark_chrome() {
    let header = ui("pages/compact_chain_view_header.slint");
    assert!(header.contains("background: Theme.chrome;"));
    assert!(header.contains("Theme.chrome-line"));
    assert!(header.contains("ToggleSwitch {"));
    assert!(!header.contains("PowerSwitch {"));
    assert!(header.contains("on-chrome: true;"));
    let title = ui("components/chain_title_preset.slint");
    assert!(title.contains("in property <bool> on-chrome"));
    assert!(title.contains("Theme.chrome-field"));
    let select = ui("components/preset_select.slint");
    assert!(select.contains("in property <bool> on-chrome"));
    let volume = ui("components/chain_volume_button.slint");
    assert!(volume.contains("in property <bool> on-chrome"));
}

#[test]
fn a_block_is_a_module_card() {
    let row = ui("pages/compact_block_row.slint");
    assert!(row.contains("Theme.side-tint"), "the side column");
    assert!(row.contains("@linear-gradient(90deg"), "the category strip");
    assert!(row.contains("root.block-data.accent-color"));
    assert!(row.contains("Gear.off"), "a bypassed card greys its strip");
    assert!(
        !row.contains("FootSwitch {"),
        "the power button sits on the strip"
    );
    assert!(!row.contains("rack-texture.png"));
    assert!(row.contains("root.block-data.lane-bar"));
    assert!(row.contains("@tr(\"compact-lane-path\""));
    assert!(row.contains("@tr(\"compact-lane-mix\")"));
}

#[test]
fn the_parameter_tabs_are_underlined_text() {
    let tabs = ui("components/param_tab_bar.slint");
    assert!(!tabs.contains("Theme.palette.accent-end"));
    assert!(tabs.contains("Theme.ink-3"));
    assert!(tabs.contains("Theme.accent"));
}

#[test]
fn a_long_option_list_opens_from_a_chevron() {
    let cell = ui("pages/compact_block_param_cell.slint");
    assert!(!cell.contains("\\u{25BC}"), "no glyph as an icon");
    assert!(cell.contains("chevron-down.svg"));
    assert!(cell.contains("tint: root.tint;"));
}

#[test]
fn the_sections_are_a_quiet_accordion() {
    let toggle = ui("components/section_toggle.slint");
    assert!(
        toggle.contains("Theme.panel-hi"),
        "the header is a raised strip"
    );
    assert!(
        toggle.contains("Theme.hair-2"),
        "a hairline under the header"
    );
    assert!(
        !toggle.contains("arrow-right.svg"),
        "the chevron is drawn and turns"
    );
    assert!(toggle.contains("Path {"));
    let sections = ui("components/compact_chain_sections.slint");
    assert!(
        sections.contains("Theme.hair;"),
        "a hairline above each section"
    );
    assert!(!sections.contains("background: Theme.panel; }"));
}

#[test]
fn the_grip_sits_in_the_rack_gutter() {
    let view = ui("pages/compact_chain_view.slint");
    assert!(!view.contains("Theme.hover : Theme.hover"));
    assert!(view.contains("x: 14px + block-item.path-depth * 16px;"));
}
