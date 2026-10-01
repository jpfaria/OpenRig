//! #328 spec §4.3: the shorter path is delayed by the difference, in a ring
//! preallocated at build, capped at 16384 samples.

use super::{align_delay, AlignDelay, MAX_ALIGN_SAMPLES};
use crate::runtime_audio_frame::AudioFrame;

fn ramp(start: usize, n: usize) -> Vec<AudioFrame> {
    (start..start + n)
        .map(|i| AudioFrame::Stereo([i as f32, -(i as f32)]))
        .collect()
}

fn left(frames: &[AudioFrame]) -> Vec<f32> {
    frames
        .iter()
        .map(|f| match f {
            AudioFrame::Stereo([l, _]) => *l,
            AudioFrame::Mono(s) => *s,
        })
        .collect()
}

#[test]
fn the_path_with_less_latency_is_delayed_by_the_difference() {
    let latencies = [0, 64, 7, 71];
    let longest = 71;
    let delays: Vec<(usize, bool)> = latencies
        .iter()
        .map(|&latency| align_delay(longest, latency))
        .collect();
    assert_eq!(
        delays,
        vec![(71, false), (7, false), (64, false), (0, false)],
        "#328 §11.4: every path meets the longest one, which is never delayed"
    );
    assert_eq!(align_delay(15, 15), (0, false));
}

#[test]
fn a_difference_above_the_cap_is_clamped() {
    assert_eq!(
        align_delay(MAX_ALIGN_SAMPLES + 10, 0),
        (MAX_ALIGN_SAMPLES, true)
    );
}

#[test]
fn delays_across_callbacks() {
    let mut delay = AlignDelay::with_capacity(64);
    delay.set_delay(5);
    let mut first = ramp(1, 4);
    delay.process(&mut first);
    let mut second = ramp(5, 4);
    delay.process(&mut second);
    assert_eq!(left(&first), vec![0.0; 4]);
    assert_eq!(left(&second), vec![0.0, 1.0, 2.0, 3.0]);
}

#[test]
fn zero_delay_leaves_the_frames_untouched() {
    let mut delay = AlignDelay::with_capacity(64);
    let mut frames = vec![AudioFrame::Mono(0.25); 3];
    delay.process(&mut frames);
    assert!(frames
        .iter()
        .all(|f| matches!(f, AudioFrame::Mono(s) if *s == 0.25)));
}

#[test]
fn a_delay_beyond_the_capacity_is_clamped() {
    let mut delay = AlignDelay::with_capacity(8);
    delay.set_delay(100);
    assert_eq!(delay.delay(), 8);
}

#[test]
fn the_capacity_never_exceeds_the_cap() {
    assert_eq!(
        AlignDelay::with_capacity(MAX_ALIGN_SAMPLES * 2).capacity(),
        MAX_ALIGN_SAMPLES
    );
}

#[test]
fn changing_the_delay_reads_history_already_recorded() {
    let mut delay = AlignDelay::with_capacity(16);
    let mut warm = ramp(1, 10);
    delay.process(&mut warm);
    delay.set_delay(3);
    let mut next = ramp(11, 2);
    delay.process(&mut next);
    assert_eq!(left(&next), vec![8.0, 9.0]);
}

#[test]
fn adopting_a_ring_of_the_same_size_keeps_its_history() {
    let mut old = AlignDelay::with_capacity(16);
    old.set_delay(3);
    let mut warm = ramp(1, 10);
    old.process(&mut warm);
    let mut fresh = AlignDelay::with_capacity(16);
    fresh.set_delay(3);
    fresh.adopt_history(old);
    let mut next = ramp(11, 2);
    fresh.process(&mut next);
    assert_eq!(left(&next), vec![8.0, 9.0]);
    assert_eq!(fresh.delay(), 3);
}

#[test]
fn a_ring_of_another_size_is_not_adopted() {
    let mut old = AlignDelay::with_capacity(8);
    let mut warm = ramp(1, 10);
    old.process(&mut warm);
    let mut fresh = AlignDelay::with_capacity(16);
    fresh.set_delay(3);
    fresh.adopt_history(old);
    let mut next = ramp(11, 2);
    fresh.process(&mut next);
    assert_eq!(left(&next), vec![0.0, 0.0]);
}
