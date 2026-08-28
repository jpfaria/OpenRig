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

/// A NAM package declares its capture axes as parameters, and a package may
/// have a single axis — the Fender amps ship one (`channel`). That lone knob
/// forms a one-parameter Capture group next to three real modules, and it must
/// not cost the block its module tabs: the grouping still divides the block,
/// one loose knob and all.
#[test]
fn a_nam_amp_with_a_single_capture_axis_keeps_its_module_tabs() {
    let single_axis = [
        vec![item("channel", "Capture")],
        vec![
            item("input", "Amp"),
            item("output", "Amp"),
            item("slim", "Amp"),
        ],
        vec![
            item("gate_enabled", "Noise Gate"),
            item("threshold", "Noise Gate"),
        ],
        vec![
            item("eq_enabled", "EQ"),
            item("bass", "EQ"),
            item("middle", "EQ"),
            item("treble", "EQ"),
        ],
    ]
    .concat();
    assert_eq!(
        tab_groups(&single_axis),
        vec!["Capture", "Amp", "Noise Gate", "EQ"],
        "three modules and one capture axis — the tabs are the block's structure"
    );
}

#[test]
fn a_grouping_of_mostly_loose_knobs_costs_the_block_its_tab_bar() {
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

    // One loose knob among two real groups is still a grouping.
    let mut with_a_loose_knob = real.clone();
    with_a_loose_knob.push(item("e", "Mix"));
    assert_eq!(
        tab_groups(&with_a_loose_knob),
        vec!["Filter", "Envelope", "Mix"]
    );

    // Two of four groups holding a single knob is not: half the tabs are a
    // label with a knob under it.
    let mut half_loose = with_a_loose_knob.clone();
    half_loose.push(item("f", "Level"));
    assert!(
        tab_groups(&half_loose).is_empty(),
        "as many one-knob groups as real ones — the grouping does not divide the block"
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

/// The installed packages themselves, when a plugin tree is at hand: every NAM
/// package must keep its module tabs. Opt-in like the VST3 tests, since the
/// tree is a separate multi-GB repo — point `OPENRIG_TEST_PLUGINS_DIR` at its
/// `plugins/source`. Run against 335 packages (156 amp, 143 gain, 36 preamp).
#[test]
fn every_installed_nam_package_keeps_its_tabs() {
    let Some(root) = std::env::var_os("OPENRIG_TEST_PLUGINS_DIR").map(std::path::PathBuf::from)
    else {
        return;
    };
    plugin_loader::registry::init(&root);
    let mut checked = 0;
    for effect_type in ["amp", "preamp", "gain"] {
        let Ok(models) = project::catalog::supported_block_models(effect_type) else {
            continue;
        };
        for model in models.iter().filter(|m| m.model_id.starts_with("nam_")) {
            let param_items = items(effect_type, &model.model_id);
            if param_items.is_empty() {
                continue;
            }
            assert!(
                !tab_groups(&param_items).is_empty(),
                "{effect_type}/{} lost its module tabs",
                model.model_id
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "the plugin tree resolved no NAM package");
}
