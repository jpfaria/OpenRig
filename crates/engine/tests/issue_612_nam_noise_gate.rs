//! Issue #612 — the NAM `noise_gate.enabled` / `noise_gate.threshold_db`
//! knobs must actually gate via the official core's `dsp::noise_gate`.
//! Two contracts through the real bundled model:
//!   1. a steady SUB-threshold input collapses (gated) vs gate off
//!   2. a steady ABOVE-threshold note is NOT strangled (~ gate off)

use std::path::PathBuf;

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use domain::value_objects::ParameterValue;
use plugin_loader::discover;
use plugin_loader::discover::LoadedPackage;

const SR: f32 = 48_000.0;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins")
}

fn load(rel_dir: &str) -> LoadedPackage {
    let root = fixtures_root();
    let target = root.join(rel_dir);
    discover::discover(&root)
        .expect("discover fixture root")
        .into_iter()
        .filter_map(Result::ok)
        .find(|p| p.root == target)
        .unwrap_or_else(|| panic!("fixture package not found: {rel_dir}"))
}

fn run_mono(proc: &mut BlockProcessor, input: &[f32]) -> Vec<f32> {
    let mut buf = input.to_vec();
    match proc {
        BlockProcessor::Mono(m) => m.process_block(&mut buf),
        BlockProcessor::Stereo(_) => panic!("expected mono"),
    }
    buf
}

fn sine(amp: f32, frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|n| amp * (2.0 * std::f32::consts::PI * 110.0 * n as f32 / SR).sin())
        .collect()
}

fn process(gate_enabled: bool, threshold_db: f32, input: &[f32]) -> Vec<f32> {
    nam::register_builder();
    let pkg = load("nam/marshall_plexi");
    let mut params = ParameterSet::default();
    params.insert("preset", ParameterValue::String("angus".into()));
    params.insert("noise_gate.enabled", ParameterValue::Bool(gate_enabled));
    params.insert(
        "noise_gate.threshold_db",
        ParameterValue::Float(threshold_db),
    );
    let mut amp = pkg
        .build_processor(&params, SR, AudioChannelLayout::Mono)
        .expect("build NAM");
    run_mono(&mut amp, input)
}

fn tail_rms(buf: &[f32]) -> f32 {
    let tail = &buf[buf.len() * 3 / 4..];
    (tail.iter().map(|v| v * v).sum::<f32>() / tail.len() as f32).sqrt()
}

#[test]
fn noise_gate_collapses_a_sub_threshold_signal() {
    let input = sine(0.0018, 24_000); // ~-55 dBFS, below a -40 dB threshold
    let gated = tail_rms(&process(true, -40.0, &input));
    let open = tail_rms(&process(false, -40.0, &input));
    assert!(
        gated < open * 0.5,
        "noise gate did not collapse a sub-threshold input: gated tail rms \
         {gated:.3e} vs gate-off {open:.3e} (issue #612)"
    );
}

#[test]
fn noise_gate_does_not_strangle_an_above_threshold_note() {
    let input = sine(0.3, 24_000); // ~-10 dBFS, well above the -40 dB threshold
    let gated = tail_rms(&process(true, -40.0, &input));
    let open = tail_rms(&process(false, -40.0, &input));
    assert!(
        (gated - open).abs() <= open * 0.1,
        "noise gate strangled an above-threshold note: gated {gated:.3e} vs \
         open {open:.3e} (#496 regression guard)"
    );
}
