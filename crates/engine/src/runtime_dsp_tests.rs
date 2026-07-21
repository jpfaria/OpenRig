//! Issue #496 numeric audit of the engine output stage. These do not
//! pretend to validate "weight" — they prove hard, mechanical
//! properties (continuity, monotonicity, boundedness) of every
//! chain output's final stage. If they fail, every chain — native
//! or NAM — is being mangled at the boundary and no per-block fix
//! can save the sound.
use super::{apply_mixdown, output_limiter, ChainOutputMixdown};

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

#[test]
fn output_limiter_is_continuous_at_its_threshold() {
    // A discontinuity here = audible step + harmonic distortion
    // at every loud peak. Must be < 0.1 dB.
    let below = output_limiter(0.9499);
    let at = output_limiter(0.9500);
    let above = output_limiter(0.9501);
    let step_db = db(at.abs()) - db(below.abs());
    eprintln!(
        "output_limiter @ threshold:  below={below:.4}  at={at:.4}  \
         above={above:.4}  step = {step_db:+.2} dB"
    );
    assert!(
        step_db.abs() < 0.1,
        "DISCONTINUOUS: {step_db:+.2} dB step at 0.95"
    );
}

#[test]
fn output_limiter_is_monotonic_across_the_operating_range() {
    // More input must never give less output. A 125 % chain
    // volume on a hot signal lands right in this range.
    let mut prev = -1.0_f32;
    let mut x = 0.0_f32;
    let mut first_bad: Option<(f32, f32, f32)> = None;
    while x <= 3.0 {
        let y = output_limiter(x);
        if y + 1e-6 < prev && first_bad.is_none() {
            first_bad = Some((x, prev, y));
        }
        prev = y;
        x += 0.001;
    }
    if let Some((x, p, y)) = first_bad {
        panic!("NON-MONOTONIC at x={x:.4}: prev={p:.4} → now={y:.4}");
    }
}

#[test]
fn output_limiter_never_exceeds_full_scale() {
    for &x in &[1.0, 2.0, 5.0, 50.0, 1.0e6, -1.0, -50.0] {
        let y = output_limiter(x);
        assert!(
            y.abs() <= 1.0 && y.is_finite(),
            "output_limiter({x}) = {y} > full-scale or NaN"
        );
    }
}

#[test]
fn output_limiter_is_transparent_well_below_threshold() {
    // Normal playing landing far below 0.95 must be byte-equal.
    for &x in &[0.0, 0.1, 0.3, 0.5, 0.7, 0.85, -0.6] {
        assert_eq!(output_limiter(x), x, "altered safe sample {x}");
    }
}

#[test]
fn stereo_sum_mixdown_can_exceed_full_scale() {
    // L+R sum can hit 2.0 from two unity-peak stereo channels —
    // that path then needs a real limiter, not a band-aid.
    let y = apply_mixdown(ChainOutputMixdown::Sum, 1.0, 1.0);
    assert_eq!(y, 2.0, "Sum mixdown semantics");
    // Average is the safe fold; document the contrast.
    assert_eq!(apply_mixdown(ChainOutputMixdown::Average, 1.0, 1.0), 1.0);
}

// ── Output stage signal-path audit (issue #496) ─────────────────
// "Sem mais suposição." The output of every chain (native, NAM,
// IR, anything) passes the same final math: `signal * volume_ratio`
// then `output_limiter` (then per-channel write). The tests below
// drive realistic signals through that exact composition and
// assert hard properties — no ear, only numbers. If any of these
// fails on real audio, every chain sounds bad regardless of the
// blocks inside it.

/// Exactly what the audio callback does per sample on the mono
/// single-stream path: `output_limiter(signal * volume_ratio)`.
fn out_stage(sample: f32, volume_pct: f32) -> f32 {
    output_limiter(sample * (volume_pct / 100.0))
}

fn peak(s: &[f32]) -> f32 {
    s.iter().fold(0.0_f32, |a, &b| a.max(b.abs()))
}
fn rms(s: &[f32]) -> f32 {
    (s.iter().map(|v| v * v).sum::<f32>() / s.len().max(1) as f32).sqrt()
}

// signal generators --------------------------------------------

fn sine(n: usize, freq: f32, amp: f32, sr: f32) -> Vec<f32> {
    (0..n)
        .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin())
        .collect()
}
fn plucked(n: usize, freq: f32, amp: f32, tau: f32, sr: f32) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            amp * (-t / tau).exp() * (2.0 * std::f32::consts::PI * freq * t).sin()
        })
        .collect()
}
fn impulse(n: usize, amp: f32) -> Vec<f32> {
    let mut v = vec![0.0; n];
    v[n / 2] = amp;
    v
}
fn dc(n: usize, level: f32) -> Vec<f32> {
    vec![level; n]
}

// ── property suite (per scenario × per volume) ─────────────────

/// At 100 % volume a signal that already peaks ≤ 0.95 MUST pass
/// through bit-identical (no coloration, no level change). This is
/// the floor of correctness for any clean playing.
#[test]
fn out_stage_is_transparent_for_safe_signals_at_unity_volume() {
    for (label, sig) in [
        ("sine_0.3", sine(2048, 220.0, 0.3, 48_000.0)),
        ("sine_0.7", sine(2048, 220.0, 0.7, 48_000.0)),
        ("sine_0.9", sine(2048, 220.0, 0.9, 48_000.0)),
        ("pluck_0.5", plucked(2048, 150.0, 0.5, 0.4, 48_000.0)),
        ("dc_0.4", dc(64, 0.4)),
        ("dc_neg_0.7", dc(64, -0.7)),
    ] {
        for (i, &s) in sig.iter().enumerate() {
            let y = out_stage(s, 100.0);
            assert_eq!(y, s, "{label} altered sample {i}: {s} → {y}");
        }
    }
}

/// 125 % volume on a signal that lands ≤ 0.95 after scaling MUST
/// still be bit-identical to `signal * 1.25` — the limiter is only
/// supposed to act ABOVE 0.95, and a hot chain × 1.25 that stays
/// inside has no business being touched.
#[test]
fn out_stage_is_transparent_at_125pct_when_scaled_signal_stays_safe() {
    // 0.7 × 1.25 = 0.875 < 0.95
    let sig = sine(2048, 220.0, 0.7, 48_000.0);
    for &s in &sig {
        let want = s * 1.25;
        let got = out_stage(s, 125.0);
        assert!(
            (got - want).abs() < 1e-6,
            "125% mangled a safe sample: {s} → {got}, wanted {want}"
        );
    }
}

/// Strict monotonicity in volume: per-sample, raising the volume
/// must never lower the magnitude of the output. A failure here
/// means "louder knob → quieter signal" somewhere in the operating
/// range — exactly the "está uma bosta" symptom.
#[test]
fn out_stage_magnitude_is_nondecreasing_in_volume() {
    let volumes = [
        25.0_f32, 50.0, 75.0, 100.0, 110.0, 125.0, 150.0, 200.0, 400.0,
    ];
    let inputs: Vec<f32> = (0..400).map(|i| -1.5 + 3.0 * i as f32 / 399.0).collect();
    for &s in &inputs {
        let mut prev = -1.0_f32;
        for &v in &volumes {
            let mag = out_stage(s, v).abs();
            assert!(
                mag + 1e-6 >= prev,
                "non-monotonic in volume @ s={s:.4}: vol {v}% → mag {mag:.4} < prev {prev:.4}"
            );
            prev = mag;
        }
    }
}

/// Volume = 0 % must give exact silence — never a residual hum,
/// DC offset, or limiter floor.
#[test]
fn out_stage_zero_volume_is_exact_silence() {
    for s in &[0.0, 0.1, 0.5, 0.95, 1.0, 1.5, -1.0, -3.0] {
        assert_eq!(out_stage(*s, 0.0), 0.0, "zero volume leaked s={s}");
    }
}

/// Sign preserved through the whole output stage at any volume.
#[test]
fn out_stage_preserves_sign_at_every_volume() {
    for &v in &[10.0_f32, 100.0, 125.0, 500.0] {
        for &s in &[0.1_f32, 0.5, 0.95, 1.0, 2.0, -0.3, -0.95, -2.0] {
            let y = out_stage(s, v);
            if s.abs() > 0.0 {
                assert!(
                    (s.signum() == y.signum()) || y == 0.0,
                    "sign flipped: s={s}, v={v}, y={y}"
                );
            }
        }
    }
}

/// At a 125 % volume the final output never exceeds full-scale —
/// no matter how hot the chain pushes. (The DAC clips at ±1.0;
/// the limiter is the contract that nothing past it ever does.)
#[test]
fn out_stage_is_bounded_at_125pct_for_every_input() {
    let mut x = -3.0_f32;
    while x <= 3.0 {
        let y = out_stage(x, 125.0);
        assert!(
            y.abs() <= 1.0 && y.is_finite(),
            "out @ 125% leaked past full scale: x={x}, y={y}"
        );
        x += 0.001;
    }
}

/// Same bound at any volume we can plausibly throw at it.
#[test]
fn out_stage_is_bounded_at_any_volume() {
    for &v in &[0.1_f32, 25.0, 100.0, 125.0, 200.0, 1_000.0] {
        for &s in &[-3.0_f32, -1.0, -0.5, 0.0, 0.3, 0.95, 1.0, 1.5, 3.0] {
            let y = out_stage(s, v);
            assert!(
                y.abs() <= 1.0 && y.is_finite(),
                "out leaked: v={v}%, s={s}, y={y}"
            );
        }
    }
}

/// DC at 125 % must not show ringing or asymmetric handling: a
/// constant input must give a constant output (no AC artifact).
#[test]
fn out_stage_dc_at_125pct_is_stable() {
    for &d in &[0.2_f32, 0.5, 0.76, -0.6, 0.0] {
        let v = vec![d; 256];
        let outs: Vec<f32> = v.iter().map(|s| out_stage(*s, 125.0)).collect();
        let first = outs[0];
        for (i, &y) in outs.iter().enumerate() {
            assert!((y - first).abs() < 1e-6, "DC drift at i={i}: {first} → {y}");
        }
    }
}

/// An impulse must come out as a single sample (memoryless limiter)
/// — no ringing, no delayed echo, no tail. Anything else means the
/// stage has state and would add latency / smear transients.
#[test]
fn out_stage_does_not_smear_an_impulse() {
    for &v in &[100.0_f32, 125.0, 200.0] {
        for &amp in &[0.3_f32, 0.8, 1.0, 1.5] {
            let n = 64;
            let inp = impulse(n, amp);
            let outs: Vec<f32> = inp.iter().map(|s| out_stage(*s, v)).collect();
            for (i, &y) in outs.iter().enumerate() {
                if i != n / 2 {
                    assert!(
                        y.abs() < 1e-9,
                        "impulse smeared: v={v}, amp={amp}, i={i}, y={y}"
                    );
                }
            }
        }
    }
}

/// Transients (peak above threshold, body well below) must not
/// see body level pumped/dragged by what the limiter does at the
/// peak: a memoryless limiter affects only the over-threshold
/// samples, body comes through clean.
#[test]
fn out_stage_does_not_pump_body_when_peak_clips() {
    // Body at 0.4, sharp peak at 1.5 in the middle.
    let mut sig = vec![0.4_f32; 256];
    sig[128] = 1.5;
    let outs: Vec<f32> = sig.iter().map(|s| out_stage(*s, 100.0)).collect();
    for (i, &y) in outs.iter().enumerate() {
        if i != 128 {
            assert_eq!(y, 0.4, "body pumped at i={i}: y={y}");
        }
    }
    assert!(
        outs[128].abs() < 1.0 && outs[128] > 0.4,
        "peak not bounded/handled"
    );
}

/// `Sum` mixdown of two unity-peak channels = 2.0 — through the
/// output stage at 100 % this MUST come out bounded.
#[test]
fn sum_mixdown_overflow_is_caught_by_output_stage() {
    let y = out_stage(apply_mixdown(ChainOutputMixdown::Sum, 1.0, 1.0), 100.0);
    assert!(y.abs() <= 1.0 && y.is_finite(), "sum overflow leaked: {y}");
    // And at 125 % too.
    let y = out_stage(apply_mixdown(ChainOutputMixdown::Sum, 1.0, 1.0), 125.0);
    assert!(
        y.abs() <= 1.0 && y.is_finite(),
        "sum overflow @125% leaked: {y}"
    );
}

/// Continuity along the whole real line, fine sweep — no audible
/// step at any input level.
#[test]
fn output_limiter_has_no_step_anywhere_in_the_operating_range() {
    let mut x = -2.0_f32;
    let mut prev = output_limiter(x);
    x += 1.0e-4;
    let mut worst = 0.0_f32;
    while x <= 2.0 {
        let y = output_limiter(x);
        let jump = (y - prev).abs();
        if jump > worst {
            worst = jump;
        }
        // 1e-4 input step ⇒ output step ≤ 1e-4 (slope ≤ 1).
        assert!(
            jump < 2.0e-4,
            "step at x={x}: prev={prev}, now={y}, jump={jump}"
        );
        prev = y;
        x += 1.0e-4;
    }
    eprintln!("output_limiter worst per-step jump over [-2, 2]: {worst:.3e}");
}

/// Symmetry: `f(-x) = -f(x)`. Asymmetric handling would add even
/// harmonics + DC offset on AC signal.
#[test]
fn output_limiter_is_odd_symmetric() {
    let mut x = 0.0_f32;
    while x <= 3.0 {
        let a = output_limiter(x);
        let b = output_limiter(-x);
        assert!(
            (a + b).abs() < 1e-6,
            "asymmetric at x={x}: f(x)={a}, f(-x)={b}"
        );
        x += 0.01;
    }
}

/// Decaying envelope must not be pumped (level riding by limiter
/// state). Per-sample memoryless ⇒ envelope decays smoothly. Uses
/// the bare exp envelope (no carrier) so RMS aliasing against a
/// short window cannot mask the property under test.
#[test]
fn out_stage_decaying_envelope_does_not_pump_at_125pct() {
    let sig: Vec<f32> = (0..1024)
        .map(|i| 0.4 * (-(i as f32 / 48_000.0) / 0.3).exp())
        .collect();
    let mut prev = f32::INFINITY;
    for &s in &sig {
        let y = out_stage(s, 125.0).abs();
        assert!(y <= prev + 1e-6, "envelope pumped: prev={prev}, now={y}");
        prev = y;
    }
}

/// At 125 % a hot sine that already peaks at 0.9 (× 1.25 = 1.125)
/// produces a peak ≤ 1.0 AND an RMS that is GREATER than the same
/// chain at 100 % (louder knob ⇒ louder output, no inversion).
#[test]
fn out_stage_125pct_on_hot_sine_is_louder_than_100pct() {
    let sig = sine(4096, 220.0, 0.9, 48_000.0);
    let at100: Vec<f32> = sig.iter().map(|s| out_stage(*s, 100.0)).collect();
    let at125: Vec<f32> = sig.iter().map(|s| out_stage(*s, 125.0)).collect();
    let (r100, r125) = (rms(&at100), rms(&at125));
    let (p100, p125) = (peak(&at100), peak(&at125));
    eprintln!("hot sine @100%: rms={r100:.4} peak={p100:.4}");
    eprintln!("hot sine @125%: rms={r125:.4} peak={p125:.4}");
    assert!(p125 <= 1.0, "125% peak leaked past full-scale: {p125}");
    assert!(
        r125 > r100,
        "louder knob did not produce more level (rms 125%={r125} ≤ 100%={r100})"
    );
}

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
        assert!(
            (apply_mixdown(ChainOutputMixdown::Average, l, r) - (l + r) * 0.5).abs() < 1e-6
        );
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
                let y =
                    output_limiter(apply_mixdown(ChainOutputMixdown::Sum, l, r) * v / 100.0);
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
                let y = output_limiter(
                    apply_mixdown(ChainOutputMixdown::Average, l, r) * v / 100.0,
                );
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
