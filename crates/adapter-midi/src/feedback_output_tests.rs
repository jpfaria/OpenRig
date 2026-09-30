//! #1007 — feedback only goes to the controller the map names. Without a
//! name it goes nowhere: blasting fader bytes at every MIDI output would hit
//! unrelated gear (amp modelers, synths) on the same machine.
use super::*;

fn ports() -> Vec<String> {
    vec![
        "SMC-Mixer".to_string(),
        "Ampero II".to_string(),
        "smc-mixer port 2".to_string(),
    ]
}

#[test]
fn no_named_device_means_no_feedback_port() {
    assert!(feedback_port_indices(&ports(), None).is_empty());
}

#[test]
fn named_device_selects_every_matching_output_case_insensitively() {
    assert_eq!(feedback_port_indices(&ports(), Some("smc")), vec![0, 2]);
}

#[test]
fn unmatched_name_selects_nothing() {
    assert!(feedback_port_indices(&ports(), Some("Chocolate")).is_empty());
}
