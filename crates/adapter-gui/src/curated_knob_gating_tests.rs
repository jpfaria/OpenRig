//! #915 — a native amp/preamp shows a parameter tab bar, so its editor must
//! render the tab-filtered parameter grid, not the curated knob overlays: the
//! overlay path is positioned by loop index and ignores the active tab, so
//! every tab drew the same knobs (POWER listed GAIN and the EQ knobs), while
//! the parameters missing from the curated layout (input, bright, output)
//! could not be edited at all.

use crate::block_editor::{block_parameter_items_for_model, build_knob_overlays, parameter_groups};
use project::param::ParameterSet;

fn items(effect_type: &str, model: &str) -> Vec<crate::BlockParameterItem> {
    block_parameter_items_for_model(effect_type, model, &ParameterSet::default())
}

#[test]
fn a_tabbed_native_amp_publishes_no_curated_knob_overlays() {
    for (effect_type, model) in [
        ("amp", "blackface_clean"),
        ("amp", "chime"),
        ("amp", "tweed_breakup"),
        ("preamp", "american_clean"),
        ("preamp", "brit_crunch"),
        ("preamp", "modern_high_gain"),
    ] {
        let param_items = items(effect_type, model);
        assert!(
            parameter_groups(&param_items).len() > 1,
            "{effect_type}/{model} declares the tab groups this test is about"
        );
        let overlays = build_knob_overlays(
            project::catalog::model_knob_layout(effect_type, model),
            &param_items,
        );
        assert!(
            overlays.is_empty(),
            "{effect_type}/{model} has tabs, so it must render the tab-filtered grid; \
             got {} curated overlays that ignore the active tab",
            overlays.len()
        );
    }
}

#[test]
fn a_single_group_native_block_keeps_its_curated_knobs() {
    let param_items = items("gain", "volume");
    assert_eq!(parameter_groups(&param_items).len(), 1);
    let overlays = build_knob_overlays(
        project::catalog::model_knob_layout("gain", "volume"),
        &param_items,
    );
    assert!(
        !overlays.is_empty(),
        "a block with no tab bar still shows its curated knob layout"
    );
}

#[test]
fn every_native_amp_parameter_reaches_its_own_tab() {
    let param_items = items("amp", "blackface_clean");
    let groups = parameter_groups(&param_items);
    let visible_in = |group: &str| -> Vec<String> {
        crate::param_tab_grouping::retag_for_group(&param_items, group)
            .into_iter()
            .filter(|it| it.tab_slot >= 0)
            .map(|it| it.path.to_string())
            .collect()
    };
    assert_eq!(visible_in("Power"), vec!["master", "sag"]);
    assert_eq!(visible_in("EQ"), vec!["bass", "middle", "treble"]);
    assert_eq!(visible_in("Amp"), vec!["gain"]);
    assert_eq!(visible_in("Input"), vec!["input"]);
    assert_eq!(visible_in("Output"), vec!["output"]);
    assert_eq!(visible_in("Switches"), vec!["bright"]);
    assert_eq!(visible_in("Cab"), vec!["room_mix"]);
    let reachable: usize = groups.iter().map(|g| visible_in(g).len()).sum();
    assert_eq!(
        reachable,
        param_items.len(),
        "every parameter is reachable from exactly one tab"
    );
}
