//! Global mixer SOLO and strip-label rules (issue #1007).

use crate::mixer_solo::solo_silenced;
use crate::mixer_strip::MixerDirection;
use crate::mixer_strip_label::endpoint_channels_label;

#[test]
fn without_any_solo_nothing_is_silenced() {
    assert!(!solo_silenced(false, false));
}

#[test]
fn a_solo_in_the_group_silences_the_strips_not_soloed() {
    assert!(solo_silenced(false, true));
}

#[test]
fn a_soloed_strip_keeps_playing() {
    assert!(!solo_silenced(true, true));
}

#[test]
fn an_input_label_names_its_channel_one_based() {
    assert_eq!(endpoint_channels_label(MixerDirection::Input, &[0]), "IN 1");
}

#[test]
fn an_output_label_lists_every_channel_comma_separated() {
    assert_eq!(
        endpoint_channels_label(MixerDirection::Output, &[0, 1]),
        "OUT 1,2"
    );
    assert_eq!(
        endpoint_channels_label(MixerDirection::Output, &[24, 25]),
        "OUT 25,26"
    );
}
