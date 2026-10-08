use super::{dynamic_group_for, is_unlabeled, looks_like_on_off, specs_from_params};
use vst3_host::param_flags::{CAN_AUTOMATE, IS_BYPASS, IS_HIDDEN};
use vst3_host::Vst3ParamInfo;

#[test]
fn dynamic_grouping_labels_by_longest_common_prefix() {
    // Real QDelay sections: params share more than the first word, so the
    // tab label should be the longest common leading-token prefix, not just
    // the first token ("Input EQ", not "Input").
    let titles: Vec<String> = [
        "Input EQ Band1 Freq",
        "Input EQ Band2 Gain",
        "Input EQ Band3 Q",
        "Saturation Pre",
        "Saturation Post",
        "Saturation Drive",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let g = dynamic_group_for(&titles);
    assert_eq!(g[0].as_deref(), Some("Input EQ"));
    assert_eq!(g[1].as_deref(), Some("Input EQ"));
    assert_eq!(g[2].as_deref(), Some("Input EQ"));
    // Saturation members diverge at token 2 → prefix collapses to "Saturation".
    assert_eq!(g[3].as_deref(), Some("Saturation"));
    assert_eq!(g[5].as_deref(), Some("Saturation"));
}

#[test]
fn dynamic_grouping_buckets_shared_leading_word() {
    let titles: Vec<String> = [
        "Gain Boost",
        "Gain Drive",
        "Gain Tone",
        "Delay Time",
        "Delay Feedback",
        "Level",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let groups = dynamic_group_for(&titles);
    // "Gain" is shared by 3 params (>= MIN_DYNAMIC_GROUP) → a tab.
    assert_eq!(groups[0].as_deref(), Some("Gain"));
    assert_eq!(groups[1].as_deref(), Some("Gain"));
    assert_eq!(groups[2].as_deref(), Some("Gain"));
    // "Delay" is shared by only 2 (< MIN) → ungrouped.
    assert_eq!(groups[3], None);
    assert_eq!(groups[4], None);
    // Unique word → ungrouped.
    assert_eq!(groups[5], None);
}

#[test]
fn unlabeled_params_are_dropped() {
    assert!(is_unlabeled("", ""));
    assert!(is_unlabeled("  ", " ")); // whitespace only
    assert!(!is_unlabeled("Gain", ""));
    assert!(!is_unlabeled("", "Drv")); // short_title is a usable label
                                       // No plugin-specific names: a param literally named "Blank" is kept —
                                       // OpenRig does not know or care about any plugin's placeholder convention.
    assert!(!is_unlabeled("Blank", ""));
}

fn opts(a: &str, b: &str) -> Vec<(String, String)> {
    vec![("0".into(), a.to_string()), ("100".into(), b.to_string())]
}

#[test]
fn on_off_by_name() {
    assert!(looks_like_on_off("Bypass", &opts("A", "B")));
    assert!(looks_like_on_off("Mono", &opts("A", "B")));
    assert!(looks_like_on_off("Gate Enable", &opts("A", "B")));
}

#[test]
fn on_off_by_labels() {
    assert!(looks_like_on_off("Foo", &opts("Off", "On")));
    assert!(looks_like_on_off("Foo", &opts("No", "Yes")));
    assert!(looks_like_on_off("Foo", &opts("", ""))); // empty labels
    assert!(looks_like_on_off("Foo", &opts("0", "1"))); // numeric labels
}

#[test]
fn real_mode_switch_stays_a_selector() {
    // Distinct, meaningful labels → NOT on/off → a 2-way selector.
    assert!(!looks_like_on_off("Mode", &opts("Sunlion", "Germanium")));
    assert!(!looks_like_on_off("Voicing", &opts("Vintage", "Modern")));
}

fn info(id: u32, title: &str, flags: i32) -> Vst3ParamInfo {
    Vst3ParamInfo {
        id,
        title: title.to_string(),
        short_title: String::new(),
        units: String::new(),
        step_count: 0,
        default_normalized: 0.5,
        flags,
        enum_options: Vec::new(),
        value_texts: Vec::new(),
    }
}

#[test]
fn schema_skips_params_the_plugin_marks_as_not_user_facing() {
    let params = [
        info(1, "Mix", CAN_AUTOMATE),
        info(2, "Hidden Thing", CAN_AUTOMATE | IS_HIDDEN),
        info(3, "BYPASS", CAN_AUTOMATE | IS_BYPASS),
        info(4, "RESERVED1", 0),
    ];
    let specs = specs_from_params(&params, &Default::default());
    let paths: Vec<&str> = specs.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(paths, ["p1"]);
}

#[test]
fn schema_labels_use_the_humanized_plugin_title() {
    let params = [
        info(1, "DELAY_MS", CAN_AUTOMATE),
        info(2, "preDelay", CAN_AUTOMATE),
    ];
    let specs = specs_from_params(&params, &Default::default());
    let labels: Vec<&str> = specs.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, ["Delay ms", "Pre Delay"]);
}

#[test]
fn continuous_knob_carries_the_plugin_value_texts() {
    let mut knob = info(1, "Decay", CAN_AUTOMATE);
    knob.value_texts = vec!["0.1 s".to_string(), "2.5 s".to_string()];
    let specs = specs_from_params(&[knob], &Default::default());
    assert_eq!(specs[0].value_labels, ["0.1 s", "2.5 s"]);
}

#[test]
fn label_drops_the_tab_name_it_repeats() {
    // #1011: ChowMatrix titles its knobs "Node 1: Delay" inside the "Node 1"
    // tab, so every knob repeated the tab name.
    let params = [
        info(1, "Node 1: Delay", CAN_AUTOMATE),
        info(2, "Node 1: Pan", CAN_AUTOMATE),
        info(3, "Node 1: Feedback", CAN_AUTOMATE),
        info(4, "Mix", CAN_AUTOMATE),
    ];
    let specs = specs_from_params(&params, &Default::default());
    let labels: Vec<&str> = specs.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, ["Delay", "Pan", "Feedback", "Mix"]);
}

#[test]
fn manifest_group_prefix_is_dropped_from_the_label() {
    let params = [info(7, "Node 2 Gain", CAN_AUTOMATE)];
    let groups = [(7u32, "Node 2".to_string())].into_iter().collect();
    let specs = specs_from_params(&params, &groups);
    assert_eq!(specs[0].label, "Gain");
    assert_eq!(specs[0].group.as_deref(), Some("Node 2"));
}

#[test]
fn stepped_parameter_with_too_many_steps_is_a_knob() {
    // #1104: an integer parameter with a huge range is a knob, not a selector.
    let mut wide = info(1, "Lookahead", CAN_AUTOMATE);
    wide.step_count = 20_000;
    let specs = specs_from_params(&[wide], &Default::default());
    assert!(matches!(
        specs[0].domain,
        block_core::param::ParameterDomain::FloatRange { .. }
    ));
}
