use super::is_user_facing;
use vst3_host::param_flags::{CAN_AUTOMATE, IS_BYPASS, IS_HIDDEN, IS_READ_ONLY};
use vst3_host::Vst3ParamInfo;

fn param(title: &str, flags: i32) -> Vst3ParamInfo {
    Vst3ParamInfo {
        id: 7,
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
fn automatable_param_is_kept() {
    assert!(is_user_facing(&param("Mix", CAN_AUTOMATE)));
}

#[test]
fn hidden_param_is_skipped() {
    assert!(!is_user_facing(&param("Mix", CAN_AUTOMATE | IS_HIDDEN)));
}

#[test]
fn read_only_param_is_skipped() {
    assert!(!is_user_facing(&param("Output Meter", IS_READ_ONLY)));
}

#[test]
fn bypass_param_is_skipped() {
    // The block footswitch already is the bypass.
    assert!(!is_user_facing(&param("Bypass", CAN_AUTOMATE | IS_BYPASS)));
}

#[test]
fn non_automatable_placeholders_are_skipped() {
    for title in [
        "RESERVED1",
        "Reserved 2",
        "reserved_3",
        "Unused",
        "UNNAMED4",
    ] {
        assert!(!is_user_facing(&param(title, 0)), "{title} kept");
    }
}

#[test]
fn non_automatable_real_control_is_kept() {
    // Not automatable is not enough on its own: plenty of real controls
    // (e.g. a mode switch) clear kCanAutomate.
    assert!(is_user_facing(&param("Mode", 0)));
    // A word that merely starts like a placeholder is not one.
    assert!(is_user_facing(&param("Reserve Tank", 0)));
}

#[test]
fn automatable_reserved_slot_is_skipped() {
    // #1011: Valhalla marks RESERVED1..4 automatable; they still showed up.
    for title in ["RESERVED1", "RESERVED4", "Reserved 2"] {
        assert!(
            !is_user_facing(&param(title, CAN_AUTOMATE)),
            "{title} must be hidden"
        );
    }
}
