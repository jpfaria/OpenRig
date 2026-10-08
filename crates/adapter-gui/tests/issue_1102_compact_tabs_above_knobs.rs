//! #1102 — the compact card's parameter tabs sit on top of the knob area,
//! above the knobs, never in the side column under the model select.

use std::path::PathBuf;

fn row() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/pages/compact_block_row.slint");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn section<'a>(src: &'a str, from: &str, to: &str) -> &'a str {
    let start = src.find(from).unwrap_or_else(|| panic!("missing {from}"));
    let end = src[start..]
        .find(to)
        .map(|i| start + i)
        .unwrap_or_else(|| panic!("missing {to}"));
    &src[start..end]
}

#[test]
fn the_side_column_holds_no_parameter_tabs() {
    let src = row();
    let side = section(&src, "// ── Side column ──", "// ── Knob area ──");
    assert!(
        !side.contains("ParamTabBar {"),
        "the tabs must not sit under the model select"
    );
}

#[test]
fn the_knob_area_opens_with_the_parameter_tabs() {
    let src = row();
    let knobs = &src[src.find("// ── Knob area ──").expect("knob area")..];
    let tabs = knobs.find("ParamTabBar {").expect("tabs in the knob area");
    let strip = knobs
        .find("CompactBlockParamStrip {")
        .expect("strip in the knob area");
    let overlays = knobs
        .find("root.block-data.overlay-lines")
        .expect("overlays");
    assert!(
        tabs < strip && tabs < overlays,
        "the tabs come before the knobs"
    );
}
