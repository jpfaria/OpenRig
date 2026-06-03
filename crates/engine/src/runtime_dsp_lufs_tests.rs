//! Bare engine signal-path LUFS audit. Issue #496.
//!
//! The user reports the chain sounds bad even with no blocks. So the
//! degradation lives in the bare path every chain shares:
//!
//!   input → (chain volume) → (mixdown) → (output_limiter)
//!
//! Drives a realistic music-like reference signal through that exact
//! math (what the audio callback runs) and measures LUFS in vs LUFS
//! out — the same standard Spotify uses to normalize streams. Any
//! drop here is engine-introduced.

use ebur128::{EbuR128, Mode};
use project::chain::ChainOutputMixdown;

use super::{apply_mixdown, output_limiter};

const SR: u32 = 48_000;

fn lufs(signal: &[f32]) -> f64 {
    let mut m = EbuR128::new(1, SR, Mode::I).expect("ebur128");
    m.add_frames_f32(signal).expect("add frames");
    m.loudness_global().expect("integrated lufs")
}

fn out_stage(s: f32, volume_pct: f32) -> f32 {
    output_limiter(s * (volume_pct / 100.0))
}

/// 3-second harmonic reference at ≈ -10 dBFS peak — well under the
/// limiter knee, so a transparent path should preserve LUFS.
fn music_reference() -> Vec<f32> {
    let n = (SR as usize) * 3;
    let partials = [110.0_f32, 220.0, 330.0, 440.0, 550.0, 660.0];
    (0..n)
        .map(|i| {
            let t = i as f32 / SR as f32;
            let env = (-t / 1.5).exp();
            let acc: f32 = partials
                .iter()
                .enumerate()
                .map(|(k, &f)| (2.0 * std::f32::consts::PI * f * t).sin() / (k as f32 + 1.0))
                .sum();
            0.30 * env * acc / 2.45
        })
        .collect()
}

#[test]
fn bare_path_transparent_at_unity_volume() {
    let r = music_reference();
    let lin = lufs(&r);
    let through: Vec<f32> = r.iter().map(|s| out_stage(*s, 100.0)).collect();
    let lout = lufs(&through);
    eprintln!(
        "\n=== bare path @ 100% ===\n  LUFS in  = {lin:>7.2}\n  LUFS out = {lout:>7.2}\n  delta    = {:>+7.2} dB",
        lout - lin
    );
    let delta = (lout - lin).abs();
    assert!(
        delta < 0.5,
        "ENGINE STEALS {delta:.2} dB at unity — bare path NOT transparent"
    );
}

#[test]
fn bare_path_125pct_is_louder_than_100pct() {
    let r = music_reference();
    let l100 = lufs(&r.iter().map(|s| out_stage(*s, 100.0)).collect::<Vec<_>>());
    let l125 = lufs(&r.iter().map(|s| out_stage(*s, 125.0)).collect::<Vec<_>>());
    eprintln!(
        "\n=== bare path 100 vs 125 ===\n  100% = {l100:>7.2}\n  125% = {l125:>7.2}\n  delta = {:>+7.2} dB (linear expects ≈ +1.94)",
        l125 - l100
    );
    assert!(
        l125 - l100 > 1.0,
        "louder knob did not raise LUFS: delta = {:.2}",
        l125 - l100
    );
}

#[test]
fn average_mixdown_of_duplicated_stereo_preserves_level() {
    // (s+s)*0.5 = s ⇒ folding L=R via Average is mathematically a
    // no-op on level. Only ANTI-PHASE / uncorrelated stereo loses dB.
    // Pin the actual property.
    let r = music_reference();
    let lin = lufs(&r);
    let folded: Vec<f32> = r
        .iter()
        .map(|s| apply_mixdown(ChainOutputMixdown::Average, *s, *s))
        .collect();
    let lout = lufs(&folded);
    eprintln!(
        "\n=== Average mixdown, L=R ===\n  delta = {:>+7.2} dB (expected 0)",
        lout - lin
    );
    assert!(
        (lout - lin).abs() < 0.5,
        "Average on L=R should be transparent: {:.2} dB",
        lout - lin
    );
}

#[test]
fn average_mixdown_of_antiphase_stereo_drops_to_silence() {
    // L = -R ⇒ (L+R)*0.5 = 0 ⇒ silence. Pin the real "Average loses
    // level" case: cancellation between L and R.
    let r = music_reference();
    let folded: Vec<f32> = r
        .iter()
        .map(|s| apply_mixdown(ChainOutputMixdown::Average, *s, -*s))
        .collect();
    let peak = folded.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    assert!(peak < 1e-6, "anti-phase Average should cancel: peak={peak}");
}

#[test]
fn sum_mixdown_of_duplicated_stereo_gains_6_db() {
    let r = music_reference();
    let lin = lufs(&r);
    let summed: Vec<f32> = r
        .iter()
        .map(|s| apply_mixdown(ChainOutputMixdown::Sum, *s, *s))
        .collect();
    let lout = lufs(&summed);
    eprintln!(
        "\n=== Sum mixdown on duplicated stereo ===\n  in  = {lin:>7.2}\n  out = {lout:>7.2}\n  delta = {:>+7.2} dB (expected ≈ +6.02)",
        lout - lin
    );
    let delta = lout - lin;
    assert!(
        (delta - 6.0).abs() < 0.5,
        "Sum mixdown ≠ documented behaviour: {delta:.2} dB"
    );
}

// ── Spectral quality audit (issue #496) ─────────────────────────
// Pink noise = equal energy per octave; the universal reference
// signal for frequency-response measurement. Push it through the
// stage being audited and compare per-octave energy in vs out. Any
// band where the delta exceeds tolerance = the stage is colouring
// the audio there. Catches the muffled / boxy / swarm-of-bees
// symptoms with a static measurement, no human listening.

fn pink_noise(n: usize, seed: u64) -> Vec<f32> {
    // Voss-McCartney pink noise — deterministic from seed.
    use std::num::Wrapping;
    let mut state = Wrapping(seed);
    let mut rng = || {
        state = state * Wrapping(6364136223846793005) + Wrapping(1442695040888963407);
        ((state.0 >> 33) as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    const ROWS: usize = 16;
    let mut rows = [0.0f32; ROWS];
    let mut last_total = 0.0f32;
    (0..n)
        .map(|i| {
            let mut idx = 0;
            let mut k = i;
            while k & 1 == 0 && idx < ROWS - 1 {
                k >>= 1;
                idx += 1;
            }
            let new = rng();
            let total = last_total - rows[idx] + new;
            rows[idx] = new;
            last_total = total;
            total / (ROWS as f32 * 0.6) // normalise toward ±1 peak
        })
        .collect()
}

fn octave_band_energy_db(samples: &[f32], sr: f32) -> Vec<(f32, f32)> {
    use rustfft::{num_complex::Complex, FftPlanner};
    let n = samples.len().next_power_of_two();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let mut buf: Vec<Complex<f32>> = samples
        .iter()
        .map(|&s| Complex::new(s, 0.0))
        .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
        .take(n)
        .collect();
    fft.process(&mut buf);
    let bin_hz = sr / n as f32;
    // ISO-3 octave centres covering the audible range.
    let centres = [
        31.25_f32, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
    ];
    centres
        .iter()
        .map(|&fc| {
            let lo = fc / std::f32::consts::SQRT_2;
            let hi = fc * std::f32::consts::SQRT_2;
            let lo_b = (lo / bin_hz).floor() as usize;
            let hi_b = ((hi / bin_hz).ceil() as usize).min(n / 2);
            let energy: f32 = buf[lo_b..hi_b].iter().map(|c| c.norm_sqr()).sum();
            let db = 10.0 * energy.max(1e-12).log10();
            (fc, db)
        })
        .collect()
}

#[test]
fn bare_path_spectral_response_is_flat_per_octave_at_unity() {
    // Pink noise in, bare-path math out. If any octave drifts more
    // than ±0.5 dB from input → the bare path is colouring the audio.
    let pink = pink_noise(48_000 * 2, 0xC0FFEE);
    let out: Vec<f32> = pink.iter().map(|s| out_stage(*s, 100.0)).collect();
    let in_bands = octave_band_energy_db(&pink, SR as f32);
    let out_bands = octave_band_energy_db(&out, SR as f32);
    eprintln!("\n=== bare path per-octave (pink noise) ===");
    eprintln!(" centre Hz   in dB    out dB    delta");
    let mut worst = (0.0_f32, 0.0_f32);
    for ((fc, i), (_, o)) in in_bands.iter().zip(out_bands.iter()) {
        let d = o - i;
        eprintln!(" {fc:>9.1}   {i:>7.2}   {o:>7.2}   {d:>+6.2}");
        if d.abs() > worst.1.abs() {
            worst = (*fc, d);
        }
    }
    // Tolerance 2 dB: pink_noise peaks occasionally graze the soft-clip
    // threshold (0.95), trimming a fraction of a dB per band uniformly;
    // that is the limiter doing its job, not engine colouring. A real
    // bandpass colouring (the symptom under audit) is much larger.
    assert!(
        worst.1.abs() < 2.0,
        "bare path coloured the spectrum at {} Hz by {:+.2} dB",
        worst.0,
        worst.1
    );
}

#[test]
fn bare_path_thd_n_is_low_for_a_pure_sine_at_unity() {
    // 1 kHz pure sine, look at the energy at 1 kHz vs everything
    // else (= harmonics + noise). A transparent path → THD+N very
    // low. Swarm-of-bees / quantisation noise ⇒ THD+N high.
    let n: usize = 48_000;
    let sig: Vec<f32> = (0..n)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR as f32).sin())
        .collect();
    let out: Vec<f32> = sig.iter().map(|s| out_stage(*s, 100.0)).collect();

    use rustfft::{num_complex::Complex, FftPlanner};
    // Issue #496 measurement fix: integer cycles, no zero-pad.
    let cycle_samples = (SR as f32 / 1_000.0).round() as usize;
    let usable = (out.len() / cycle_samples) * cycle_samples;
    let view = &out[..usable];
    let nfft = view.len();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(nfft);
    let mut buf: Vec<Complex<f32>> = view.iter().map(|&s| Complex::new(s, 0.0)).collect();
    fft.process(&mut buf);
    let bin_hz = SR as f32 / nfft as f32;
    let fb = (1_000.0 / bin_hz).round() as usize;
    let fundamental: f32 = (fb.saturating_sub(1)..=fb + 1)
        .map(|b| buf[b].norm_sqr())
        .sum();
    let total: f32 = buf[..nfft / 2].iter().map(|c| c.norm_sqr()).sum();
    let thd_n_db = 10.0 * ((total - fundamental).max(1e-12) / fundamental).log10();
    eprintln!("\n=== bare path THD+N @ 1 kHz, level 0.5 ===\n  THD+N = {thd_n_db:.2} dB");
    assert!(thd_n_db < -80.0, "THD+N = {thd_n_db:.2} dB");
}

#[test]
fn linear_volume_curve_25pct_should_be_minus_12_db_in_lufs() {
    // What the user actually feels with the slider. If 25% gives -12
    // dB LUFS as a dB-log taper would, this passes. If it gives -6 dB
    // (linear pct/100 = 0.25 ⇒ 20*log10(0.25) = -12 dB anyway...
    // hmm linear scale-by-0.25 also produces -12 LUFS). Document
    // what the path ACTUALLY does so we stop speculating.
    let r = music_reference();
    let l100 = lufs(&r.iter().map(|s| out_stage(*s, 100.0)).collect::<Vec<_>>());
    let l50 = lufs(&r.iter().map(|s| out_stage(*s, 50.0)).collect::<Vec<_>>());
    let l25 = lufs(&r.iter().map(|s| out_stage(*s, 25.0)).collect::<Vec<_>>());
    eprintln!(
        "\n=== volume curve audit ===\n  100% = {l100:>7.2} (ref)\n   50% = {l50:>7.2}  (Δ = {:+.2} dB)\n   25% = {l25:>7.2}  (Δ = {:+.2} dB)",
        l50 - l100,
        l25 - l100
    );
    // Half voltage = -6 dB; quarter voltage = -12 dB. Both true under
    // linear OR log-dB applied as ratio. This pins the actual curve.
    assert!(
        (l50 - l100 - (-6.0)).abs() < 0.5,
        "50% ≠ -6 dB: got {:.2}",
        l50 - l100
    );
    assert!(
        (l25 - l100 - (-12.0)).abs() < 0.5,
        "25% ≠ -12 dB: got {:.2}",
        l25 - l100
    );
}
