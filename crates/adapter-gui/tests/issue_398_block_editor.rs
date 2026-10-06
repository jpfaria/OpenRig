//! #398 — the block editor: a dark model bar (the category in its colour, the
//! model field, info and remove), then a brand zone on the left (a category
//! stripe, the logo or the model word, a round power button) beside the
//! control area (tabs, then the knob grid).

use std::path::PathBuf;

fn ui(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn the_model_bar_is_dark_chrome() {
    let header = ui("components/block_panel_header.slint");
    assert!(header.contains("background: Theme.chrome;"));
    assert!(header.contains("Theme.chrome-line"));
    assert!(header.contains("in property <color> category"));
    assert!(
        header.contains("colorize: root.category;"),
        "the category icon takes its colour"
    );
    assert!(
        header.contains("on-chrome: true;"),
        "the model field sits on the chrome"
    );
    assert!(
        header.contains("Theme.chrome-2"),
        "the icon buttons light on hover"
    );
    assert!(!header.contains("Theme.panel-lo"));
    let select = ui("components/model_select_with_search.slint");
    assert!(select.contains("in property <bool> on-chrome"));
    assert!(select.contains("Theme.chrome-field"));
}

#[test]
fn the_brand_zone_sits_beside_the_controls() {
    let zone = ui("components/block_panel_brand_zone.slint");
    assert!(zone.starts_with("// Responsibility: "));
    assert!(zone.contains("background: Theme.panel-hi;"));
    assert!(zone.contains("height: 3px;"), "the category stripe");
    assert!(zone.contains("BrandLogo {"));
    assert!(
        zone.contains("font-family: \"Bebas Neue\";"),
        "the model word"
    );
    assert!(
        zone.contains("BlockPowerButton {"),
        "the round power button"
    );
    let power = ui("components/block_power_button.slint");
    assert!(power.contains("power-line.svg"));
    assert!(power.contains("@tr(\"block-editor-power\")"));
    let editor = ui("pages/block_panel_editor.slint");
    assert!(editor.contains("BlockPanelBrandZone {"));
    assert!(!editor.contains("FootSwitch {"));
    assert!(!editor.contains("BlockPanelBrandStrip {"));
    assert!(editor.contains("brand-w"));
}

#[test]
fn the_editor_tabs_are_a_boxed_row() {
    let tabs = ui("components/param_tab_bar.slint");
    assert!(tabs.contains("in property <bool> boxed"));
    assert!(
        tabs.contains("Theme.panel-lo"),
        "the row sits on the low panel"
    );
    let editor = ui("pages/block_panel_editor.slint");
    assert!(editor.contains("boxed: true;"));
}

#[test]
fn the_knobs_take_the_category_colour() {
    let grid = ui("components/block_param_grid.slint");
    assert!(grid.contains("in property <color> tint"));
    assert!(grid.contains("width: 48px;"), "the editor knob is 48px");
    let item = ui("components/block_panel_parameter_item.slint");
    assert!(item.contains("in property <color> tint"));
    assert!(item.contains("tint: root.tint;"));
    assert!(
        !item.contains("font-family: \"Bebas Neue\";"),
        "the caption is the UI face"
    );
}
