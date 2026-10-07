//! #398 — the mixer: a dark tool bar, display tabs on a low row, then strips
//! on the well. A strip is a raised card: the level in a field, a thin groove
//! lit up to a white cap with a unity mark, small buttons, and a quiet label.
//! The chain's dual strip tints its GLOBAL half in the input colour.

use std::path::PathBuf;

fn ui(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ui")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn a_tool_window_opens_with_a_dark_bar() {
    let bar = ui("components/tool_bar.slint");
    assert!(bar.starts_with("// Responsibility: "));
    assert!(bar.contains("background: Theme.chrome;"));
    assert!(bar.contains("Theme.chrome-line"));
    assert!(
        bar.contains("@children"),
        "the bar hosts the window's switches"
    );
    let pill = ui("components/pill_switch.slint");
    assert!(pill.starts_with("// Responsibility: "));
    assert!(pill.contains("Gear.led"), "a lit switch shows its LED");
    assert!(pill.contains("Theme.chrome-field"));
}

#[test]
fn a_strip_is_a_raised_card_with_a_thin_groove() {
    let strip = ui("components/mixer_strip.slint");
    assert!(strip.contains("Theme.panel-hi"));
    assert!(strip.contains("Theme.groove"));
    assert!(strip.contains("in property <color> tint"));
    assert!(
        strip.contains("MixerBridge.unity-position"),
        "the unity mark"
    );
    assert!(strip.contains("Gear.knob-mid"), "the white cap");
    assert!(!strip.contains("font-family: \"Bebas Neue\";"));
    let label = ui("components/strip_label.slint");
    assert!(label.contains("Theme.hair-2"));
    assert!(!label.contains("font-family: \"Bebas Neue\";"));
    let globals = ui("components/mixer_globals.slint");
    assert!(globals.contains("unity-position"));
}

#[test]
fn the_global_half_takes_the_input_colour() {
    let dual = ui("components/dual_mixer_strip.slint");
    assert!(dual.contains("Theme.in.with-alpha(0.1)"));
    assert!(dual.contains("tint: Theme.in;"));
    assert!(dual.contains("Theme.panel-hi"));
}

#[test]
fn the_mixer_tabs_are_display_tabs_over_the_well() {
    let tabs = ui("components/param_tab_bar.slint");
    assert!(tabs.contains("in property <bool> display"));
    let window = ui("pages/mixer_window.slint");
    assert!(window.contains("ToolBar {"));
    assert!(window.contains("display: true;"));
    assert!(window.contains("background: Theme.well;"));
    let chain = ui("components/chain_mixer_tabs.slint");
    assert!(chain.contains("display: true;"));
    assert!(chain.contains("background: Theme.well;"));
}
