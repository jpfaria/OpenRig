//! #948: the Tone Doctor analyses the first SOUNDING source — guitars, then the
//! looper, then the DI — with the guitars summed and the playing loops summed.

use super::*;

const SR: f32 = 48_000.0;

fn tone(level: f32, frames: usize) -> ToneSignal {
    (vec![[level, -level]; frames], SR)
}

fn silence(frames: usize) -> ToneSignal {
    (vec![[0.0, 0.0]; frames], SR)
}

#[test]
fn a_sounding_guitar_wins_over_the_looper_and_the_di() {
    let picked = pick_signal(vec![
        Some(tone(0.5, 8)),
        Some(tone(0.25, 8)),
        Some(tone(0.1, 8)),
    ]);
    assert_eq!(picked, Some(tone(0.5, 8)));
}

#[test]
fn a_silent_guitar_falls_through_to_the_looper() {
    let picked = pick_signal(vec![
        Some(silence(8)),
        Some(tone(0.25, 8)),
        Some(tone(0.1, 8)),
    ]);
    assert_eq!(picked, Some(tone(0.25, 8)));
}

#[test]
fn with_no_guitar_and_no_loop_playing_the_di_is_analysed() {
    let picked = pick_signal(vec![Some(silence(8)), None, Some(tone(0.1, 8))]);
    assert_eq!(picked, Some(tone(0.1, 8)));
}

#[test]
fn when_nothing_sounds_the_first_existing_source_is_kept_to_report_the_silence() {
    let picked = pick_signal(vec![None, Some(silence(8)), None]);
    assert_eq!(picked, Some(silence(8)));
    assert_eq!(pick_signal(vec![None, None, None]), None);
}

#[test]
fn the_guitars_are_summed_frame_by_frame() {
    let summed = sum_streams(&[vec![0.1, 0.2, 0.3], vec![0.4, 0.5]]);
    let expected = [[0.5_f32, 0.5], [0.7, 0.7]];
    assert_eq!(summed.len(), expected.len());
    for (got, want) in summed.iter().zip(expected.iter()) {
        assert!(
            (got[0] - want[0]).abs() < 1e-6 && (got[1] - want[1]).abs() < 1e-6,
            "{summed:?}"
        );
    }
}

#[test]
fn the_playing_loops_are_summed_and_repeated_over_the_window() {
    // Loop A: 2 frames, loop B: 3 frames (interleaved L/R).
    let a = vec![1.0, 10.0, 2.0, 20.0];
    let b = vec![0.1, 0.01, 0.2, 0.02, 0.3, 0.03];
    let mixed = mix_loops(&[a, b], 5);
    let expected = [
        [1.1_f32, 10.01],
        [2.2, 20.02],
        [1.3, 10.03],
        [2.1, 20.01],
        [1.2, 10.02],
    ];
    assert_eq!(mixed.len(), 5, "{mixed:?}");
    for (got, want) in mixed.iter().zip(expected.iter()) {
        assert!(
            (got[0] - want[0]).abs() < 1e-5 && (got[1] - want[1]).abs() < 1e-5,
            "{mixed:?}"
        );
    }
}

#[test]
fn an_empty_loop_adds_nothing() {
    let mixed = mix_loops(&[Vec::new(), vec![0.5, 0.5]], 2);
    assert_eq!(mixed, vec![[0.5, 0.5], [0.5, 0.5]]);
}
