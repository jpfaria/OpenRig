//! Behavioral tests for the native reverbs — issue #387.
//!
//! The pre-existing per-module tests only prove "finite, non-zero output".
//! That is not enough: a model can pass them and still be a toy that does not
//! deliver the effect its name promises. These tests assert the **defining
//! property** of each reverb type, measured from its impulse / steady-state
//! response, so that a model which does not fulfil its premise fails loudly.
//!
//! All measurements use the public registry API and the wet signal (mix=100)
//! so the algorithm itself is under test, not the dry blend.

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use block_reverb::{build_reverb_processor_for_layout, reverb_model_schema};
use domain::value_objects::ParameterValue;
use std::f32::consts::TAU;

const SR: f32 = 48_000.0;

// ── harness ─────────────────────────────────────────────────────────

fn build_wet_mono(model: &str, overrides: &[(&str, f32)]) -> BlockProcessor {
    let schema = reverb_model_schema(model).unwrap_or_else(|_| panic!("schema for {model}"));
    let mut ps = ParameterSet::default();
    ps.insert("mix", ParameterValue::Float(100.0));
    for (key, value) in overrides {
        ps.insert(*key, ParameterValue::Float(*value));
    }
    let params = ps
        .normalized_against(&schema)
        .unwrap_or_else(|e| panic!("normalize {model}: {e:?}"));
    build_reverb_processor_for_layout(model, &params, SR, AudioChannelLayout::Mono)
        .unwrap_or_else(|e| panic!("build {model}: {e:?}"))
}

fn run_mono(proc: &mut BlockProcessor, input: &[f32]) -> Vec<f32> {
    match proc {
        BlockProcessor::Mono(p) => input.iter().map(|&x| p.process_sample(x)).collect(),
        _ => panic!("expected a mono processor"),
    }
}

fn ir(model: &str, overrides: &[(&str, f32)], seconds: f32) -> Vec<f32> {
    let n = (seconds * SR) as usize;
    let mut input = vec![0.0f32; n];
    input[0] = 1.0;
    run_mono(&mut build_wet_mono(model, overrides), &input)
}

fn sine(freq: f32, n: usize) -> Vec<f32> {
    (0..n).map(|i| (TAU * freq * i as f32 / SR).sin()).collect()
}

fn at(seconds: f32) -> usize {
    (seconds * SR) as usize
}

// ── measurements ────────────────────────────────────────────────────

fn rms(sig: &[f32]) -> f32 {
    if sig.is_empty() {
        return 0.0;
    }
    (sig.iter().map(|&x| x * x).sum::<f32>() / sig.len() as f32).sqrt()
}

fn rms_blocks(sig: &[f32], win: usize) -> Vec<f32> {
    sig.chunks(win.max(1)).map(rms).collect()
}

/// RT60 in milliseconds via Schroeder backward energy integration.
/// Returns the time for the energy-decay curve to fall 60 dB below its start.
fn rt60_ms(ir: &[f32]) -> f32 {
    let n = ir.len();
    let mut acc = 0.0f64;
    let mut edc = vec![0.0f64; n];
    for i in (0..n).rev() {
        acc += (ir[i] as f64) * (ir[i] as f64);
        edc[i] = acc;
    }
    if edc[0] <= 0.0 {
        return 0.0;
    }
    let threshold = edc[0] * 1e-6; // -60 dB in energy
    for (i, &e) in edc.iter().enumerate() {
        if e <= threshold {
            return i as f32 / SR * 1000.0;
        }
    }
    n as f32 / SR * 1000.0
}

/// Energy at a single frequency via the Goertzel algorithm.
fn goertzel_energy(sig: &[f32], freq: f32) -> f32 {
    let w = TAU * freq / SR;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &x in sig {
        let s0 = x + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0)
}

/// Coefficient of variation of the short-time amplitude envelope.
/// Steady tail → ~0; modulated/beating tail → larger.
fn amplitude_cov(sig: &[f32]) -> f32 {
    let env = rms_blocks(sig, at(0.01));
    if env.is_empty() {
        return 0.0;
    }
    let mean = env.iter().sum::<f32>() / env.len() as f32;
    if mean <= 0.0 {
        return 0.0;
    }
    let var = env.iter().map(|&x| (x - mean) * (x - mean)).sum::<f32>() / env.len() as f32;
    var.sqrt() / mean
}

/// Fraction of samples in the first `window` exceeding 5% of the local peak —
/// a proxy for echo density (diffusion).
fn echo_density(ir: &[f32], window: usize) -> f32 {
    let slice = &ir[..window.min(ir.len())];
    let peak = slice.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
    if peak <= 0.0 {
        return 0.0;
    }
    let count = slice.iter().filter(|&&x| x.abs() > 0.05 * peak).count();
    count as f32 / slice.len() as f32
}

fn one_pole_lowpass(sig: &[f32], cut: f32) -> Vec<f32> {
    let a = (-TAU * cut / SR).exp();
    let mut y = 0.0f32;
    sig.iter()
        .map(|&x| {
            y = a * y + (1.0 - a) * x;
            y
        })
        .collect()
}

fn one_pole_highpass(sig: &[f32], cut: f32) -> Vec<f32> {
    let lp = one_pole_lowpass(sig, cut);
    sig.iter().zip(lp).map(|(&x, l)| x - l).collect()
}

/// Energy-weighted time centroid of a signal, in milliseconds.
fn energy_centroid_ms(sig: &[f32]) -> f32 {
    let mut num = 0.0f64;
    let mut den = 0.0f64;
    for (i, &x) in sig.iter().enumerate() {
        let e = (x as f64) * (x as f64);
        num += e * i as f64;
        den += e;
    }
    if den <= 0.0 {
        0.0
    } else {
        (num / den) as f32 / SR * 1000.0
    }
}

// ── decay-time identity: room < hall < cathedral ───────────────────

#[test]
fn room_decay_sits_in_room_range() {
    let rt = rt60_ms(&ir("room", &[], 8.0));
    assert!(
        (120.0..=1500.0).contains(&rt),
        "room RT60 = {rt:.0} ms is outside a believable room range (120–1500 ms)"
    );
}

#[test]
fn hall_decays_clearly_longer_than_room() {
    let hall = rt60_ms(&ir("hall", &[], 12.0));
    let room = rt60_ms(&ir("room", &[], 12.0));
    assert!(
        hall >= 900.0,
        "hall RT60 = {hall:.0} ms is too short to read as a hall"
    );
    assert!(
        hall > room * 1.3,
        "hall RT60 = {hall:.0} ms must clearly exceed room RT60 = {room:.0} ms"
    );
}

#[test]
fn cathedral_decay_is_very_long() {
    let rt = rt60_ms(&ir("cathedral", &[], 16.0));
    assert!(
        rt >= 3000.0,
        "cathedral RT60 = {rt:.0} ms is far short of a cathedral-sized space (≥3 s)"
    );
}

// ── parameter actually drives the tail length ──────────────────────

#[test]
fn fdn_jot_decay_param_controls_tail_length() {
    let short = rt60_ms(&ir("fdn_jot", &[("decay", 30.0)], 8.0));
    let long = rt60_ms(&ir("fdn_jot", &[("decay", 90.0)], 8.0));
    assert!(
        long > short * 1.5,
        "fdn_jot decay=90 ({long:.0} ms) must far exceed decay=30 ({short:.0} ms)"
    );
}

#[test]
fn freeverb_room_size_controls_tail_length() {
    let small = rt60_ms(&ir("freeverb_canonical", &[("room_size", 20.0)], 8.0));
    let big = rt60_ms(&ir("freeverb_canonical", &[("room_size", 90.0)], 8.0));
    assert!(
        big > small * 1.3,
        "freeverb room_size=90 ({big:.0} ms) must exceed room_size=20 ({small:.0} ms)"
    );
}

// ── diffusion: plates must smear, not echo ─────────────────────────

#[test]
fn dattorro_plate_is_diffuse() {
    let response = ir("dattorro_plate", &[], 1.0);
    let density = echo_density(&response, at(0.1));
    assert!(
        density > 0.3,
        "dattorro_plate echo density = {density:.2} in first 100 ms is too sparse to be a plate"
    );
}

#[test]
fn plate_foundation_is_diffuse() {
    let response = ir("plate_foundation", &[], 1.0);
    let density = echo_density(&response, at(0.1));
    assert!(
        density > 0.3,
        "plate_foundation echo density = {density:.2} in first 100 ms is too sparse to be a plate"
    );
}

// ── shimmer must add an octave-up partial ──────────────────────────

#[test]
fn shimmer_adds_octave_up_content() {
    // Shimmer is a tail effect: play a note, let it ring out, and the +12
    // must come to dominate the surviving tail. Measuring during sustained
    // input would just read the continuously re-injected fundamental.
    let f0 = 220.0;
    let mut input = sine(f0, at(2.5));
    for x in input[at(0.8)..].iter_mut() {
        *x = 0.0;
    }
    let out = run_mono(&mut build_wet_mono("shimmer", &[]), &input);
    let tail = &out[at(1.2)..at(2.4)];
    let fund = goertzel_energy(tail, f0);
    let octave = goertzel_energy(tail, f0 * 2.0);
    let ratio = octave / (fund + 1e-12);
    assert!(
        ratio > 0.2,
        "shimmer octave/fundamental ratio = {ratio:.3} in the ring-out; no audible +12"
    );
}

// ── modulated/lush tail must move; static FDN must not ─────────────

#[test]
fn modulated_lush_tail_is_not_static() {
    let lush = run_mono(&mut build_wet_mono("modulated_lush", &[]), &sine(440.0, at(3.0)));
    let flat = run_mono(&mut build_wet_mono("fdn_jot", &[]), &sine(440.0, at(3.0)));
    let cov_lush = amplitude_cov(&lush[at(1.0)..]);
    let cov_flat = amplitude_cov(&flat[at(1.0)..]);
    assert!(
        cov_lush > cov_flat * 1.5,
        "modulated_lush tail CoV = {cov_lush:.3} is not clearly above static fdn_jot CoV = {cov_flat:.3}"
    );
}

// ── gated: tail must be cut off after the gate closes ──────────────

#[test]
fn gated_tail_collapses_after_release() {
    let on = at(0.4);
    let total = at(2.0);
    let mut input = sine(220.0, total);
    for x in input[on..].iter_mut() {
        *x = 0.0;
    }
    let out = run_mono(&mut build_wet_mono("gated", &[]), &input);
    let during = rms(&out[at(0.15)..at(0.35)]);
    let after = rms(&out[at(1.2)..at(1.4)]);
    assert!(during > 1e-4, "gated produced no in-gate signal to begin with");
    assert!(
        after < during * 0.05,
        "gated tail did not collapse: after={after:.5} vs during={during:.5}"
    );
}

// ── reverse: energy must swell toward the end, not decay ───────────

#[test]
fn reverse_envelope_swells_then_cuts() {
    let response = ir("reverse", &[], 3.0);
    let env = rms_blocks(&response, at(0.02));
    let (peak_idx, _) = env
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .expect("non-empty envelope");
    assert!(
        peak_idx as f32 > env.len() as f32 * 0.4,
        "reverse peak at block {peak_idx}/{} should be late (a swell), not at the start",
        env.len()
    );
}

// ── spring: must be dispersive (frequency-dependent group delay) ───

#[test]
fn spring_parker_is_dispersive() {
    let response = ir("spring_parker_2010", &[], 1.0);
    let low = energy_centroid_ms(&one_pole_lowpass(&response, 800.0));
    let high = energy_centroid_ms(&one_pole_highpass(&response, 3000.0));
    assert!(
        (high - low).abs() > 5.0,
        "spring_parker_2010 band arrival difference = {:.1} ms is too small for a dispersive 'boing'",
        (high - low).abs()
    );
}

#[test]
fn spring_is_dispersive() {
    let response = ir("spring", &[], 1.0);
    let low = energy_centroid_ms(&one_pole_lowpass(&response, 800.0));
    let high = energy_centroid_ms(&one_pole_highpass(&response, 3000.0));
    assert!(
        (high - low).abs() > 5.0,
        "spring band arrival difference = {:.1} ms is too small for a dispersive 'boing'",
        (high - low).abs()
    );
}
