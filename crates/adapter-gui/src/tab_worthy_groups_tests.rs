//! #915 — WHICH blocks deserve a tab bar, pinned per block kind. The rule is
//! not a parameter count: a NAM capture has nine parameters and three real
//! modules (Amp, Noise Gate, EQ), each with two or more knobs — those tabs are
//! the block's own structure and must stay. A native amp has ten parameters
//! spread over seven groups, five of them holding a single knob: that is not a
//! division of the panel, it is an amp's front panel cut into slivers.
//!
//! A group of one is a label, not a tab.

use crate::block_editor::block_parameter_items_for_model;
use crate::param_tab_grouping::tab_groups;
use crate::BlockParameterItem;
use project::param::ParameterSet;

fn items(effect_type: &str, model: &str) -> Vec<BlockParameterItem> {
    block_parameter_items_for_model(effect_type, model, &ParameterSet::default())
}

#[test]
fn a_nam_capture_keeps_its_module_tabs() {
    let param_items = items("nam", "neural_amp_modeler");
    assert!(
        !param_items.is_empty(),
        "the generic NAM block must resolve a schema"
    );
    // `Main` holds the capture's own rows; the rest are
    // `nam::params::{AMP,NOISE_GATE,EQ}_GROUP` (this crate does not depend on
    // `nam`, so they are spelled out). Two-to-four knobs each.
    assert_eq!(
        tab_groups(&param_items),
        vec!["Main", "Amp", "Noise Gate", "EQ"],
        "a NAM capture's modules are real groups — they stay tabs"
    );
}

/// The blocks whose groups do group: the tab bar is their own structure.
#[test]
fn a_block_whose_groups_group_keeps_its_tabs() {
    assert_eq!(
        tab_groups(&items("wah", "cry_classic")),
        vec!["Wah", "Output"],
        "two groups of two knobs"
    );
    // A block an EQ widget draws publishes no tabs whatever its groups look
    // like — the widget shows every band at once (#878).
    assert!(tab_groups(&items("filter", "eq_three_band_basic")).is_empty());
}

#[test]
fn a_native_amp_front_panel_is_not_cut_into_tabs() {
    // Seven groups, five of them a single knob (Input, Amp, Switches, Cab,
    // Output) — a tab bar there hides an amp's own panel behind itself.
    assert!(
        tab_groups(&items("amp", "blackface_clean")).is_empty(),
        "a native amp is one front panel"
    );
    assert!(
        tab_groups(&items("preamp", "american_clean")).is_empty(),
        "a native preamp is one front panel"
    );
    // Same shape elsewhere: a cab with three one-knob groups, and a saturation
    // whose every group is a single knob.
    assert!(tab_groups(&items("cab", "brit_4x12")).is_empty());
    assert!(tab_groups(&items("gain", "tube_saturation")).is_empty());
    assert!(tab_groups(&items("gain", "tape_saturation")).is_empty());
}

fn item(label: &str, group: &str) -> BlockParameterItem {
    BlockParameterItem {
        path: label.into(),
        label: label.into(),
        group: group.into(),
        widget_kind: "knob".into(),
        ..Default::default()
    }
}

#[test]
fn a_group_of_one_costs_the_block_its_tab_bar() {
    let real = vec![
        item("a", "Filter"),
        item("b", "Filter"),
        item("c", "Envelope"),
        item("d", "Envelope"),
    ];
    assert_eq!(
        tab_groups(&real),
        vec!["Filter", "Envelope"],
        "every group holds more than one parameter — the tabs are real"
    );

    let mut with_a_loose_knob = real.clone();
    with_a_loose_knob.push(item("e", "Mix"));
    assert!(
        tab_groups(&with_a_loose_knob).is_empty(),
        "one group holds a single knob, so the grouping does not divide the block"
    );
}

#[test]
fn a_plugin_too_big_for_one_panel_keeps_its_tabs_even_with_a_loose_knob() {
    // Above one panel there is no choice: something has to give, and a tab bar
    // beats a knob the window cannot show.
    let mut big: Vec<BlockParameterItem> = (0..12)
        .map(|i| item(&format!("p{i}"), if i < 6 { "A" } else { "B" }))
        .collect();
    big.push(item("loose", "C"));
    assert_eq!(tab_groups(&big), vec!["A", "B", "C"]);
}
