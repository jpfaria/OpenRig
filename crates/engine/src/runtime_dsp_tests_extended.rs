//! Extended structural battery for the output stage (issue #792 split
//! from runtime_dsp_tests.rs). Shares the signal helpers with the base
//! suite via super::tests.

use super::tests::{impulse, out_stage, sine};
use super::{apply_mixdown, output_limiter, ChainOutputMixdown};

// ── Extended structural battery (issue #496) ────────────────────
// Each test below pins one independent property of the output
// signal path. They were each written from a distinct concern
// about how the stage could mis-behave on real audio.

// boundedness, by region ----------------------------------------
#[test]
fn out_limiter_bounded_near_threshold_positive() {
    for x in (9300..=9700u32).map(|i| i as f32 / 10_000.0) {
        let y = output_limiter(x);
        assert!(y.abs() <= 1.0 && y.is_finite(), "x={x} y={y}");
    }
}
#[test]
fn out_limiter_bounded_near_threshold_negative() {
    for x in (9300..=9700u32).map(|i| -(i as f32 / 10_000.0)) {
        let y = output_limiter(x);
        assert!(y.abs() <= 1.0 && y.is_finite(), "x={x} y={y}");
    }
}
#[test]
fn out_limiter_bounded_just_above_unity() {
    for x in (10_000..=20_000u32).map(|i| i as f32 / 10_000.0) {
        let y = output_limiter(x);
        assert!(y.abs() <= 1.0 && y.is_finite());
    }
}
#[test]
fn out_limiter_bounded_extreme_positive() {
    for &x in &[10.0f32, 100.0, 1e3, 1e4, 1e5, 1e6] {
        assert!(output_limiter(x).abs() <= 1.0);
    }
}
#[test]
fn out_limiter_bounded_extreme_negative() {
    for &x in &[-10.0f32, -100.0, -1e3, -1e4, -1e5, -1e6] {
        assert!(output_limiter(x).abs() <= 1.0);
    }
}
#[test]
fn out_limiter_bounded_for_f32_max() {
    assert!(output_limiter(f32::MAX).abs() <= 1.0);
}
#[test]
fn out_limiter_bounded_for_f32_min() {
    assert!(output_limiter(f32::MIN).abs() <= 1.0);
}

// transparency, by region ---------------------------------------
#[test]
fn out_limiter_transparent_at_zero() {
    assert_eq!(output_limiter(0.0), 0.0);
}
#[test]
fn out_limiter_transparent_at_subnormal_positive() {
    let x = f32::MIN_POSITIVE / 2.0;
    assert_eq!(output_limiter(x), x);
}
#[test]
fn out_limiter_transparent_at_subnormal_negative() {
    let x = -f32::MIN_POSITIVE / 2.0;
    assert_eq!(output_limiter(x), x);
}
#[test]
fn out_limiter_transparent_dense_below_threshold_positive() {
    for i in 0..=9499u32 {
        let x = i as f32 / 10_000.0;
        assert_eq!(output_limiter(x), x, "x={x}");
    }
}
#[test]
fn out_limiter_transparent_dense_below_threshold_negative() {
    for i in 1..=9499u32 {
        let x = -(i as f32 / 10_000.0);
        assert_eq!(output_limiter(x), x, "x={x}");
    }
}
#[test]
fn out_limiter_transparent_at_exact_threshold_positive() {
    assert_eq!(output_limiter(0.95), 0.95);
}
#[test]
fn out_limiter_transparent_at_exact_threshold_negative() {
    assert_eq!(output_limiter(-0.95), -0.95);
}

// monotonicity, fine sweeps -------------------------------------
fn _mono_sweep(lo: f32, hi: f32, step: f32) {
    let mut prev = output_limiter(lo);
    let mut x = lo + step;
    while x <= hi {
        let y = output_limiter(x);
        assert!(y + 1e-6 >= prev, "non-mono x={x} prev={prev} y={y}");
        prev = y;
        x += step;
    }
}
#[test]
fn out_limiter_monotonic_near_threshold() {
    _mono_sweep(0.94, 0.96, 1e-5);
}
#[test]
fn out_limiter_monotonic_just_above_threshold() {
    _mono_sweep(0.95, 1.50, 1e-4);
}
#[test]
fn out_limiter_monotonic_above_unity() {
    _mono_sweep(1.0, 3.0, 1e-3);
}
#[test]
fn out_limiter_monotonic_zero_to_full_scale() {
    _mono_sweep(0.0, 1.0, 1e-4);
}
#[test]
fn out_limiter_monotonic_negative_zero_to_full() {
    let mut prev = output_limiter(-1.0);
    let mut x = -1.0 + 1e-4;
    while x <= 0.0 {
        let y = output_limiter(x);
        assert!(y + 1e-6 >= prev, "x={x}");
        prev = y;
        x += 1e-4;
    }
}

// continuity, finer step ---------------------------------------
#[test]
fn out_limiter_no_jump_dense_positive() {
    let mut prev = output_limiter(0.0);
    let mut x = 1e-5;
    while x <= 2.0 {
        let y = output_limiter(x);
        assert!((y - prev).abs() < 2e-5, "x={x} step={:.2e}", y - prev);
        prev = y;
        x += 1e-5;
    }
}
#[test]
fn out_limiter_no_jump_dense_negative() {
    let mut prev = output_limiter(0.0);
    let mut x = -1e-5_f32;
    while x >= -2.0 {
        let y = output_limiter(x);
        assert!((y - prev).abs() < 2e-5);
        prev = y;
        x -= 1e-5;
    }
}

// odd symmetry, many points -------------------------------------
#[test]
fn out_limiter_odd_symmetric_grid_below_threshold() {
    for i in 0..=950u32 {
        let x = i as f32 / 1000.0;
        assert!(
            (output_limiter(x) + output_limiter(-x)).abs() < 1e-6,
            "x={x}"
        );
    }
}
#[test]
fn out_limiter_odd_symmetric_grid_above_threshold() {
    for i in 950..=3000u32 {
        let x = i as f32 / 1000.0;
        assert!(
            (output_limiter(x) + output_limiter(-x)).abs() < 1e-6,
            "x={x}"
        );
    }
}

// out_stage @ various volumes -----------------------------------
#[test]
fn out_stage_silent_at_50pct_zero_input() {
    assert_eq!(out_stage(0.0, 50.0), 0.0);
}
#[test]
fn out_stage_silent_at_200pct_zero_input() {
    assert_eq!(out_stage(0.0, 200.0), 0.0);
}
#[test]
fn out_stage_50pct_halves_safe_signal() {
    for &s in &[0.1f32, 0.3, 0.5, 0.7, 0.9] {
        let y = out_stage(s, 50.0);
        assert!((y - s * 0.5).abs() < 1e-6, "s={s} y={y}");
    }
}
#[test]
fn out_stage_25pct_quarters_safe_signal() {
    for &s in &[0.1f32, 0.5, 0.9, -0.4] {
        let y = out_stage(s, 25.0);
        assert!((y - s * 0.25).abs() < 1e-6);
    }
}
#[test]
fn out_stage_75pct_three_quarters_safe_signal() {
    for &s in &[0.2f32, 0.4, 0.8, -0.6] {
        let y = out_stage(s, 75.0);
        assert!((y - s * 0.75).abs() < 1e-6);
    }
}
#[test]
fn out_stage_125pct_overshoot_above_unity_is_bounded() {
    for &s in &[0.8f32, 0.9, 0.95, 1.0] {
        let y = out_stage(s, 125.0);
        assert!(y.abs() <= 1.0);
    }
}
#[test]
fn out_stage_200pct_doubles_quiet_signal_safely() {
    for &s in &[0.0f32, 0.1, 0.3, 0.47] {
        let y = out_stage(s, 200.0);
        assert!((y - s * 2.0).abs() < 1e-6);
    }
}
#[test]
fn out_stage_400pct_quiet_signal_still_transparent_under_threshold() {
    for &s in &[0.0f32, 0.05, 0.1, 0.2] {
        let y = out_stage(s, 400.0);
        assert!((y - s * 4.0).abs() < 1e-6);
    }
}

// sign preservation across volume grid --------------------------
#[test]
fn out_stage_sign_grid_positive() {
    for &v in &[10.0f32, 50.0, 100.0, 125.0, 200.0] {
        for i in 1..1000u32 {
            let s = i as f32 / 1000.0;
            let y = out_stage(s, v);
            assert!(y >= 0.0);
        }
    }
}
#[test]
fn out_stage_sign_grid_negative() {
    for &v in &[10.0f32, 50.0, 100.0, 125.0, 200.0] {
        for i in 1..1000u32 {
            let s = -(i as f32 / 1000.0);
            let y = out_stage(s, v);
            assert!(y <= 0.0);
        }
    }
}

// DC stability across volumes -----------------------------------
#[test]
fn out_stage_dc_stable_at_50pct() {
    for &d in &[0.1f32, 0.3, 0.7, -0.4] {
        let buf: Vec<f32> = (0..128).map(|_| out_stage(d, 50.0)).collect();
        for &y in &buf {
            assert_eq!(y, buf[0]);
        }
    }
}
#[test]
fn out_stage_dc_stable_at_200pct_below_threshold() {
    for &d in &[0.1f32, 0.2, 0.4] {
        let buf: Vec<f32> = (0..128).map(|_| out_stage(d, 200.0)).collect();
        for &y in &buf {
            assert_eq!(y, buf[0]);
        }
    }
}

// no-DC introduced on AC. Use 200 Hz at 48 kHz (period = 240
// samples); 4800 samples = exactly 20 full cycles → raw sine
// integrates to ~0 numerically, so any DC seen is the stage's.
#[test]
fn out_stage_no_dc_offset_on_loud_sine() {
    let sig = sine(4800, 200.0, 0.95, 48_000.0);
    let outs: Vec<f32> = sig.iter().map(|s| out_stage(*s, 125.0)).collect();
    let mean = outs.iter().sum::<f32>() / outs.len() as f32;
    assert!(
        mean.abs() < 1e-3,
        "DC introduced at 125% on loud sine: {mean}"
    );
}
#[test]
fn out_stage_no_dc_offset_on_clean_sine() {
    let sig = sine(4800, 200.0, 0.5, 48_000.0);
    let outs: Vec<f32> = sig.iter().map(|s| out_stage(*s, 100.0)).collect();
    let mean = outs.iter().sum::<f32>() / outs.len() as f32;
    assert!(mean.abs() < 1e-5, "DC introduced on clean sine: {mean}");
}

// mixdown semantics + safety ------------------------------------
#[test]
fn mixdown_sum_matches_addition() {
    for (l, r) in [
        (0.0_f32, 0.0),
        (0.3, 0.4),
        (-0.5, 0.2),
        (0.9, 0.9),
        (-1.0, -1.0),
    ] {
        assert_eq!(apply_mixdown(ChainOutputMixdown::Sum, l, r), l + r);
    }
}
#[test]
fn mixdown_average_halves_sum() {
    for (l, r) in [(0.4_f32, 0.6), (-0.2, 0.8), (1.0, 1.0), (-1.0, 1.0)] {
        assert!((apply_mixdown(ChainOutputMixdown::Average, l, r) - (l + r) * 0.5).abs() < 1e-6);
    }
}
#[test]
fn mixdown_left_ignores_right() {
    for r in [-1.0_f32, 0.0, 0.7] {
        assert_eq!(apply_mixdown(ChainOutputMixdown::Left, 0.42, r), 0.42);
    }
}
#[test]
fn mixdown_right_ignores_left() {
    for l in [-1.0_f32, 0.0, 0.7] {
        assert_eq!(apply_mixdown(ChainOutputMixdown::Right, l, 0.42), 0.42);
    }
}
#[test]
fn mixdown_sum_can_overflow_to_two() {
    assert_eq!(apply_mixdown(ChainOutputMixdown::Sum, 1.0, 1.0), 2.0);
}
#[test]
fn mixdown_sum_can_underflow_to_minus_two() {
    assert_eq!(apply_mixdown(ChainOutputMixdown::Sum, -1.0, -1.0), -2.0);
}
#[test]
fn mixdown_average_never_exceeds_max_input() {
    for (l, r) in [(0.5_f32, 0.7), (-0.3, 0.9), (1.0, 0.0), (-1.0, 1.0)] {
        let m = apply_mixdown(ChainOutputMixdown::Average, l, r).abs();
        assert!(m <= l.abs().max(r.abs()) + 1e-6);
    }
}

// mixdown→limiter composition: every mode bounded
#[test]
fn limit_after_sum_mixdown_always_bounded() {
    for li in -10i32..=10 {
        for ri in -10i32..=10 {
            let (l, r) = (li as f32 / 10.0, ri as f32 / 10.0);
            let y = output_limiter(apply_mixdown(ChainOutputMixdown::Sum, l, r));
            assert!(y.abs() <= 1.0 && y.is_finite(), "l={l} r={r} y={y}");
        }
    }
}
#[test]
fn limit_after_avg_mixdown_always_bounded() {
    for li in -10i32..=10 {
        for ri in -10i32..=10 {
            let (l, r) = (li as f32 / 10.0, ri as f32 / 10.0);
            let y = output_limiter(apply_mixdown(ChainOutputMixdown::Average, l, r));
            assert!(y.abs() <= 1.0 && y.is_finite());
        }
    }
}
#[test]
fn limit_after_left_mixdown_always_bounded() {
    for li in -20i32..=20 {
        let l = li as f32 / 10.0;
        let y = output_limiter(apply_mixdown(ChainOutputMixdown::Left, l, 0.0));
        assert!(y.abs() <= 1.0);
    }
}
#[test]
fn limit_after_right_mixdown_always_bounded() {
    for ri in -20i32..=20 {
        let r = ri as f32 / 10.0;
        let y = output_limiter(apply_mixdown(ChainOutputMixdown::Right, 0.0, r));
        assert!(y.abs() <= 1.0);
    }
}

// full output-stage composition: volume × mixdown × limiter -----
#[test]
fn full_stage_sum_mixdown_125pct_bounded_grid() {
    for v in [50.0_f32, 100.0, 125.0, 200.0] {
        for li in -10i32..=10 {
            for ri in -10i32..=10 {
                let (l, r) = (li as f32 / 10.0, ri as f32 / 10.0);
                let y = output_limiter(apply_mixdown(ChainOutputMixdown::Sum, l, r) * v / 100.0);
                assert!(y.abs() <= 1.0 && y.is_finite(), "v={v} l={l} r={r} y={y}");
            }
        }
    }
}
#[test]
fn full_stage_avg_mixdown_125pct_bounded_grid() {
    for v in [50.0_f32, 100.0, 125.0, 200.0] {
        for li in -10i32..=10 {
            for ri in -10i32..=10 {
                let (l, r) = (li as f32 / 10.0, ri as f32 / 10.0);
                let y =
                    output_limiter(apply_mixdown(ChainOutputMixdown::Average, l, r) * v / 100.0);
                assert!(y.abs() <= 1.0 && y.is_finite());
            }
        }
    }
}

// impulses & impulses chains -----------------------------------
#[test]
fn out_stage_impulse_amp_0_3_at_100() {
    let n = 64;
    let i = impulse(n, 0.3);
    for (k, &s) in i.iter().enumerate() {
        let y = out_stage(s, 100.0);
        if k != n / 2 {
            assert_eq!(y, 0.0);
        } else {
            assert_eq!(y, 0.3);
        }
    }
}
#[test]
fn out_stage_impulse_amp_0_9_at_125() {
    let n = 64;
    let i = impulse(n, 0.9);
    for (k, &s) in i.iter().enumerate() {
        let y = out_stage(s, 125.0);
        if k != n / 2 {
            assert_eq!(y, 0.0);
        } else {
            assert!(y.abs() <= 1.0 && y > 0.9);
        }
    }
}
#[test]
fn out_stage_impulse_negative_amp_at_200() {
    let n = 64;
    let mut i = impulse(n, 0.0);
    i[n / 2] = -0.7;
    for (k, &s) in i.iter().enumerate() {
        let y = out_stage(s, 200.0);
        if k != n / 2 {
            assert_eq!(y, 0.0);
        } else {
            assert!(y < 0.0 && y.abs() <= 1.0);
        }
    }
}

// sine sweeps preserve no-clip at 125 ----------------------------
#[test]
fn out_stage_sine_grid_at_125_is_bounded() {
    for freq in [40.0_f32, 110.0, 220.0, 440.0, 1000.0, 4000.0] {
        for amp in [0.1_f32, 0.3, 0.5, 0.7, 0.9, 0.95, 1.0] {
            let sig = sine(2048, freq, amp, 48_000.0);
            let outs: Vec<f32> = sig.iter().map(|s| out_stage(*s, 125.0)).collect();
            assert!(
                outs.iter().all(|y| y.abs() <= 1.0 && y.is_finite()),
                "freq={freq} amp={amp}"
            );
        }
    }
}
#[test]
fn out_stage_sine_grid_no_dc_at_125() {
    for freq in [110.0_f32, 220.0, 440.0] {
        for amp in [0.5_f32, 0.8, 0.95] {
            let sig = sine(16384, freq, amp, 48_000.0);
            let outs: Vec<f32> = sig.iter().map(|s| out_stage(*s, 125.0)).collect();
            let mean = outs.iter().sum::<f32>() / outs.len() as f32;
            assert!(
                mean.abs() < 0.01,
                "DC introduced freq={freq} amp={amp}: {mean}"
            );
        }
    }
}

// identity: zero in chain → zero out chain ----------------------
#[test]
fn out_stage_silence_in_produces_silence_out_any_volume() {
    for v in [0.0_f32, 50.0, 100.0, 125.0, 200.0, 500.0] {
        for _ in 0..256 {
            assert_eq!(out_stage(0.0, v), 0.0);
        }
    }
}
