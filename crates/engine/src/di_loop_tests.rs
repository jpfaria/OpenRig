use super::*;
use block_core::AudioChannelLayout;

#[test]
fn from_samples_mono_no_resample_preserves_len_and_layout() {
    let samples = vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7];
    let di = DiLoop::from_samples(&samples, 48_000, 1, 48_000, 0);
    assert_eq!(di.layout(), AudioChannelLayout::Mono);
    assert_eq!(di.len(), 8);
}

#[test]
fn from_samples_stereo_deinterleaves_to_stereo_frames() {
    let samples = vec![0.1, 0.2, 0.3, 0.4];
    let di = DiLoop::from_samples(&samples, 48_000, 2, 48_000, 0);
    assert_eq!(di.layout(), AudioChannelLayout::Stereo);
    assert_eq!(di.len(), 2);
    match di.frame_at(0) {
        DiFrame::Stereo([l, r]) => assert!((l - 0.1).abs() < 1e-6 && (r - 0.2).abs() < 1e-6),
        _ => panic!("expected stereo"),
    }
}

#[test]
fn frame_at_wraps_around() {
    let samples = vec![0.0, 1.0, 2.0];
    let di = DiLoop::from_samples(&samples, 48_000, 1, 48_000, 0);
    match (di.frame_at(0), di.frame_at(3), di.frame_at(4)) {
        (DiFrame::Mono(a), DiFrame::Mono(b), DiFrame::Mono(c)) => {
            assert_eq!(a, 0.0);
            assert_eq!(b, 0.0);
            assert_eq!(c, 1.0);
        }
        _ => panic!("mono expected"),
    }
}

#[test]
fn resample_doubles_length_when_target_is_double() {
    let samples = vec![0.0, 0.25, 0.5, 0.75];
    let di = DiLoop::from_samples(&samples, 24_000, 1, 48_000, 0);
    assert!((di.len() as i64 - 8).abs() <= 1, "len was {}", di.len());
}

#[test]
fn loop_crossfade_makes_seam_continuous() {
    let n = 256;
    let samples: Vec<f32> = (0..n).map(|i| i as f32 / n as f32).collect();
    let xfade = 32;
    let di = DiLoop::from_samples(&samples, 48_000, 1, 48_000, xfade);
    let last = match di.frame_at(di.len() - 1) {
        DiFrame::Mono(s) => s,
        _ => unreachable!(),
    };
    let first = match di.frame_at(0) {
        DiFrame::Mono(s) => s,
        _ => unreachable!(),
    };
    let seam_step = (first - last).abs();
    assert!(
        seam_step < 0.5,
        "seam step {seam_step} not reduced by crossfade"
    );
}

#[test]
fn loop_wrap_step_is_no_worse_than_the_body() {
    // #614 clipping report: a sine that does NOT complete whole cycles over `n`
    // has mismatched ends (head[0] != tail[n-1]) AND head[0] != head[xfade], so
    // a crossfade that merely pulls the tail toward head[xfade-1] leaves a step
    // at the actual wrap point (last -> first). Through a high-gain chain that
    // step becomes an audible click that sounds like clipping on every loop.
    // A correct loop crossfade makes the wrap as smooth as the body.
    let n = 4096;
    let cycles = 9.37_f32; // non-integer ⇒ mismatched ends
    let samples: Vec<f32> = (0..n)
        .map(|i| (2.0 * std::f32::consts::PI * cycles * i as f32 / n as f32).sin() * 0.5)
        .collect();
    let xfade = 256;
    let di = DiLoop::from_samples(&samples, 48_000, 1, 48_000, xfade);
    let m = di.len();
    let s = |i: usize| match di.frame_at(i) {
        DiFrame::Mono(v) => v,
        _ => unreachable!(),
    };
    let mut steps: Vec<f32> = (1..m).map(|i| (s(i) - s(i - 1)).abs()).collect();
    steps.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = steps[steps.len() / 2];
    let wrap = (s(0) - s(m - 1)).abs();
    assert!(
        wrap <= median * 3.0 + 1e-6,
        "loop wrap step {wrap} >> body median step {median} — seam discontinuity (click/clip on restart)"
    );
}

// ── #669 was a DI loop in slow motion: the loop built at 48 kHz played into a
//    44.1 kHz device clock. These pin the DOWNSAMPLE path (the user's real
//    case), the identity path, and short-loop rounding.

#[test]
fn downsample_48k_to_44k1_length_matches_ratio() {
    // The #669 case: a 48 kHz loop resampled to the device's 44.1 kHz clock.
    let samples: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.001).fract()).collect();
    let di = DiLoop::from_samples(&samples, 48_000, 1, 44_100, 0);
    let expected = (1000.0_f64 * 44_100.0 / 48_000.0).round() as usize; // 919
    assert_eq!(
        di.len(),
        expected,
        "downsampled length must scale by 44100/48000"
    );
}

#[test]
fn identity_rate_preserves_every_sample() {
    // src_sr == engine_sr: no resample, byte-for-byte passthrough.
    let samples = vec![-0.9, -0.1, 0.0, 0.25, 0.5, 0.99];
    let di = DiLoop::from_samples(&samples, 48_000, 1, 48_000, 0);
    assert_eq!(di.len(), samples.len());
    for (i, &expected) in samples.iter().enumerate() {
        match di.frame_at(i) {
            DiFrame::Mono(v) => {
                assert!((v - expected).abs() < 1e-6, "frame {i}: {v} != {expected}")
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn short_loop_downsample_rounds_to_nearest() {
    // 7 frames @ 48k → round(7 * 44100/48000) = round(6.43) = 6.
    let samples = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7];
    let di = DiLoop::from_samples(&samples, 48_000, 1, 44_100, 0);
    assert_eq!(di.len(), 6);
}

#[test]
fn single_frame_loop_stays_one_frame() {
    let di = DiLoop::from_samples(&[0.5], 48_000, 1, 44_100, 0);
    assert_eq!(di.len(), 1, "a 1-frame loop cannot be resampled away");
}

#[test]
fn downsample_preserves_a_sine_period_within_one_frame() {
    // A 1 kHz sine at 48 kHz (period 48 frames), 4 cycles. After 44.1 kHz
    // resample the total length must scale by the rate ratio (period preserved
    // ⇒ no slow-motion/pitch shift, the #669 symptom).
    let freq = 1_000.0_f32;
    let src_sr = 48_000.0_f32;
    let n = (src_sr / freq) as usize * 4; // 4 full cycles
    let samples: Vec<f32> = (0..n)
        .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / src_sr).sin())
        .collect();
    let di = DiLoop::from_samples(&samples, 48_000, 1, 44_100, 0);
    let expected = (n as f64 * 44_100.0 / 48_000.0).round() as i64;
    assert!(
        (di.len() as i64 - expected).abs() <= 1,
        "len {} should be ~{expected} (period preserved, not stretched)",
        di.len()
    );
}

#[test]
fn crossfade_shortens_the_loop_by_the_fade_length() {
    // The crossfade folds `xfade` frames of the tail into the head, so the
    // looped body is `n - xfade` long.
    let n = 512;
    let samples: Vec<f32> = (0..n).map(|i| i as f32 / n as f32).collect();
    let xfade = 64;
    let di = DiLoop::from_samples(&samples, 48_000, 1, 48_000, xfade);
    assert_eq!(di.len(), n - xfade);
}
