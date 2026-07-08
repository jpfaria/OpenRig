//! Issue #681 — every native modulation model must *fulfill its proposal*
//! at default knobs, not merely stay finite/bounded. Each test below feeds a
//! deterministic signal through one native and asserts the characteristic
//! signature of that effect (amplitude modulation for tremolo, f±carrier
//! sidebands for ring mod, single-sideband shift for the frequency shifter,
//! a moving comb/all-pass notch for flanger/phaser, a wide stereo image for
//! chorus/Leslie, pitch-only modulation for vibrato). Family tests assert the
//! shipped variants are mutually distinct (#620/#633/#634 pattern).
//!
//! Metrics are spelled out as plain DFT/envelope helpers so the thresholds are
//! auditable. Each test prints its measured value via `eprintln!` to make a
//! RED self-explanatory.

use std::f32::consts::TAU;

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use block_mod::{build_modulation_processor_for_layout, modulation_model_schema};

const SR: f32 = 48_000.0;
const N: usize = 120_000; // 2.5 s — long enough for sub-Hz LFOs
const SKIP: usize = (SR as usize) / 5; // ignore 200 ms warm-up

fn defaults(model: &str) -> ParameterSet {
    let schema = modulation_model_schema(model).expect("schema");
    ParameterSet::default()
        .normalized_against(&schema)
        .expect("normalize defaults")
}

/// Render a mono input through the (MonoToStereo) native, returning (L, R).
fn render(model: &str, params: &ParameterSet, input: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mut proc =
        build_modulation_processor_for_layout(model, params, SR, AudioChannelLayout::Stereo)
            .unwrap_or_else(|e| panic!("build '{model}': {e}"));
    let (mut l, mut r) = (
        Vec::with_capacity(input.len()),
        Vec::with_capacity(input.len()),
    );
    match &mut proc {
        BlockProcessor::Stereo(p) => {
            for &x in input {
                let [a, b] = p.process_frame([x, x]);
                l.push(a);
                r.push(b);
            }
        }
        BlockProcessor::Mono(_) => unreachable!("all mod natives are MonoToStereo"),
    }
    (l, r)
}

fn render_default(model: &str, input: &[f32]) -> (Vec<f32>, Vec<f32>) {
    render(model, &defaults(model), input)
}

fn sine(freq: f32, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.5 * (TAU * freq * i as f32 / SR).sin())
        .collect()
}

/// Deterministic white-ish noise in [-amp, amp] via a small LCG.
fn noise(amp: f32, n: usize) -> Vec<f32> {
    let mut state: u32 = 0x12345678;
    (0..n)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            amp * ((state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0)
        })
        .collect()
}

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// Relative-RMS difference of the steady-state regions (effect vs reference).
fn rel_diff(a: &[f32], b: &[f32]) -> f32 {
    let d: Vec<f32> = a[SKIP..]
        .iter()
        .zip(&b[SKIP..])
        .map(|(x, y)| x - y)
        .collect();
    rms(&d) / rms(&a[SKIP..]).max(1e-9)
}

/// Single-bin magnitude (normalized so a unit-amplitude sine ≈ its amplitude).
fn mag_at(x: &[f32], freq: f32) -> f32 {
    let w = TAU * freq / SR;
    let (mut re, mut im) = (0.0f32, 0.0f32);
    for (n, &s) in x[SKIP..].iter().enumerate() {
        re += s * (w * n as f32).cos();
        im += s * (w * n as f32).sin();
    }
    let len = (x.len() - SKIP) as f32;
    2.0 * (re * re + im * im).sqrt() / len
}

/// Amplitude-modulation depth: (max-min)/(max+min) over 10 ms RMS windows.
fn env_swing(x: &[f32]) -> f32 {
    let win = (SR * 0.01) as usize;
    let mut env: Vec<f32> = Vec::new();
    let mut i = SKIP;
    while i + win <= x.len() {
        env.push(rms(&x[i..i + win]));
        i += win;
    }
    let max = env.iter().cloned().fold(0.0f32, f32::max);
    let min = env.iter().cloned().fold(f32::INFINITY, f32::min);
    if max + min <= 1e-9 {
        0.0
    } else {
        (max - min) / (max + min)
    }
}

fn mag_window(x: &[f32], freq: f32) -> f32 {
    let w = TAU * freq / SR;
    let (mut re, mut im) = (0.0f32, 0.0f32);
    for (n, &s) in x.iter().enumerate() {
        re += s * (w * n as f32).cos();
        im += s * (w * n as f32).sin();
    }
    (re * re + im * im).sqrt() / x.len() as f32
}

/// Cosine DISTANCE between the magnitude spectra of an early vs late window.
/// High ⇒ the filter shape moved over time (flanger/phaser signature).
fn spectral_time_variance(x: &[f32]) -> f32 {
    let probes: Vec<f32> = (0..24)
        .map(|k| 100.0 * (8000.0f32 / 100.0).powf(k as f32 / 23.0))
        .collect();
    let third = (x.len() - SKIP) / 3;
    let wa = &x[SKIP..SKIP + third];
    let wb = &x[SKIP + 2 * third..];
    let va: Vec<f32> = probes.iter().map(|&f| mag_window(wa, f)).collect();
    let vb: Vec<f32> = probes.iter().map(|&f| mag_window(wb, f)).collect();
    let dot: f32 = va.iter().zip(&vb).map(|(a, b)| a * b).sum();
    let na = va.iter().map(|a| a * a).sum::<f32>().sqrt();
    let nb = vb.iter().map(|b| b * b).sum::<f32>().sqrt();
    if na * nb <= 1e-12 {
        0.0
    } else {
        1.0 - dot / (na * nb)
    }
}

fn tone() -> Vec<f32> {
    sine(440.0, N)
}
fn broadband() -> Vec<f32> {
    noise(0.3, N)
}

// ── chorus: wide stereo image + audible thickening ───────────────────────

fn assert_chorus(model: &str, min_decorr: f32) {
    let input = tone();
    let (l, r) = render_default(model, &input);
    let presence = rel_diff(&l, &input);
    let decorr = rel_diff(&l, &r);
    eprintln!("{model}: presence={presence:.3} stereoDecorr={decorr:.3}");
    assert!(
        presence > 0.15,
        "{model} barely alters the dry tone (presence {presence:.3})"
    );
    assert!(
        decorr > min_decorr,
        "{model} stereo image too narrow (decorr {decorr:.3} ≤ {min_decorr}) — a chorus must widen"
    );
}

#[test]
fn classic_chorus_fulfills_chorus_signature() {
    assert_chorus("classic_chorus", 0.20);
}
#[test]
fn ensemble_chorus_fulfills_chorus_signature() {
    assert_chorus("ensemble_chorus", 0.20);
}
#[test]
fn stereo_chorus_fulfills_chorus_signature() {
    assert_chorus("stereo_chorus", 0.20);
}

// ── flanger / phaser: a notch that sweeps over time ──────────────────────

fn assert_swept_notch(model: &str) {
    let input = broadband();
    let (l, _) = render_default(model, &input);
    let presence = rel_diff(&l, &input);
    let stv = spectral_time_variance(&l);
    eprintln!("{model}: presence={presence:.3} specTimeVar={stv:.4}");
    assert!(
        presence > 0.15,
        "{model} barely alters the signal (presence {presence:.3})"
    );
    assert!(
        stv > 0.05,
        "{model} spectrum is static (specTimeVar {stv:.4}) — no sweeping notch"
    );
}

#[test]
fn flanger_classic_fulfills_swept_notch_signature() {
    assert_swept_notch("flanger_classic");
}
#[test]
fn flanger_jet_fulfills_swept_notch_signature() {
    assert_swept_notch("flanger_jet");
}
#[test]
fn flanger_subtle_fulfills_swept_notch_signature() {
    assert_swept_notch("flanger_subtle");
}
#[test]
fn phaser_classic_fulfills_swept_notch_signature() {
    assert_swept_notch("phaser_classic");
}
#[test]
fn phaser_4stage_fulfills_swept_notch_signature() {
    assert_swept_notch("phaser_4stage");
}
#[test]
fn phaser_8stage_fulfills_swept_notch_signature() {
    assert_swept_notch("phaser_8stage");
}

// ── ring modulator: f ± carrier sidebands, carrier suppressed ────────────

#[test]
fn ring_modulator_fulfills_sideband_signature() {
    let carrier = 220.0; // schema default
    let (l, _) = render_default("ring_modulator", &tone());
    let lower = mag_at(&l, 440.0 - carrier);
    let upper = mag_at(&l, 440.0 + carrier);
    let center = mag_at(&l, 440.0);
    eprintln!(
        "ring_modulator: side{:.0}={lower:.3} side{:.0}={upper:.3} carrier440={center:.3}",
        440.0 - carrier,
        440.0 + carrier
    );
    assert!(
        lower > 0.10 && upper > 0.10,
        "ring mod produced no sidebands (lower {lower:.3}, upper {upper:.3})"
    );
    assert!(
        center < 0.5 * lower.min(upper),
        "ring mod did not suppress the carrier (carrier {center:.3} vs sidebands ~{:.3})",
        lower.min(upper)
    );
}

// ── frequency shifter: single-sideband, inharmonic shift ─────────────────

#[test]
fn frequency_shifter_fulfills_single_sideband_signature() {
    let shift = 50.0; // schema default
    let (l, _) = render_default("frequency_shifter", &tone());
    let up = mag_at(&l, 440.0 + shift);
    let down = mag_at(&l, 440.0 - shift);
    let center = mag_at(&l, 440.0);
    let (strong, weak) = if up >= down { (up, down) } else { (down, up) };
    eprintln!("frequency_shifter: up={up:.3} down={down:.3} carrier440={center:.3}");
    assert!(
        strong > 0.15,
        "frequency shifter produced no shifted tone ({strong:.3})"
    );
    assert!(
        center < 0.10,
        "frequency shifter left the original tone in place ({center:.3})"
    );
    assert!(
        strong > 4.0 * weak.max(1e-4),
        "frequency shifter is not single-sideband (strong {strong:.3} vs other {weak:.3})"
    );
}

// ── tremolo: amplitude modulation, carrier preserved ─────────────────────

#[test]
fn tremolo_sine_fulfills_amplitude_modulation_signature() {
    let (l, _) = render_default("tremolo_sine", &tone());
    let swing = env_swing(&l);
    let carrier = mag_at(&l, 440.0);
    eprintln!("tremolo_sine: envSwing={swing:.3} carrier440={carrier:.3}");
    assert!(
        swing > 0.20,
        "tremolo barely modulates amplitude (swing {swing:.3})"
    );
    assert!(
        carrier > 0.15,
        "tremolo destroyed the carrier (mag440 {carrier:.3})"
    );
}

// ── vibrato: pitch-only — modulation present, amplitude flat ─────────────

#[test]
fn vibrato_fulfills_pitch_modulation_signature() {
    let input = tone();
    let (l, _) = render_default("vibrato", &input);
    let swing = env_swing(&l);
    let presence = rel_diff(&l, &input);
    let carrier = mag_at(&l, 440.0); // dry would be ~0.5
    eprintln!("vibrato: envSwing={swing:.3} presence={presence:.3} carrier440={carrier:.3}");
    assert!(
        presence > 0.20,
        "vibrato barely alters the tone (presence {presence:.3})"
    );
    assert!(
        carrier < 0.40,
        "vibrato did not detune — 440 energy still {carrier:.3} (dry ≈ 0.5)"
    );
    assert!(
        swing < 0.15,
        "vibrato is modulating amplitude (swing {swing:.3}) — should be pitch-only"
    );
}

// ── Leslie: amplitude modulation + a wide swirling stereo image ──────────

fn assert_leslie(model: &str) {
    let input = tone();
    let (l, r) = render_default(model, &input);
    let presence = rel_diff(&l, &input);
    let swing = env_swing(&l);
    let decorr = rel_diff(&l, &r);
    eprintln!("{model}: presence={presence:.3} envSwing={swing:.3} stereoDecorr={decorr:.3}");
    assert!(
        presence > 0.15,
        "{model} barely alters the tone (presence {presence:.3})"
    );
    assert!(
        swing > 0.08,
        "{model} has no amplitude swirl (swing {swing:.3})"
    );
    assert!(
        decorr > 0.20,
        "{model} stereo image too narrow (decorr {decorr:.3}) — a rotary speaker must swirl in stereo"
    );
}

#[test]
fn rotary_leslie_fulfills_rotary_signature() {
    assert_leslie("rotary_leslie");
}
#[test]
fn rotary_leslie_studio_fulfills_rotary_signature() {
    assert_leslie("rotary_leslie_studio");
}
#[test]
fn rotary_leslie_vintage_fulfills_rotary_signature() {
    assert_leslie("rotary_leslie_vintage");
}

// ── variant distinctness within a family (same input, shipped defaults) ──

fn assert_family_distinct(models: &[&str], input: &[f32], min_diff: f32) {
    let renders: Vec<(&str, Vec<f32>)> = models
        .iter()
        .map(|m| (*m, render_default(m, input).0))
        .collect();
    for i in 0..renders.len() {
        for j in (i + 1)..renders.len() {
            let diff = rel_diff(&renders[i].1, &renders[j].1);
            eprintln!("{} vs {}: {:.3}", renders[i].0, renders[j].0, diff);
            assert!(
                diff > min_diff,
                "{} and {} are near-identical ({:.1}% < {:.0}%) — redundant variant",
                renders[i].0,
                renders[j].0,
                diff * 100.0,
                min_diff * 100.0
            );
        }
    }
}

#[test]
fn flanger_variants_are_distinct() {
    assert_family_distinct(
        &["flanger_classic", "flanger_jet", "flanger_subtle"],
        &broadband(),
        0.20,
    );
}
#[test]
fn phaser_variants_are_distinct() {
    assert_family_distinct(
        &["phaser_classic", "phaser_4stage", "phaser_8stage"],
        &broadband(),
        0.20,
    );
}
#[test]
fn chorus_variants_are_distinct() {
    assert_family_distinct(
        &["classic_chorus", "ensemble_chorus", "stereo_chorus"],
        &tone(),
        0.20,
    );
}
#[test]
fn leslie_variants_are_distinct() {
    assert_family_distinct(
        &[
            "rotary_leslie",
            "rotary_leslie_studio",
            "rotary_leslie_vintage",
        ],
        &tone(),
        0.20,
    );
}

// ── Fractal-grade quality bar (issue #681 round 2) ───────────────────────

#[test]
fn ensemble_chorus_is_a_wide_image() {
    // A real ensemble (Juno-60 / Solina) spreads its detuned voices into a wide
    // stereo image. It used to average the voices to mono and only offset L/R,
    // collapsing the image to near-mono (decorr 0.49) — narrower than the basic
    // single-voice chorus. A wide ensemble must clear a much higher bar.
    let input = tone();
    let (el, er) = render_default("ensemble_chorus", &input);
    let decorr = rel_diff(&el, &er);
    eprintln!("ensemble decorr={decorr:.3}");
    assert!(
        decorr > 0.70,
        "ensemble stereo image not wide enough (decorr {decorr:.3})"
    );
}
