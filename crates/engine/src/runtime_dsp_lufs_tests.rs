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
                .map(|(k, &f)| {
                    (2.0 * std::f32::consts::PI * f * t).sin() / (k as f32 + 1.0)
                })
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
