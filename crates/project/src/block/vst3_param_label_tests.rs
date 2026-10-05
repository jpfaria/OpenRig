use super::humanize_param_label;

#[test]
fn snake_case_all_caps_becomes_sentence_case() {
    assert_eq!(humanize_param_label("DELAY_MS", ""), "Delay ms");
    assert_eq!(humanize_param_label("HIGH_CUT_FREQ", ""), "High cut freq");
}

#[test]
fn glued_all_caps_is_only_case_folded() {
    // No dictionary: words glued without a separator cannot be split reliably.
    assert_eq!(
        humanize_param_label("LATEDIFFUSIONFEEDBACK", ""),
        "Latediffusionfeedback"
    );
    assert_eq!(humanize_param_label("MIX", ""), "Mix");
}

#[test]
fn camel_case_is_split() {
    assert_eq!(humanize_param_label("delayTime", ""), "Delay Time");
    assert_eq!(humanize_param_label("PreDelay", ""), "Pre Delay");
    assert_eq!(humanize_param_label("LFOSpeed", ""), "LFO Speed");
}

#[test]
fn readable_titles_are_unchanged() {
    for t in ["Input EQ Band1 Freq", "Mix", "Decay (s)", "Low Cut"] {
        assert_eq!(humanize_param_label(t, ""), t);
    }
}

#[test]
fn separators_collapse_to_single_spaces() {
    assert_eq!(humanize_param_label("  Wet__Level  ", ""), "Wet Level");
    assert_eq!(humanize_param_label("mod-depth", ""), "Mod depth");
}

#[test]
fn falls_back_to_short_title() {
    assert_eq!(humanize_param_label("", "DRV_LVL"), "Drv lvl");
    assert_eq!(humanize_param_label("   ", "Gain"), "Gain");
}

#[test]
fn unit_prefixes_and_plural_acronyms_stay_whole() {
    assert_eq!(humanize_param_label("Gain dB", ""), "Gain dB");
    assert_eq!(humanize_param_label("Freq kHz", ""), "Freq kHz");
    assert_eq!(humanize_param_label("LFOs Sync", ""), "LFOs Sync");
    assert_eq!(humanize_param_label("EQs", ""), "EQs");
}
