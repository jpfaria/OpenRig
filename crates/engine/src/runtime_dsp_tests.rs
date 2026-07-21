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
pub(super) fn out_stage(sample: f32, volume_pct: f32) -> f32 {
    output_limiter(sample * (volume_pct / 100.0))
}

fn peak(s: &[f32]) -> f32 {
    s.iter().fold(0.0_f32, |a, &b| a.max(b.abs()))
}
fn rms(s: &[f32]) -> f32 {
    (s.iter().map(|v| v * v).sum::<f32>() / s.len().max(1) as f32).sqrt()
}

// signal generators --------------------------------------------

pub(super) fn sine(n: usize, freq: f32, amp: f32, sr: f32) -> Vec<f32> {
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
pub(super) fn impulse(n: usize, amp: f32) -> Vec<f32> {
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

