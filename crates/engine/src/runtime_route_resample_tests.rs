//! Tests for the per-route sample-rate converter (#85).

use super::*;

fn mono(v: f32) -> AudioFrame {
    AudioFrame::Mono(v)
}

/// Same rate in and out ⇒ no converter at all, so a single-rate rig stays
/// bit-identical (it never even enters this code).
#[test]
fn matching_rates_need_no_resampler() {
    assert!(RouteResampler::new(48_000.0, 48_000.0, mono(0.0)).is_none());
    assert!(RouteResampler::new(0.0, 48_000.0, mono(0.0)).is_none());
    assert!(RouteResampler::new(f32::NAN, 48_000.0, mono(0.0)).is_none());
}

/// 44.1 kHz in, 48 kHz out: the converter has to hand the device MORE frames
/// than it was given, in the 48/44.1 proportion — that gap is exactly what
/// starved the route and crackled.
#[test]
fn upsampling_produces_the_destination_frame_count() {
    let mut r = RouteResampler::new(44_100.0, 48_000.0, mono(0.0)).expect("rates differ");
    let mut out = Vec::new();
    for i in 0..44_100 {
        r.push(mono((i % 100) as f32 / 100.0), &mut out);
    }
    let produced = out.len() as f64;
    let expected = 48_000.0;
    assert!(
        (produced - expected).abs() / expected < 0.001,
        "expected ~{expected} frames for one second, got {produced}"
    );
}

/// And the other way round: 48 kHz in, 44.1 kHz out hands over fewer.
#[test]
fn downsampling_produces_the_destination_frame_count() {
    let mut r = RouteResampler::new(48_000.0, 44_100.0, mono(0.0)).expect("rates differ");
    let mut out = Vec::new();
    for i in 0..48_000 {
        r.push(mono((i % 100) as f32 / 100.0), &mut out);
    }
    let produced = out.len() as f64;
    let expected = 44_100.0;
    assert!(
        (produced - expected).abs() / expected < 0.001,
        "expected ~{expected} frames for one second, got {produced}"
    );
}

/// A converted 440 Hz tone must still be that tone: the peak survives and the
/// zero crossings keep their count, so the pitch is unchanged.
#[test]
fn a_tone_keeps_its_level_and_its_pitch() {
    const IN_RATE: f32 = 44_100.0;
    const OUT_RATE: f32 = 48_000.0;
    let mut r = RouteResampler::new(IN_RATE, OUT_RATE, mono(0.0)).expect("rates differ");
    let mut out = Vec::new();
    let step = 2.0 * std::f32::consts::PI * 440.0 / IN_RATE;
    for i in 0..(IN_RATE as usize) {
        r.push(mono(0.5 * (i as f32 * step).sin()), &mut out);
    }

    let samples: Vec<f32> = out
        .iter()
        .map(|f| match f {
            AudioFrame::Mono(s) => *s,
            AudioFrame::Stereo([l, _]) => *l,
        })
        .collect();
    // Skip the first frames: the cubic primes on the first three.
    let body = &samples[16..];
    let peak = body.iter().fold(0.0_f32, |a, b| a.max(b.abs()));
    assert!(
        (peak - 0.5).abs() < 0.01,
        "the converted tone changed level: peak {peak}"
    );

    let crossings = body
        .windows(2)
        .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
        .count();
    // 440 Hz over one second = 440 rising zero crossings, ±1 for the edges.
    assert!(
        (439..=441).contains(&crossings),
        "the converted tone changed pitch: {crossings} rising crossings"
    );
}

/// Stereo goes through untouched channel-wise.
#[test]
fn stereo_frames_stay_stereo() {
    let mut r =
        RouteResampler::new(44_100.0, 48_000.0, AudioFrame::Stereo([0.0, 0.0])).expect("differ");
    let mut out = Vec::new();
    for _ in 0..64 {
        r.push(AudioFrame::Stereo([0.5, -0.5]), &mut out);
    }
    assert!(!out.is_empty());
    for frame in &out {
        assert!(
            matches!(frame, AudioFrame::Stereo(_)),
            "a stereo route must stay stereo, got {frame:?}"
        );
    }
}
