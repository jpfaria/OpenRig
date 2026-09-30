//! #1007 — the surface only hears about a strip when its state changed, so a
//! 50 ms poll does not flood the MIDI port with the same fader position.
use super::*;

#[test]
fn first_sight_of_a_strip_is_a_change() {
    let mut t = MixerFeedbackTracker::default();
    assert!(t.changed("in:0@dev", 0.0, false));
}

#[test]
fn same_state_again_is_not_a_change() {
    let mut t = MixerFeedbackTracker::default();
    t.changed("in:0@dev", -6.0, false);
    assert!(!t.changed("in:0@dev", -6.0, false));
}

#[test]
fn gain_move_is_a_change() {
    let mut t = MixerFeedbackTracker::default();
    t.changed("in:0@dev", -6.0, false);
    assert!(t.changed("in:0@dev", -5.9, false));
}

#[test]
fn mute_flip_is_a_change() {
    let mut t = MixerFeedbackTracker::default();
    t.changed("in:0@dev", -6.0, false);
    assert!(t.changed("in:0@dev", -6.0, true));
}

#[test]
fn strips_are_tracked_independently() {
    let mut t = MixerFeedbackTracker::default();
    t.changed("in:0@dev", 0.0, false);
    assert!(t.changed("out:0,1@dev", 0.0, false));
    assert!(!t.changed("in:0@dev", 0.0, false));
}
