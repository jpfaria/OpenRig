use super::{dynamic_group_for, is_unlabeled, looks_like_on_off};

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
