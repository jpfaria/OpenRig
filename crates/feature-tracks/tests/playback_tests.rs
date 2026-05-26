//! RED-first invariants for the RT-safe `MultiStemPlayer`.
//!
//! These tests exercise the per-stem mix math (mute, solo, gain, pan),
//! playhead advancement, and looping. The audio thread will call
//! `process` per buffer; tests do so synchronously to assert determinism.

use feature_tracks::MultiStemPlayer;

const SR: u32 = 44_100;

fn const_stereo(value: f32, frames: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(frames * 2);
    for _ in 0..frames {
        out.push(value);
        out.push(value);
    }
    out
}

#[test]
fn process_with_no_stems_fills_buffer_with_zeros() {
    let player = MultiStemPlayer::new(Vec::new(), SR);
    let mut out = vec![1.0_f32; 128];
    player.process(&mut out);
    assert!(out.iter().all(|s| *s == 0.0), "no stems must yield silence");
}

#[test]
fn process_advances_playhead_by_frame_count() {
    let player = MultiStemPlayer::new(vec![const_stereo(0.5, 4096)], SR);
    let mut out = vec![0.0_f32; 512];
    player.process(&mut out);
    assert_eq!(
        player.playhead(),
        256,
        "256 frames consumed = 512 samples / 2"
    );
}

#[test]
fn process_loops_back_to_zero_when_reaching_end_of_stems() {
    let player = MultiStemPlayer::new(vec![const_stereo(0.5, 100)], SR);
    let mut out = vec![0.0_f32; 200]; // 100 frames — exactly the buffer length
    player.process(&mut out);
    assert_eq!(player.playhead(), 0, "playhead must wrap at end of stems");

    let mut out2 = vec![0.0_f32; 200];
    player.process(&mut out2);
    assert!(
        out2.iter().any(|s| *s > 0.0),
        "after wrap, the next process must play from the start again"
    );
}

#[test]
fn muted_stem_does_not_contribute_to_mix() {
    let a = const_stereo(0.4, 256);
    let b = const_stereo(0.6, 256);
    let player = MultiStemPlayer::new(vec![a, b], SR);
    player.set_mute(1, true);

    let mut out = vec![0.0_f32; 64];
    player.process(&mut out);

    for sample in out {
        assert!(
            (sample - 0.4).abs() < 1e-5,
            "only stem 0 must contribute, got {sample}"
        );
    }
}

#[test]
fn soloed_stem_silences_every_other_stem() {
    let a = const_stereo(0.4, 256);
    let b = const_stereo(0.6, 256);
    let c = const_stereo(0.2, 256);
    let player = MultiStemPlayer::new(vec![a, b, c], SR);
    player.set_solo(1, true);

    let mut out = vec![0.0_f32; 64];
    player.process(&mut out);

    for sample in out {
        assert!(
            (sample - 0.6).abs() < 1e-5,
            "only the soloed stem 1 must play, got {sample}"
        );
    }
}

#[test]
fn gain_scales_stem_contribution_linearly() {
    let player = MultiStemPlayer::new(vec![const_stereo(0.5, 256)], SR);
    player.set_gain(0, 0.5);

    let mut out = vec![0.0_f32; 64];
    player.process(&mut out);

    for sample in out {
        assert!(
            (sample - 0.25).abs() < 1e-5,
            "gain 0.5 of 0.5 must yield 0.25, got {sample}"
        );
    }
}

#[test]
fn pan_full_left_zeros_right_channel_and_keeps_left() {
    let player = MultiStemPlayer::new(vec![const_stereo(0.5, 256)], SR);
    player.set_pan(0, -1.0);

    let mut out = vec![0.0_f32; 64];
    player.process(&mut out);

    for frame in out.chunks_exact(2) {
        assert!(
            (frame[0] - 0.5).abs() < 1e-5,
            "left must keep the source level, got {}",
            frame[0]
        );
        assert!(
            frame[1].abs() < 1e-5,
            "right must be silent when fully panned left, got {}",
            frame[1]
        );
    }
}

#[test]
fn pan_full_right_zeros_left_channel_and_keeps_right() {
    let player = MultiStemPlayer::new(vec![const_stereo(0.5, 256)], SR);
    player.set_pan(0, 1.0);

    let mut out = vec![0.0_f32; 64];
    player.process(&mut out);

    for frame in out.chunks_exact(2) {
        assert!(
            frame[0].abs() < 1e-5,
            "left must be silent when fully panned right, got {}",
            frame[0]
        );
        assert!(
            (frame[1] - 0.5).abs() < 1e-5,
            "right must keep the source level, got {}",
            frame[1]
        );
    }
}

#[test]
fn out_of_bounds_index_setters_are_silently_ignored() {
    let player = MultiStemPlayer::new(vec![const_stereo(0.5, 256)], SR);
    player.set_gain(999, 0.0);
    player.set_mute(42, true);
    player.set_solo(7, true);
    player.set_pan(3, -0.5);

    let mut out = vec![0.0_f32; 64];
    player.process(&mut out);

    for sample in out {
        assert!(
            (sample - 0.5).abs() < 1e-5,
            "out-of-bounds setters must be no-ops; got {sample}"
        );
    }
}
