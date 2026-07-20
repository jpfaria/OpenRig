//! Tests for the objective quality metrics. Fixtures are synthetic signals of
//! known distortion / level, so each metric has an unambiguous expected value.

use super::*;

const SR: f32 = 48_000.0;

fn sine(freq: f32, amp: f32, secs: f32) -> Vec<f32> {
    let n = (secs * SR) as usize;
    (0..n)
        .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / SR).sin())
        .collect()
}

#[test]
fn thd_n_of_a_pure_tone_is_near_zero() {
    let d = thd_n(&sine(1_000.0, 0.5, 1.0), 1_000.0, SR);
    assert!(d < 0.01, "a pure 1 kHz tone has ~0 THD+N, got {d}");
}

#[test]
fn thd_n_rises_with_added_harmonics() {
    // Fundamental plus a third harmonic at 20 % amplitude → THD ≈ 0.2.
    let n = SR as usize;
    let sig: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * t).sin()
                + 0.1 * (2.0 * std::f32::consts::PI * 3_000.0 * t).sin()
        })
        .collect();
    let d = thd_n(&sig, 1_000.0, SR);
    assert!((d - 0.2).abs() < 0.02, "third harmonic at 20 % → THD+N ≈ 0.2, got {d}");
}

#[test]
fn noise_floor_of_silence_is_very_low() {
    let nf = rms_dbfs(&BatterySignal::Silence.generate(1.0, SR));
    assert!(nf <= -120.0, "silence should read a very low floor, got {nf} dBFS");
}

#[test]
fn peak_dbfs_of_half_scale_sine_is_about_minus_6db() {
    let p = peak_dbfs(&sine(1_000.0, 0.5, 0.1));
    assert!((p - (-6.02)).abs() < 0.3, "0.5 peak ≈ -6 dBFS, got {p}");
}

#[test]
fn rms_dbfs_of_half_scale_sine_is_about_minus_9db() {
    // RMS of a 0.5 sine = 0.5/√2 ≈ 0.3536 → ≈ -9 dBFS.
    let r = rms_dbfs(&sine(1_000.0, 0.5, 0.1));
    assert!((r - (-9.03)).abs() < 0.3, "0.5 sine RMS ≈ -9 dBFS, got {r}");
}

#[test]
fn clean_sine_does_not_clip() {
    assert_eq!(clip_fraction(&sine(1_000.0, 0.5, 0.1)), 0.0);
}

#[test]
fn clipped_signal_reports_clipping() {
    let clipped: Vec<f32> = sine(1_000.0, 2.0, 0.1).iter().map(|s| s.clamp(-1.0, 1.0)).collect();
    assert!(clip_fraction(&clipped) > 0.001, "hard-clipped sine clips");
}

#[test]
fn battery_sine_is_1khz_at_half_scale() {
    let s = BatterySignal::Sine1k.generate(0.5, SR);
    assert_eq!(s.len(), (0.5 * SR) as usize);
    assert!((peak_dbfs(&s) - (-6.02)).abs() < 0.3);
}

#[test]
fn assemble_reports_clean_chain_metrics() {
    let sine_out = sine(1_000.0, 0.5, 1.0);
    let silence_out = BatterySignal::Silence.generate(1.0, SR);
    let m = assemble(&sine_out, &silence_out, SR);
    assert!(m.thd_n < 0.01, "clean: low THD+N: {m:?}");
    assert!(m.noise_floor_dbfs <= -120.0, "clean: low noise floor: {m:?}");
    assert!((m.peak_dbfs - (-6.02)).abs() < 0.3, "{m:?}");
    assert!(m.dynamic_range_db > 0.0, "{m:?}");
    assert_eq!(m.clip_fraction, 0.0, "{m:?}");
}
