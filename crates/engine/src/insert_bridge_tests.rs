//! Tests for the insert dry bridge (#967).

use super::InsertBridge;
use crate::audio_frame::AudioFrame;

const SILENCE: AudioFrame = AudioFrame::Mono(0.0);

/// `AudioFrame` carries no `PartialEq` (it is an audio-thread type); compare
/// the mono samples the tests park.
fn samples(frames: &[AudioFrame]) -> Vec<f32> {
    frames
        .iter()
        .map(|f| match f {
            AudioFrame::Mono(s) => *s,
            AudioFrame::Stereo([l, _]) => *l,
        })
        .collect()
}

#[test]
fn parked_frames_come_back_in_order() {
    let mut bridge = InsertBridge::new(true, 8);
    bridge.park(&[AudioFrame::Mono(0.1), AudioFrame::Mono(0.2)]);
    let mut out = Vec::new();
    bridge.take(2, SILENCE, &mut out);
    assert_eq!(samples(&out), vec![0.1, 0.2]);
}

#[test]
fn a_short_read_pads_with_silence() {
    let mut bridge = InsertBridge::new(true, 8);
    bridge.park(&[AudioFrame::Mono(0.5)]);
    let mut out = Vec::new();
    bridge.take(3, SILENCE, &mut out);
    assert_eq!(
        samples(&out),
        vec![0.5, 0.0, 0.0],
        "the return callback may run ahead of the send callback"
    );
}

#[test]
fn parking_past_the_capacity_drops_the_oldest_and_never_grows() {
    let mut bridge = InsertBridge::new(true, 4);
    for i in 0..10 {
        bridge.park(&[AudioFrame::Mono(i as f32)]);
    }
    let mut out = Vec::new();
    bridge.take(4, SILENCE, &mut out);
    assert_eq!(
        samples(&out),
        vec![6.0, 7.0, 8.0, 9.0],
        "a bridge nobody drains keeps the NEWEST frames and stays bounded"
    );
}
