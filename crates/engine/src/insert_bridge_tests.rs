//! Unit tests for the insert bypass bridge (#967). The audible behaviour is
//! covered end to end in `issue_967_insert_bypass_tests.rs`.

use super::{blend, replace_bridges, InsertBridge};
use crate::audio_frame::AudioFrame;
use domain::ids::BlockId;

fn bridge(enabled: bool, producer: usize, consumer: usize) -> InsertBridge {
    InsertBridge::new(BlockId("insert".into()), enabled, 1, producer, consumer)
}

fn mono(frames: &[AudioFrame]) -> Vec<f32> {
    frames
        .iter()
        .map(|f| match f {
            AudioFrame::Mono(s) => *s,
            AudioFrame::Stereo([l, _]) => *l,
        })
        .collect()
}

#[test]
fn a_cross_callback_dry_path_is_held_to_about_one_period() {
    let mut b = bridge(false, 0, 1);
    let period = vec![AudioFrame::Mono(0.5); 128];
    for _ in 0..100 {
        b.shape_send(&mut period.clone());
    }
    assert!(
        b.held_frames() <= 2 * 128,
        "a producer nobody drains must not build delay — holds {}",
        b.held_frames()
    );
}

#[test]
fn a_same_callback_loop_parks_nothing() {
    let mut b = bridge(false, 0, 0);
    b.shape_send(&mut vec![AudioFrame::Mono(0.5); 128]);
    assert_eq!(b.held_frames(), 0, "the return reads the send in place");
}

#[test]
fn a_bypassed_send_ramps_to_silence_and_stays_there() {
    let mut b = bridge(true, 0, 0);
    b.set_enabled(true);
    b.set_enabled(false);
    let mut send = vec![AudioFrame::Mono(0.5); 512];
    b.shape_send(&mut send);
    let samples = mono(&send);
    assert!(
        samples[0] <= 0.5 && samples[511].abs() < 1e-6,
        "ends silent: {:?}",
        &samples[508..]
    );
}

#[test]
fn a_replaced_bridge_keeps_its_ramps_and_applies_the_new_flag() {
    let mut current = vec![bridge(true, 0, 0)];
    // The bridge being replaced was switched off live; the fresh one is built
    // from a chain that says the insert is on.
    current[0].set_enabled(false);
    replace_bridges(&mut current, vec![bridge(true, 0, 0)]);
    let mut gear = vec![AudioFrame::Mono(0.2); 1];
    current[0].mix_return(&mut gear, Some(&[AudioFrame::Mono(0.5)]));
    assert!(
        mono(&gear)[0] < 0.21,
        "the switch-on starts from where the old bridge was, not with a jump"
    );
}

#[test]
fn blend_matches_the_gear_frames_layout() {
    assert!(matches!(
        blend(AudioFrame::Stereo([0.0, 0.0]), AudioFrame::Mono(1.0), 1.0),
        AudioFrame::Stereo([l, r]) if l == 1.0 && r == 1.0
    ));
    assert!(matches!(
        blend(AudioFrame::Mono(0.0), AudioFrame::Stereo([1.0, 0.0]), 1.0),
        AudioFrame::Mono(m) if (m - 0.5).abs() < 1e-6
    ));
}
