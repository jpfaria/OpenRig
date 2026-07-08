//! Issue #620 — native cabinets must each have a distinct, real-cabinet-shaped
//! voice. RED reproduction: today all native CABs share one one-pole engine and
//! differ only 5–9% RMS with the same knobs, so switching the model barely
//! changes the tone. This test asserts an audible, cabinet-scale difference
//! (>20% relative RMS) and a cabinet-shaped high-frequency rolloff per model.

use std::f32::consts::PI;

use block_cab::{build_cab_processor_for_layout, cab_model_schema};
use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};

const SR: f32 = 48_000.0;
const MODELS: [&str; 3] = ["brit_4x12", "vintage_1x12", "american_2x12"];

/// Audible, cabinet-scale voicing difference. Two different cabinets driven with
/// identical knobs must diverge well beyond the current 5–9%.
const MIN_PAIRWISE_DIFF: f32 = 0.20;

fn broadband(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            0.16 * ((2.0 * PI * 110.0 * t).sin()
                + (2.0 * PI * 440.0 * t).sin()
                + (2.0 * PI * 1_000.0 * t).sin()
                + (2.0 * PI * 3_000.0 * t).sin()
                + (2.0 * PI * 6_000.0 * t).sin())
        })
        .collect()
}

fn sine(freq: f32, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.5 * (2.0 * PI * freq * i as f32 / SR).sin())
        .collect()
}

fn render(model: &str, params: &ParameterSet, input: &[f32]) -> Vec<f32> {
    let mut processor = build_cab_processor_for_layout(model, params, SR, AudioChannelLayout::Mono)
        .unwrap_or_else(|e| panic!("build '{model}' failed: {e}"));
    input
        .iter()
        .map(|&x| match &mut processor {
            BlockProcessor::Mono(p) => p.process_sample(x),
            BlockProcessor::Stereo(_) => unreachable!("requested Mono layout"),
        })
        .collect()
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// Relative RMS difference, ignoring the first 100 ms transient.
fn rel_diff(a: &[f32], b: &[f32]) -> f32 {
    let skip = (SR * 0.1) as usize;
    let d: Vec<f32> = a[skip..]
        .iter()
        .zip(&b[skip..])
        .map(|(x, y)| x - y)
        .collect();
    rms(&d) / rms(&a[skip..]).max(1e-9)
}

fn defaults_for(model: &str) -> ParameterSet {
    let schema = cab_model_schema(model).expect("schema");
    ParameterSet::default()
        .normalized_against(&schema)
        .expect("normalize defaults")
}

#[test]
fn native_cabs_are_audibly_distinct_with_same_knobs() {
    let shared = defaults_for("brit_4x12");
    let signal = broadband(SR as usize);
    let renders: Vec<(&str, Vec<f32>)> = MODELS
        .iter()
        .map(|m| (*m, render(m, &shared, &signal)))
        .collect();

    for i in 0..renders.len() {
        for j in (i + 1)..renders.len() {
            let diff = rel_diff(&renders[i].1, &renders[j].1);
            eprintln!("{} vs {}: {:.4}", renders[i].0, renders[j].0, diff);
            assert!(
                diff > MIN_PAIRWISE_DIFF,
                "CAB '{}' vs '{}' differ only {:.1}% with the same knobs — not an \
                 audible cabinet change (need > {:.0}%)",
                renders[i].0,
                renders[j].0,
                diff * 100.0,
                MIN_PAIRWISE_DIFF * 100.0
            );
        }
    }
}

#[test]
fn each_native_cab_has_a_speaker_rolloff() {
    // A real cabinet rolls off hard above ~5 kHz. Each model must attenuate 8 kHz
    // far more than 1 kHz — i.e. behave like a speaker, not a flat wire.
    for model in MODELS {
        let params = defaults_for(model);
        let low = rms(&render(model, &params, &sine(1_000.0, SR as usize))[(SR * 0.1) as usize..]);
        let high = rms(&render(model, &params, &sine(8_000.0, SR as usize))[(SR * 0.1) as usize..]);
        let rolloff_db = 20.0 * (high / low.max(1e-9)).log10();
        eprintln!("{model}: 8kHz vs 1kHz = {rolloff_db:.1} dB");
        assert!(
            rolloff_db < -12.0,
            "CAB '{model}' only rolls off {rolloff_db:.1} dB at 8 kHz vs 1 kHz — \
             not cabinet-shaped (expected < -12 dB)"
        );
    }
}
