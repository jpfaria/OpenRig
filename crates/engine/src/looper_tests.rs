//! Issue #323 — behavioural tests for the per-chain looper core.
//!
//! Everything here is offline and deterministic: a slot is fed frames one by
//! one and the returned loop contribution is asserted sample by sample. No
//! audio device, no threads.

use super::*;

/// Frames of headroom used by the tests (small, so a full buffer is cheap).
const MAX: usize = 16;

fn spare(max_frames: usize) -> Box<[f32]> {
    vec![0.0; max_frames * 2].into_boxed_slice()
}

fn slot() -> LooperSlot {
    LooperSlot::new(MAX)
}

/// Feed `frames` into the slot, returning the loop contribution per frame.
fn feed(s: &mut LooperSlot, frames: &[[f32; 2]]) -> Vec<[f32; 2]> {
    frames.iter().map(|f| s.tick(*f)).collect()
}

#[test]
fn new_slot_is_empty_and_silent() {
    let mut s = slot();
    assert_eq!(s.state(), LooperState::Empty);
    assert_eq!(s.tick([0.5, -0.5]), [0.0, 0.0]);
    assert_eq!(s.len_frames(), 0);
}

#[test]
fn record_then_tap_plays_back_the_recorded_frames() {
    let mut s = slot();
    s.tap_record(Some(spare(MAX)));
    assert_eq!(s.state(), LooperState::Recording);

    let dry = [[0.1, 0.2], [0.3, 0.4], [0.5, 0.6]];
    // While recording the looper contributes nothing (no layer is playing).
    for out in feed(&mut s, &dry) {
        assert_eq!(out, [0.0, 0.0]);
    }

    s.tap_record(None);
    assert_eq!(s.state(), LooperState::Playing);
    assert_eq!(s.len_frames(), 3);

    // Playback replays exactly what was recorded, from the top, and wraps.
    let played = feed(&mut s, &[[0.0, 0.0]; 4]);
    assert_eq!(played[0], [0.1, 0.2]);
    assert_eq!(played[1], [0.3, 0.4]);
    assert_eq!(played[2], [0.5, 0.6]);
    assert_eq!(played[3], [0.1, 0.2], "loop must wrap to the start");
}

#[test]
fn overdub_sums_the_new_layer_onto_the_first() {
    let mut s = slot();
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0], [2.0, 2.0]]);
    s.tap_record(None); // freeze, 2 frames long

    s.tap_record(Some(spare(MAX))); // start overdub at position 0
    assert_eq!(s.state(), LooperState::Overdubbing);
    let out = feed(&mut s, &[[0.5, 0.5], [0.25, 0.25]]);
    // The old layer plays while the new one records.
    assert_eq!(out[0], [1.0, 1.0]);
    assert_eq!(out[1], [2.0, 2.0]);
    s.tap_record(None);
    assert_eq!(s.state(), LooperState::Playing);

    let played = feed(&mut s, &[[0.0, 0.0]; 2]);
    assert_eq!(played[0], [1.5, 1.5]);
    assert_eq!(played[1], [2.25, 2.25]);
}

#[test]
fn undo_drops_the_last_layer_and_redo_restores_it() {
    let mut s = slot();
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0]]);
    s.tap_record(None);
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[0.5, 0.5]]);
    s.tap_record(None);
    assert_eq!(feed(&mut s, &[[0.0, 0.0]])[0], [1.5, 1.5]);

    s.undo();
    assert_eq!(
        feed(&mut s, &[[0.0, 0.0]])[0],
        [1.0, 1.0],
        "undo removes the overdub"
    );

    s.redo();
    assert_eq!(
        feed(&mut s, &[[0.0, 0.0]])[0],
        [1.5, 1.5],
        "redo restores it"
    );
}

#[test]
fn recording_after_undo_drops_the_redo_tail_and_retires_its_buffer() {
    let mut s = slot();
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0]]);
    s.tap_record(None);
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[0.5, 0.5]]);
    s.tap_record(None);
    s.undo();

    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[0.25, 0.25]]);
    s.tap_record(None);

    assert_eq!(feed(&mut s, &[[0.0, 0.0]])[0], [1.25, 1.25]);
    s.redo();
    assert_eq!(
        feed(&mut s, &[[0.0, 0.0]])[0],
        [1.25, 1.25],
        "the undone layer is gone for good once a new one is recorded"
    );
    assert!(
        s.take_retired().is_some(),
        "the dropped layer is handed back for off-thread drop"
    );
}

#[test]
fn clear_empties_the_looper_and_retires_every_layer() {
    let mut s = slot();
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0]]);
    s.tap_record(None);

    s.clear();
    assert_eq!(s.state(), LooperState::Empty);
    assert_eq!(s.len_frames(), 0);
    assert_eq!(s.tick([0.0, 0.0]), [0.0, 0.0]);
    assert!(s.take_retired().is_some());
}

#[test]
fn stop_holds_the_loop_and_play_resumes_from_the_start() {
    let mut s = slot();
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0], [2.0, 2.0]]);
    s.tap_record(None);

    s.stop();
    assert_eq!(s.state(), LooperState::Stopped);
    assert_eq!(s.tick([0.0, 0.0]), [0.0, 0.0], "a stopped looper is silent");
    assert_eq!(s.len_frames(), 2, "stop keeps the material");

    s.play();
    assert_eq!(s.state(), LooperState::Playing);
    assert_eq!(s.tick([0.0, 0.0]), [1.0, 1.0]);
}

#[test]
fn recording_stops_at_the_buffer_ceiling_and_starts_playing() {
    let mut s = LooperSlot::new(3);
    s.tap_record(Some(spare(3)));
    feed(&mut s, &[[1.0, 1.0], [2.0, 2.0], [3.0, 3.0], [4.0, 4.0]]);

    assert_eq!(
        s.state(),
        LooperState::Playing,
        "the ceiling freezes the loop"
    );
    assert_eq!(s.len_frames(), 3);
}

#[test]
fn double_speed_reads_every_other_frame() {
    let mut s = LooperSlot::new(MAX);
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0], [2.0, 2.0], [3.0, 3.0], [4.0, 4.0]]);
    s.tap_record(None);

    s.set_speed(LooperSpeed::Double);
    let played = feed(&mut s, &[[0.0, 0.0]; 2]);
    assert_eq!(played[0], [1.0, 1.0]);
    assert_eq!(played[1], [3.0, 3.0]);
}

#[test]
fn half_speed_interpolates_between_frames() {
    let mut s = LooperSlot::new(MAX);
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0], [3.0, 3.0]]);
    s.tap_record(None);

    s.set_speed(LooperSpeed::Half);
    let played = feed(&mut s, &[[0.0, 0.0]; 3]);
    assert_eq!(played[0], [1.0, 1.0]);
    assert_eq!(played[1], [2.0, 2.0], "halfway between the two frames");
    assert_eq!(played[2], [3.0, 3.0]);
}

#[test]
fn reverse_walks_the_loop_backwards() {
    let mut s = LooperSlot::new(MAX);
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0], [2.0, 2.0], [3.0, 3.0]]);
    s.tap_record(None);

    s.set_reverse(true);
    let played = feed(&mut s, &[[0.0, 0.0]; 3]);
    assert_eq!(played[0], [1.0, 1.0]);
    assert_eq!(played[1], [3.0, 3.0]);
    assert_eq!(played[2], [2.0, 2.0]);
}

#[test]
fn mix_scales_the_loop_contribution() {
    let mut s = slot();
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0]]);
    s.tap_record(None);

    s.set_mix(0.5);
    assert_eq!(s.tick([0.0, 0.0]), [0.5, 0.5]);
}

#[test]
fn decay_attenuates_older_layers_by_their_age() {
    let mut s = slot();
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0]]);
    s.tap_record(None);
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0]]);
    s.tap_record(None);

    s.set_decay(0.5);
    // Newest layer at unity, the one below it one decay step down.
    assert_eq!(s.tick([0.0, 0.0]), [1.5, 1.5]);
}

#[test]
fn layer_budget_is_capped_and_reports_when_a_buffer_is_needed() {
    let mut s = slot();
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0]]);
    s.tap_record(None);

    for _ in 1..LOOPER_MAX_LAYERS {
        assert!(s.can_record(), "still below the layer cap");
        s.tap_record(Some(spare(MAX)));
        s.tap_record(None);
    }
    assert!(!s.can_record(), "the layer cap is reached");
    let rejected = spare(MAX);
    s.tap_record(Some(rejected));
    assert_eq!(
        s.state(),
        LooperState::Playing,
        "a refused overdub does not change state"
    );
    assert!(
        s.take_retired().is_some(),
        "the refused buffer is handed back"
    );
}

#[test]
fn position_tracks_playback() {
    let mut s = slot();
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0], [2.0, 2.0], [3.0, 3.0], [4.0, 4.0]]);
    s.tap_record(None);
    feed(&mut s, &[[0.0, 0.0]; 2]);
    assert_eq!(s.position_frames(), 2);
}

#[test]
fn load_layer_restores_a_saved_loop_stopped_and_ready() {
    let mut s = slot();
    let mut buf = spare(MAX);
    buf[0] = 1.0;
    buf[1] = 1.0;
    buf[2] = 2.0;
    buf[3] = 2.0;
    s.load_layer(buf, 2);

    assert_eq!(
        s.state(),
        LooperState::Stopped,
        "a loop restored from disk waits for the user, it does not start playing"
    );
    assert_eq!(s.len_frames(), 2);
    assert_eq!(s.active_layers(), 1);

    s.play();
    assert_eq!(s.tick([0.0, 0.0]), [1.0, 1.0]);
    assert_eq!(s.tick([0.0, 0.0]), [2.0, 2.0]);
}

#[test]
fn load_layer_into_a_recorded_looper_replaces_what_was_there() {
    let mut s = slot();
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[9.0, 9.0]]);
    s.tap_record(None);

    let mut buf = spare(MAX);
    buf[0] = 1.0;
    buf[1] = 1.0;
    s.load_layer(buf, 1);

    assert_eq!(s.active_layers(), 1);
    assert!(s.take_retired().is_some(), "the replaced layer comes back");
    s.play();
    assert_eq!(s.tick([0.0, 0.0]), [1.0, 1.0]);
}

#[test]
fn export_mixdown_sums_the_audible_layers_over_one_loop() {
    let mut s = slot();
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0], [2.0, 2.0]]);
    s.tap_record(None);
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[0.5, 0.5], [0.25, 0.25]]);
    s.tap_record(None);

    let pcm = s.export_mixdown().expect("there is material");
    assert_eq!(pcm, vec![1.5, 1.5, 2.25, 2.25]);
}

#[test]
fn export_mixdown_honours_undo_and_is_none_when_empty() {
    let mut s = slot();
    assert!(s.export_mixdown().is_none(), "nothing recorded yet");

    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0]]);
    s.tap_record(None);
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[0.5, 0.5]]);
    s.tap_record(None);
    s.undo();

    assert_eq!(
        s.export_mixdown().expect("the first layer is still there"),
        vec![1.0, 1.0],
        "an undone layer must not land in the saved file"
    );
}
