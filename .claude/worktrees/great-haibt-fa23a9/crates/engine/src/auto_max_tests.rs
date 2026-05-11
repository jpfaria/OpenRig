//! Tests for the per-chain auto-max loudness (issue #402).

use super::*;

const SR: f32 = 48_000.0;

fn rms(buf: &[f32]) -> f32 {
    let m = buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32;
    m.sqrt()
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0_f32, |a, s| a.max(s.abs()))
}

fn lin_to_db(x: f32) -> f32 {
    20.0 * x.log10()
}

/// Drive a constant-amplitude tone through the auto-max long enough
/// for the gain follower to settle, return the steady-state output.
fn settle(state: &mut AutoMaxState, amplitude: f32, n: usize) -> Vec<f32> {
    let mut frames: Vec<AudioFrame> = (0..n)
        .map(|i| {
            // simple +/- alternating signal at a sub-audio rate so peak == amplitude
            let s = if i % 2 == 0 { amplitude } else { -amplitude };
            AudioFrame::Mono(s)
        })
        .collect();
    state.process(&mut frames);
    frames
        .iter()
        .map(|f| match f {
            AudioFrame::Mono(s) => *s,
            _ => panic!("expected mono"),
        })
        .collect()
}

#[test]
fn quiet_signal_is_boosted_toward_target_rms() {
    let mut s = AutoMaxState::with_enabled(SR, true);
    // -30 dBFS RMS sine wave (amplitude tracks RMS for full-scale
    // square-ish signal in `settle`). Wants +18 dB to hit -12 RMS,
    // peak ceiling +6 dBFS leaves ~36 dB of headroom — RMS target
    // is the binding constraint.
    let amp = 10.0_f32.powf(-30.0 / 20.0);
    let out = settle(&mut s, amp, (SR as usize) * 3);
    let tail = &out[out.len() - (SR as usize / 2)..];
    let final_rms_db = lin_to_db(rms(tail));
    assert!(
        (final_rms_db - TARGET_RMS_DBFS).abs() < 1.5,
        "quiet signal should reach RMS target; got {final_rms_db:.2} dBFS"
    );
}

#[test]
fn loud_signal_is_left_alone() {
    let mut s = AutoMaxState::with_enabled(SR, true);
    // Already at RMS target — auto-max should not attenuate.
    let amp = 10.0_f32.powf(TARGET_RMS_DBFS / 20.0);
    let out = settle(&mut s, amp, (SR as usize) * 2);
    let tail = &out[out.len() - (SR as usize / 2)..];
    let delta_db = (lin_to_db(rms(tail)) - TARGET_RMS_DBFS).abs();
    assert!(
        delta_db < 1.5,
        "loud signal must not be attenuated; Δ = {delta_db:.2} dB"
    );
}

#[test]
fn peak_ceiling_caps_high_crest_signal() {
    let mut s = AutoMaxState::with_enabled(SR, true);
    // Burst pattern: high peak, low RMS (crest factor ~ 20 dB).
    // Auto-max should NOT push peak past PEAK_CEILING_DBFS chasing RMS.
    let amp_peak = 0.05_f32; // -26 dBFS
    let mut frames: Vec<AudioFrame> = Vec::with_capacity((SR as usize) * 3);
    for i in 0..(SR as usize) * 3 {
        // 1-in-100 samples carry the burst, the rest are zero — peak
        // factor ~20 dB above RMS.
        let v = if i % 100 == 0 { amp_peak } else { 0.0 };
        let signed = if (i / 100) % 2 == 0 { v } else { -v };
        frames.push(AudioFrame::Mono(signed));
    }
    s.process(&mut frames);
    let tail: Vec<f32> = frames[frames.len() - (SR as usize / 2)..]
        .iter()
        .map(|f| match f {
            AudioFrame::Mono(s) => *s,
            _ => 0.0,
        })
        .collect();
    let final_peak_db = lin_to_db(peak(&tail));
    assert!(
        final_peak_db <= PEAK_CEILING_DBFS + 1.0,
        "peak ceiling should hold; got {final_peak_db:.2} dBFS"
    );
}

#[test]
fn boost_is_capped_at_max() {
    let mut s = AutoMaxState::with_enabled(SR, true);
    // Very quiet (-60 dBFS) but not silent. Desired RMS gain would
    // be ~+48 dB; must clamp to MAX_BOOST_DB.
    let amp = 10.0_f32.powf(-60.0 / 20.0);
    let out = settle(&mut s, amp, (SR as usize) * 3);
    let tail = &out[out.len() - (SR as usize / 2)..];
    let achieved_boost = lin_to_db(rms(tail)) - (-60.0);
    assert!(
        achieved_boost <= MAX_BOOST_DB + 1.0,
        "boost should be capped at {MAX_BOOST_DB} dB; got {achieved_boost:.2} dB"
    );
}

#[test]
fn silent_signal_does_not_explode_gain() {
    let mut s = AutoMaxState::with_enabled(SR, true);
    // Pure silence — peak envelope stays near zero, gain must NOT
    // chase it to +infinity.
    let mut frames: Vec<AudioFrame> = vec![AudioFrame::Mono(0.0); SR as usize];
    s.process(&mut frames);
    // Now push a small sample through and check gain didn't blow up.
    let mut probe = vec![AudioFrame::Mono(0.01); 1024];
    s.process(&mut probe);
    let r = rms(&probe
        .iter()
        .map(|f| match f {
            AudioFrame::Mono(s) => *s,
            _ => 0.0,
        })
        .collect::<Vec<_>>());
    // Should be bounded by max boost (24 dB) above the input rms (0.01).
    let max_expected = 0.01 * 10.0_f32.powf(MAX_BOOST_DB / 20.0);
    assert!(
        r <= max_expected,
        "silence should not explode gain; rms={r}, max_expected={max_expected}"
    );
}

#[test]
fn smooth_coefficients_are_in_unit_interval() {
    let s = AutoMaxState::with_enabled(SR, true);
    for c in [
        s.rms_coeff,
        s.peak_attack_coeff,
        s.peak_release_coeff,
        s.smooth_coeff,
    ] {
        assert!(c > 0.0 && c < 1.0, "coeff out of range: {c}");
    }
}

#[test]
fn disabled_state_passes_signal_through_unchanged() {
    // Default constructor inherits the process flag, which is OFF in
    // tests — proves volume invariant #10 holds for any caller that
    // doesn't opt in.
    let mut s = AutoMaxState::new(SR);
    let mut frames = vec![AudioFrame::Mono(0.5_f32); 1024];
    s.process(&mut frames);
    for f in frames {
        if let AudioFrame::Mono(v) = f {
            assert_eq!(v, 0.5_f32, "disabled auto-max must be unity");
        }
    }
}

#[test]
fn process_handles_stereo_frames() {
    let mut s = AutoMaxState::with_enabled(SR, true);
    let mut frames: Vec<AudioFrame> = (0..1024)
        .map(|i| {
            let v = if i % 2 == 0 { 0.05 } else { -0.05 };
            AudioFrame::Stereo([v, -v])
        })
        .collect();
    s.process(&mut frames);
    // No NaN / inf in output.
    for f in frames {
        if let AudioFrame::Stereo([l, r]) = f {
            assert!(l.is_finite() && r.is_finite(), "produced non-finite sample");
        }
    }
}
